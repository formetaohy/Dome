use crate::bus::{AudioBus, BusState};
use crate::command::{Command, Voice};
use crate::component::{AudioListener, AudioSource};
use crate::device::{Output, Stream};
use crate::garbage;
use crate::graph::{self, BusParams, Layout};
use crate::mixer::{MAX_VOICES, Mixer};
use crate::playback::{Playback, State};
use crate::ring;
use crate::source::Source;
use crate::spatial::{self, Listener};
use dome_ecs::{Entity, World};
use dome_math::Transform;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use crate::device::Unavailable as AudioUnavailable;

const COMMAND_CAPACITY: usize = 8192;
const GARBAGE_CAPACITY: usize = 1024;
const RETRY: Duration = Duration::from_secs(1);
const MISSING_BEATS: u32 = 4;

struct Deck {
    stream: Stream,
    commands: Mutex<ring::Producer<Command>>,
    garbage: garbage::Bin,
    active: Arc<AtomicUsize>,
}

impl Deck {
    fn open(playback: Arc<Playback>) -> Result<Self, AudioUnavailable> {
        let output = Output::open()?;
        let format = output.format();
        let (commands, command_consumer) = ring::channel(COMMAND_CAPACITY);
        let (garbage, bin) = garbage::channel(GARBAGE_CAPACITY);
        let active = Arc::new(AtomicUsize::new(0));
        let mut mixer = Mixer::new(
            format.sample_rate(),
            command_consumer,
            garbage,
            playback,
            Arc::clone(&active),
        );
        let stream = output.start(Box::new(move |out: &mut [f32]| mixer.render(out)))?;
        Ok(Self {
            stream,
            commands: Mutex::new(commands),
            garbage: bin,
            active,
        })
    }

    fn alive(&self) -> bool {
        self.stream.alive()
    }

    fn empty(&mut self) {
        self.garbage.empty();
    }

    fn send(&self, command: Command) -> bool {
        let mut commands = self
            .commands
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        commands.send(command).is_ok()
    }
}

struct Playing {
    voice: Voice,
    state: State,
    position: u64,
    missing: u32,
    sequence: u64,
    generation: u64,
    last: Params,
    bus: String,
    looping: bool,
    start: u64,
    priority: u8,
    name: String,
    source: usize,
    pitch: f32,
}

impl Playing {
    fn plays(&self, source: &AudioSource) -> bool {
        self.rebinds(source) && self.source == source.source().identity()
    }

    fn rebinds(&self, source: &AudioSource) -> bool {
        self.name == source.name()
            && self.generation == source.generation()
            && self.bus == source.bus()
            && self.looping == source.looping()
            && self.priority == source.priority()
            && self.pitch == source.pitch()
    }
}

#[derive(Clone, Copy)]
enum End {
    Played,
    Lost,
}

#[derive(Clone, Copy, PartialEq)]
struct Params {
    gain: f32,
    pan: f32,
    cutoff: f32,
}

pub struct Audio {
    deck: Option<Deck>,
    failure: Option<AudioUnavailable>,
    retry: Instant,
    playback: Arc<Playback>,
    free: Vec<u32>,
    tokens: u32,
    sequences: u64,
    beat: u64,
    voices: BTreeMap<Entity, Playing>,
    suspended: BTreeMap<Entity, (u64, u64)>,
    owners: BTreeMap<u32, Entity>,
    finished: Vec<Entity>,
    master: f32,
    paused: bool,
    sent: Option<(f32, bool)>,
    layout: Arc<Layout>,
    bus_entities: Vec<Entity>,
    buses: Vec<BusParams>,
    scratch: Vec<BusParams>,
    fingerprint: u64,
    pending: Pending,
    revision: u64,
    reopen: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    Synced,
    Layout,
    Params,
}

impl Audio {
    pub fn open() -> Self {
        let mut audio = Self {
            deck: None,
            failure: None,
            retry: Instant::now(),
            playback: Playback::new(MAX_VOICES),
            free: (0..MAX_VOICES as u32).rev().collect(),
            tokens: 1,
            sequences: 0,
            beat: 0,
            voices: BTreeMap::new(),
            suspended: BTreeMap::new(),
            owners: BTreeMap::new(),
            finished: Vec::new(),
            master: 1.0,
            paused: false,
            sent: None,
            layout: Arc::new(Layout::master()),
            bus_entities: Vec::new(),
            buses: vec![BusParams::default()],
            scratch: Vec::new(),
            fingerprint: 0,
            pending: Pending::Synced,
            revision: 0,
            reopen: true,
        };
        audio.acquire();
        audio
    }

    pub fn unavailable(&self) -> Option<&AudioUnavailable> {
        self.failure.as_ref()
    }

    pub fn alive(&self) -> bool {
        self.deck.as_ref().is_some_and(Deck::alive)
    }

    pub fn active(&self) -> usize {
        self.deck
            .as_ref()
            .map_or(0, |deck| deck.active.load(Ordering::Relaxed))
    }

    pub fn master(&self) -> f32 {
        self.master
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn set_master(&mut self, gain: f32) {
        assert!(
            gain.is_finite() && gain >= 0.0,
            "the audio mixer of gain {gain} plays silence or less",
        );
        let gain = gain.min(1.0);
        if self.master == gain {
            return;
        }
        self.master = gain;
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        if self.paused == paused {
            return;
        }
        self.paused = paused;
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn buses(&self) -> impl Iterator<Item = BusState<'_>> + '_ {
        self.layout
            .buses()
            .iter()
            .zip(&self.buses)
            .filter(|(bus, _)| !bus.id().is_empty())
            .map(|(bus, params)| BusState {
                id: bus.id(),
                gain_db: params.gain_db,
                mute: params.mute,
                send_db: params.send_db,
            })
    }

    fn acquire(&mut self) {
        self.retry = Instant::now() + RETRY;
        match Deck::open(Arc::clone(&self.playback)) {
            Ok(deck) => {
                self.deck = Some(deck);
                self.failure = None;
                self.reopen = true;
                self.sent = None;
                self.beat = 0;
            }
            Err(error) => {
                self.deck = None;
                self.failure = Some(error);
            }
        }
    }

    fn supervise(&mut self) {
        if let Some(deck) = &mut self.deck
            && deck.alive()
        {
            deck.empty();
            return;
        }
        if self.deck.is_some() {
            self.deck = None;
            self.playback.release_all();
            self.suspend_all();
            self.retry = Instant::now();
        }
        if Instant::now() < self.retry {
            return;
        }
        self.acquire();
    }

    fn suspend_all(&mut self) {
        let entities = self.voices.keys().copied().collect::<Vec<_>>();
        for entity in entities {
            if let Some(playing) = self.release(entity) {
                self.suspended.insert(
                    entity,
                    (playing.generation, playing.position.max(playing.start)),
                );
            }
        }
    }

    fn sync_mixer(&mut self) {
        let desired = (self.master, self.paused);
        if self.sent == Some(desired) {
            return;
        }
        if self.send(Command::Master { gain: self.master })
            && self.send(Command::Paused {
                paused: self.paused,
            })
        {
            self.sent = Some(desired);
        }
    }

    fn sync_buses(&mut self, world: &World) {
        let fingerprint = graph::fingerprint(world);
        let empty = world.count::<AudioBus>() == 0;
        let mut rebuilt = std::mem::take(&mut self.reopen);
        if empty {
            let synthetic = !self.layout.synthetic();
            if synthetic {
                self.layout = Arc::new(Layout::master());
                self.bus_entities.clear();
            }
            rebuilt |= synthetic;
        } else if self.layout.synthetic() || fingerprint != self.fingerprint {
            let (layout, entities) = graph::compile(world);
            self.layout = Arc::new(layout);
            self.bus_entities = entities;
            rebuilt = true;
        } else {
            graph::entities(world, &self.layout, &mut self.bus_entities);
        }
        self.fingerprint = fingerprint;
        graph::params(world, &self.layout, &self.bus_entities, &mut self.scratch);
        if rebuilt {
            self.pending = Pending::Layout;
        } else if self.pending != Pending::Layout && self.scratch != self.buses {
            self.pending = Pending::Params;
        }
        let command = match self.pending {
            Pending::Synced => return,
            Pending::Layout => Command::Layout {
                layout: Arc::clone(&self.layout),
                params: Arc::new(self.scratch.clone()),
            },
            Pending::Params => Command::Params {
                params: Arc::new(self.scratch.clone()),
            },
        };
        if self.send(command) {
            std::mem::swap(&mut self.scratch, &mut self.buses);
            self.pending = Pending::Synced;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn send(&self, command: Command) -> bool {
        match &self.deck {
            Some(deck) => deck.send(command),
            None => false,
        }
    }

    fn alloc(&mut self) -> Option<u32> {
        self.free.pop()
    }

    fn token(&mut self) -> u32 {
        let token = self.tokens;
        self.tokens = self
            .tokens
            .checked_add(1)
            .expect("the audio mixer does not hand out four billion voices");
        token
    }

    fn victim(&self, priority: u8, gain: f32) -> Option<Entity> {
        let (entity, playing) = self.voices.iter().min_by(|(_, left), (_, right)| {
            left.priority
                .cmp(&right.priority)
                .then_with(|| left.last.gain.total_cmp(&right.last.gain))
                .then_with(|| left.sequence.cmp(&right.sequence))
        })?;
        let worth = playing.priority < priority
            || (playing.priority == priority && playing.last.gain <= gain);
        worth.then_some(*entity)
    }

    fn reserve(&mut self, priority: u8, gain: f32) -> Option<u32> {
        if let Some(slot) = self.alloc() {
            return Some(slot);
        }
        let entity = self.victim(priority, gain)?;
        let slot = self.voices.get(&entity)?.voice.slot;
        let playing = self.detach(entity)?;
        if playing.looping {
            self.suspended.insert(
                entity,
                (playing.generation, playing.position.max(playing.start)),
            );
        } else {
            self.finished.push(entity);
        }
        Some(slot)
    }

    fn start(&mut self, entity: Entity, source: &AudioSource, params: Params, start: u64) -> bool {
        if self.deck.is_none() {
            return false;
        }
        assert!(
            !self.voices.contains_key(&entity),
            "the audio source of entity {} starts a second voice",
            entity.index(),
        );
        if !source.bus().is_empty() && self.layout.index(source.bus()).is_none() {
            panic!(
                "the audio source of entity {} plays through the bus {}, which the mixer does not hold",
                entity.index(),
                source.bus(),
            );
        }
        let Some(slot) = self.reserve(source.priority(), params.gain) else {
            if !source.looping() {
                self.finished.push(entity);
            }
            return false;
        };
        let input = source
            .source()
            .open(source.looping(), start)
            .unwrap_or_else(|error| panic!("{error}"));
        let token = self.token();
        let voice = Voice { slot, token };
        self.playback.begin(slot as usize, token);
        let accepted = self.send(Command::Play {
            voice,
            input,
            bus: (!source.bus().is_empty()).then(|| String::from(source.bus())),
            gain: params.gain,
            pan: params.pan,
            cutoff: params.cutoff,
            looping: source.looping(),
            pitch: source.pitch(),
            start,
        });
        if !accepted {
            self.playback.release(slot as usize, token);
            self.free.push(slot);
            return false;
        }
        let sequence = self.sequences;
        self.sequences = self.sequences.wrapping_add(1);
        self.voices.insert(
            entity,
            Playing {
                voice,
                state: State::Pending,
                position: start,
                missing: 0,
                sequence,
                generation: source.generation(),
                last: params,
                bus: String::from(source.bus()),
                looping: source.looping(),
                start,
                priority: source.priority(),
                name: String::from(source.name()),
                source: source.source().identity(),
                pitch: source.pitch(),
            },
        );
        self.owners.insert(token, entity);
        self.suspended.remove(&entity);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    fn detach(&mut self, entity: Entity) -> Option<Playing> {
        let playing = self.voices.remove(&entity)?;
        self.owners.remove(&playing.voice.token);
        self.playback
            .release(playing.voice.slot as usize, playing.voice.token);
        self.revision = self.revision.wrapping_add(1);
        Some(playing)
    }

    fn release(&mut self, entity: Entity) -> Option<Playing> {
        let playing = self.detach(entity)?;
        self.free.push(playing.voice.slot);
        Some(playing)
    }

    fn stop(&mut self, entity: Entity) -> bool {
        self.suspended.remove(&entity);
        let Some(playing) = self.voices.get(&entity) else {
            return true;
        };
        if !self.send(Command::Stop {
            voice: playing.voice,
        }) {
            return false;
        }
        self.release(entity);
        true
    }

    fn reconcile(&mut self) {
        let beat = self.playback.beat();
        if beat != self.beat {
            self.beat = beat;
            for playing in self.voices.values_mut() {
                playing.missing = playing.missing.saturating_add(1);
            }
        }
        for slot in 0..self.playback.len() {
            let Some(report) = self.playback.report(slot) else {
                continue;
            };
            let Some(entity) = self.owners.get(&report.token).copied() else {
                continue;
            };
            let Some(playing) = self.voices.get_mut(&entity) else {
                continue;
            };
            if playing.voice.slot as usize != slot {
                continue;
            }
            playing.missing = 0;
            playing.position = report.position;
            playing.state = report.state;
        }
        let ended = self
            .voices
            .iter()
            .filter_map(|(entity, playing)| match playing.state {
                State::Finished => Some((*entity, End::Played)),
                _ if playing.missing > MISSING_BEATS => Some((*entity, End::Lost)),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (entity, end) in ended {
            let Some(playing) = self.release(entity) else {
                continue;
            };
            match end {
                End::Played => self.finished.push(entity),
                End::Lost if playing.looping => {
                    self.suspended.insert(
                        entity,
                        (playing.generation, playing.position.max(playing.start)),
                    );
                }
                End::Lost => self.finished.push(entity),
            }
        }
    }

    fn take_finished(&mut self) -> Vec<Entity> {
        std::mem::take(&mut self.finished)
    }

    fn ensure(&mut self, entity: Entity, source: &AudioSource, params: Params) {
        let held = self.voices.get(&entity).map(|playing| {
            (
                playing.voice,
                playing.last,
                playing.plays(source),
                playing.rebinds(source),
                playing.position,
            )
        });
        match held {
            Some((voice, last, true, _, _)) => {
                if last != params {
                    self.update(entity, voice, params);
                }
            }
            Some((voice, _, false, true, position)) => {
                self.exchange(entity, voice, position, source, params);
            }
            Some((_, _, false, false, _)) => {
                if self.stop(entity) {
                    self.start(entity, source, params, 0);
                }
            }
            None => {
                self.start(entity, source, params, 0);
            }
        }
    }

    fn exchange(
        &mut self,
        entity: Entity,
        voice: Voice,
        position: u64,
        source: &AudioSource,
        params: Params,
    ) {
        let seek = match source.source() {
            Source::Decoded(_) => 0,
            Source::Stream(_) => position,
        };
        let input = source
            .source()
            .open(source.looping(), seek)
            .unwrap_or_else(|error| panic!("{error}"));
        if !self.send(Command::Replace { voice, input, seek }) {
            return;
        }
        if let Some(playing) = self.voices.get_mut(&entity) {
            playing.source = source.source().identity();
            playing.position = seek;
        }
        self.update(entity, voice, params);
    }

    fn update(&mut self, entity: Entity, voice: Voice, params: Params) {
        let accepted = self.send(Command::Update {
            voice,
            gain: params.gain,
            pan: params.pan,
            cutoff: params.cutoff,
        });
        if accepted && let Some(playing) = self.voices.get_mut(&entity) {
            playing.last = params;
        }
    }

    fn retain(&mut self, alive: &BTreeSet<Entity>) {
        let stale = self
            .voices
            .keys()
            .copied()
            .filter(|entity| !alive.contains(entity))
            .collect::<Vec<_>>();
        for entity in stale {
            self.stop(entity);
        }
        self.suspended.retain(|entity, _| alive.contains(entity));
    }
}

pub fn sync(world: &mut World) {
    if !world.contains_resource::<Audio>() {
        return;
    }
    let listener = listener(world);
    let Some(mut audio) = world.remove_resource::<Audio>() else {
        return;
    };
    audio.supervise();
    audio.sync_buses(world);
    audio.sync_mixer();
    audio.reconcile();
    for entity in audio.take_finished() {
        if let Some(source) = world.get_mut::<AudioSource>(entity) {
            source.finished();
        }
    }
    let mut gathered = world
        .iter::<AudioSource>()
        .map(|(entity, source)| {
            let position = source.spatial().then(|| {
                world
                    .get::<Transform>(entity)
                    .map(|transform| transform.translation)
                    .unwrap_or_else(|| {
                        panic!(
                            "the spatial audio source of entity {} holds no transform",
                            entity.index(),
                        )
                    })
            });
            (entity, source.clone(), position)
        })
        .collect::<Vec<_>>();
    gathered.sort_by_key(|held| std::cmp::Reverse(held.1.priority()));
    let alive = gathered
        .iter()
        .map(|(entity, ..)| *entity)
        .collect::<BTreeSet<_>>();
    audio.retain(&alive);
    for (entity, source, position) in gathered {
        if !source.is_playing() {
            audio.stop(entity);
            continue;
        }
        let (gain, pan, cutoff) = spatial::place(
            listener.as_ref(),
            position,
            source.gain(),
            source.min_distance(),
            source.max_distance(),
            source.spatial(),
        );
        let params = Params { gain, pan, cutoff };
        let held = audio
            .suspended
            .get(&entity)
            .copied()
            .filter(|(generation, _)| *generation == source.generation());
        if let Some((_, start)) = held {
            audio.start(entity, &source, params, start);
            continue;
        }
        if audio.suspended.remove(&entity).is_some() {
            audio.start(entity, &source, params, 0);
            continue;
        }
        audio.ensure(entity, &source, params);
    }
    world.insert_resource(audio);
}

fn listener(world: &World) -> Option<Listener> {
    let entity = world
        .iter::<AudioListener>()
        .map(|(entity, _)| entity)
        .min()?;
    let transform = world.get::<Transform>(entity).unwrap_or_else(|| {
        panic!(
            "the audio listener of entity {} holds no transform",
            entity.index()
        )
    });
    Some(Listener::of(transform))
}

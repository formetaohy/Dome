use crate::bus::{AudioBus, BusState};
use crate::command::{Command, Event, VoiceId};
use crate::component::{AudioListener, AudioSource};
use crate::device::{Output, Stream};
use crate::garbage;
use crate::graph::{self, BusParams, Layout};
use crate::mixer::{MAX_VOICES, Mixer};
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
const EVENT_CAPACITY: usize = 4096;
const GARBAGE_CAPACITY: usize = 1024;
const RETRY: Duration = Duration::from_secs(1);

struct Deck {
    stream: Stream,
    commands: Mutex<ring::Producer<Command>>,
    events: Mutex<ring::Consumer<Event>>,
    garbage: garbage::Bin,
    active: Arc<AtomicUsize>,
}

impl Deck {
    fn open() -> Result<Self, AudioUnavailable> {
        let output = Output::open()?;
        let format = output.format();
        let (commands, command_consumer) = ring::channel(COMMAND_CAPACITY);
        let (event_producer, events) = ring::channel(EVENT_CAPACITY);
        let (garbage, bin) = garbage::channel(GARBAGE_CAPACITY);
        let active = Arc::new(AtomicUsize::new(0));
        let mut mixer = Mixer::new(
            format.sample_rate(),
            command_consumer,
            event_producer,
            garbage,
            Arc::clone(&active),
        );
        let stream = output.start(Box::new(move |out: &mut [f32]| mixer.render(out)))?;
        Ok(Self {
            stream,
            commands: Mutex::new(commands),
            events: Mutex::new(events),
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

    fn send(&self, command: Command) {
        let mut commands = self
            .commands
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if commands.send(command).is_err() {
            panic!("the audio mixer holds every command the frame sends");
        }
    }

    fn take_events(&self) -> impl Iterator<Item = Event> {
        let mut events = self
            .events
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        std::iter::from_fn(move || events.take())
    }
}

struct Playing {
    voice: u32,
    generation: u64,
    last: Params,
    bus: String,
    looping: bool,
    start: u64,
    priority: u8,
    name: String,
    source: usize,
    pitch: f32,
    exchange: Option<Exchange>,
}

struct Exchange {
    source: usize,
    position: Option<u64>,
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
    voices: BTreeMap<Entity, Playing>,
    suspended: BTreeMap<Entity, (u64, u64)>,
    owners: BTreeMap<u32, Entity>,
    next: u32,
    master: f32,
    paused: bool,
    layout: Arc<Layout>,
    bus_entities: Vec<Entity>,
    buses: Vec<BusParams>,
    scratch: Vec<BusParams>,
    fingerprint: u64,
    revision: u64,
    reopen: bool,
}

impl Audio {
    pub fn open() -> Self {
        let mut audio = Self {
            deck: None,
            failure: None,
            retry: Instant::now(),
            voices: BTreeMap::new(),
            suspended: BTreeMap::new(),
            owners: BTreeMap::new(),
            next: 0,
            master: 1.0,
            paused: false,
            layout: Arc::new(Layout::master()),
            bus_entities: Vec::new(),
            buses: vec![BusParams::default()],
            scratch: Vec::new(),
            fingerprint: 0,
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

    fn lowest_priority(&self) -> Option<u8> {
        self.voices.values().map(|playing| playing.priority).min()
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
        self.send(Command::Master { gain });
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
        self.send(Command::Paused { paused });
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
        match Deck::open() {
            Ok(deck) => {
                deck.send(Command::Master { gain: self.master });
                deck.send(Command::Paused {
                    paused: self.paused,
                });
                self.deck = Some(deck);
                self.failure = None;
                self.reopen = true;
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
        let now = Instant::now();
        if now < self.retry {
            return;
        }
        self.deck = None;
        self.forget();
        self.acquire();
    }

    fn forget(&mut self) {
        if self.voices.is_empty() && self.suspended.is_empty() && self.owners.is_empty() {
            return;
        }
        self.voices.clear();
        self.suspended.clear();
        self.owners.clear();
        self.reopen = true;
        self.revision = self.revision.wrapping_add(1);
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
        if rebuilt {
            self.send(Command::Buses(Arc::clone(&self.layout)));
        }
        graph::params(world, &self.layout, &self.bus_entities, &mut self.scratch);
        if rebuilt || self.scratch != self.buses {
            self.send(Command::Params(Arc::new(self.scratch.clone())));
            std::mem::swap(&mut self.scratch, &mut self.buses);
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn send(&mut self, command: Command) {
        if let Some(deck) = &self.deck {
            deck.send(command);
        }
    }

    fn start(&mut self, entity: Entity, source: &AudioSource, params: Params, start: u64) {
        if self.deck.is_none() {
            return;
        }
        if !source.bus().is_empty() && self.layout.index(source.bus()).is_none() {
            panic!(
                "the audio source of entity {} plays through the bus {}, which the mixer does not hold",
                entity.index(),
                source.bus(),
            );
        }
        let input = source
            .source()
            .open(source.looping(), start)
            .unwrap_or_else(|error| panic!("{error}"));
        let voice = self.next;
        self.next = self.next.wrapping_add(1);
        self.send(Command::Play {
            voice: VoiceId(voice),
            input,
            bus: (!source.bus().is_empty()).then(|| String::from(source.bus())),
            gain: params.gain,
            pan: params.pan,
            cutoff: params.cutoff,
            looping: source.looping(),
            pitch: source.pitch(),
            priority: i32::from(source.priority()),
            start,
        });
        self.voices.insert(
            entity,
            Playing {
                voice,
                generation: source.generation(),
                last: params,
                bus: String::from(source.bus()),
                looping: source.looping(),
                start,
                priority: source.priority(),
                name: String::from(source.name()),
                source: source.source().identity(),
                pitch: source.pitch(),
                exchange: None,
            },
        );
        self.owners.insert(voice, entity);
        self.suspended.remove(&entity);
        self.revision = self.revision.wrapping_add(1);
    }

    fn stop(&mut self, entity: Entity) {
        self.suspended.remove(&entity);
        let Some(playing) = self.voices.remove(&entity) else {
            return;
        };
        self.send(Command::Stop {
            voice: VoiceId(playing.voice),
        });
        self.owners.remove(&playing.voice);
        self.revision = self.revision.wrapping_add(1);
    }

    fn ensure(&mut self, entity: Entity, source: &AudioSource, params: Params) {
        let held = self.voices.get(&entity).map(|playing| {
            (
                playing.voice,
                playing.last,
                playing.plays(source),
                playing.rebinds(source),
            )
        });
        match held {
            Some((voice, last, true, _)) => {
                if last != params {
                    self.update(entity, voice, params);
                }
            }
            Some((voice, _, false, true)) => self.exchange(entity, voice, source, params),
            Some(_) => {
                self.stop(entity);
                self.start(entity, source, params, 0);
            }
            None => self.start(entity, source, params, 0),
        }
    }

    fn exchange(&mut self, entity: Entity, voice: u32, source: &AudioSource, params: Params) {
        let identity = source.source().identity();
        let position = match source.source() {
            Source::Decoded(_) => Some(0),
            Source::Stream(_) => match self.requested(entity, identity) {
                Some(Some(position)) => Some(position),
                Some(None) => None,
                None => {
                    self.playing_mut(entity).exchange = Some(Exchange {
                        source: identity,
                        position: None,
                    });
                    self.send(Command::Locate {
                        voice: VoiceId(voice),
                    });
                    None
                }
            },
        };
        let Some(position) = position else {
            return;
        };
        let input = source
            .source()
            .open(source.looping(), position)
            .unwrap_or_else(|error| panic!("{error}"));
        self.send(Command::Replace {
            voice: VoiceId(voice),
            input,
        });
        let playing = self.playing_mut(entity);
        playing.source = identity;
        playing.exchange = None;
        self.update(entity, voice, params);
    }

    fn requested(&self, entity: Entity, identity: usize) -> Option<Option<u64>> {
        let exchange = self.voices.get(&entity)?.exchange.as_ref()?;
        (exchange.source == identity).then_some(exchange.position)
    }

    fn playing_mut(&mut self, entity: Entity) -> &mut Playing {
        self.voices
            .get_mut(&entity)
            .expect("a playing voice holds its entity")
    }

    fn locate(&mut self, voice: VoiceId, position: u64) {
        let Some(playing) = self
            .voices
            .values_mut()
            .find(|playing| playing.voice == voice.0)
        else {
            return;
        };
        if let Some(exchange) = playing.exchange.as_mut() {
            exchange.position = Some(position);
        }
    }

    fn update(&mut self, entity: Entity, voice: u32, params: Params) {
        self.send(Command::Update {
            voice: VoiceId(voice),
            gain: params.gain,
            pan: params.pan,
            cutoff: params.cutoff,
        });
        if let Some(playing) = self.voices.get_mut(&entity) {
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

    fn take_finished(&mut self) -> Vec<Entity> {
        let Some(deck) = &self.deck else {
            return Vec::new();
        };
        let taken = deck.take_events().collect::<Vec<_>>();
        let mut finished = Vec::new();
        for event in taken {
            self.settle(event, &mut finished);
        }
        if !finished.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
        finished
    }

    fn settle(&mut self, event: Event, finished: &mut Vec<Entity>) {
        let (voice, suspend, position) = match event {
            Event::Located { voice, position } => {
                self.locate(voice, position);
                return;
            }
            Event::Finished(voice) => (voice, false, 0),
            Event::Stolen { voice, position } => (voice, true, position),
            Event::Dropped(voice) => (voice, true, 0),
        };
        let Some(entity) = self.owners.remove(&voice.0) else {
            return;
        };
        if self
            .voices
            .get(&entity)
            .is_none_or(|playing| playing.voice != voice.0)
        {
            return;
        }
        let playing = self
            .voices
            .remove(&entity)
            .expect("an event names the voice of its entity");
        if suspend && playing.looping {
            self.suspended
                .insert(entity, (playing.generation, position.max(playing.start)));
        } else {
            finished.push(entity);
        }
    }
}

pub fn sync(world: &mut World) {
    if !world.contains_resource::<Audio>() {
        return;
    }
    let listener = listener(world);
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
    let Some(mut audio) = world.remove_resource::<Audio>() else {
        return;
    };
    audio.supervise();
    audio.sync_buses(world);
    for entity in audio.take_finished() {
        if let Some(source) = world.get_mut::<AudioSource>(entity) {
            source.finished();
        }
    }
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
            let room = audio.voices.len() < MAX_VOICES;
            let worth = audio
                .lowest_priority()
                .is_some_and(|lowest| lowest < source.priority());
            if room || worth {
                audio.start(entity, &source, params, start);
            }
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

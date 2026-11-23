use crate::bus::{AudioBus, BusState};
use crate::command::{Command, Event, VoiceId};
use crate::component::{AudioListener, AudioSource};
use crate::device::{Output, OutputFormat, Stream};
use crate::graph::{self, BusParams, Layout};
use crate::mixer::{MAX_VOICES, Mixer};
use crate::ring;
use crate::spatial::{self, Listener};
use dome_ecs::{Entity, World};
use dome_math::Transform;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub use crate::device::Unavailable as AudioUnavailable;

const COMMAND_CAPACITY: usize = 8192;
const EVENT_CAPACITY: usize = 4096;

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
    stream: Stream,
    commands: Mutex<ring::Producer<Command>>,
    events: Mutex<ring::Consumer<Event>>,
    voices: BTreeMap<Entity, Playing>,
    suspended: BTreeMap<Entity, (u64, u64)>,
    owners: BTreeMap<u32, Entity>,
    next: u32,
    format: OutputFormat,
    master: f32,
    paused: bool,
    active: Arc<AtomicUsize>,
    layout: Arc<Layout>,
    bus_entities: Vec<Entity>,
    buses: Vec<BusParams>,
    scratch: Vec<BusParams>,
    fingerprint: u64,
    revision: u64,
}

impl Audio {
    pub fn open() -> Result<Self, AudioUnavailable> {
        let output = Output::open()?;
        let format = output.format();
        let (commands, command_consumer) = ring::channel(COMMAND_CAPACITY);
        let (event_producer, events) = ring::channel(EVENT_CAPACITY);
        let active = Arc::new(AtomicUsize::new(0));
        let mut mixer = Mixer::new(
            format.sample_rate(),
            command_consumer,
            event_producer,
            Arc::clone(&active),
        );
        let stream = output.start(Box::new(move |out: &mut [f32]| mixer.render(out)))?;
        Ok(Self {
            stream,
            commands: Mutex::new(commands),
            events: Mutex::new(events),
            voices: BTreeMap::new(),
            suspended: BTreeMap::new(),
            owners: BTreeMap::new(),
            next: 0,
            format,
            master: 1.0,
            paused: false,
            active,
            layout: Arc::new(Layout::master()),
            bus_entities: Vec::new(),
            buses: vec![BusParams::default()],
            scratch: Vec::new(),
            fingerprint: 0,
            revision: 0,
        })
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    pub fn alive(&self) -> bool {
        self.stream.alive()
    }

    pub fn active(&self) -> usize {
        self.active.load(Ordering::Relaxed)
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

    fn sync_buses(&mut self, world: &World) {
        let fingerprint = graph::fingerprint(world);
        let empty = world.count::<AudioBus>() == 0;
        let rebuilt = if empty {
            let synthetic = !self.layout.synthetic();
            if synthetic {
                self.layout = Arc::new(Layout::master());
                self.bus_entities.clear();
            }
            synthetic
        } else if self.layout.synthetic() || fingerprint != self.fingerprint {
            let (layout, entities) = graph::compile(world);
            self.layout = Arc::new(layout);
            self.bus_entities = entities;
            true
        } else {
            graph::entities(world, &self.layout, &mut self.bus_entities);
            false
        };
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
        let mut commands = self
            .commands
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if commands.send(command).is_err() {
            panic!("the audio mixer holds every command the frame sends");
        }
    }

    fn start(&mut self, entity: Entity, source: &AudioSource, params: Params, start: u64) {
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
            Some((voice, _, false, true)) => {
                self.send(Command::Replace {
                    voice: VoiceId(voice),
                    source: source.source().clone(),
                });
                if let Some(playing) = self.voices.get_mut(&entity) {
                    playing.source = source.source().identity();
                }
                self.update(entity, voice, params);
            }
            Some(_) => {
                self.stop(entity);
                self.start(entity, source, params, 0);
            }
            None => self.start(entity, source, params, 0),
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
        let mut finished = Vec::new();
        let mut events = self
            .events
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while let Some(event) = events.take() {
            let (voice, suspend, position) = match event {
                Event::Finished(voice) => (voice, false, 0),
                Event::Stolen { voice, position } => (voice, true, position),
                Event::Dropped(voice) => (voice, true, 0),
            };
            let Some(entity) = self.owners.remove(&voice.0) else {
                continue;
            };
            if self
                .voices
                .get(&entity)
                .is_none_or(|playing| playing.voice != voice.0)
            {
                continue;
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
        if !finished.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
        finished
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
            entity.index(),
        )
    });
    Some(Listener::of(transform))
}

use crate::bus::decibels;
use crate::command::{Command, Event, VoiceId};
use crate::effect::{Compressor, Filter, Reverb};
use crate::graph::{BLOCK_FRAMES, BusParams, Layout, MAX_BUSES};
use crate::ring::{Consumer, Producer};
use crate::voice::{Settings, Voice as Playing};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) const MAX_VOICES: usize = 256;
const RETIRING: usize = 16;

struct Bus {
    buffer: Vec<f32>,
    gain: f32,
    gain_target: f32,
    send: f32,
    send_target: f32,
    params: BusParams,
    filter: Filter,
    compressor: Compressor,
    reverb: Reverb,
}

impl Bus {
    fn new(rate: u32) -> Self {
        Self {
            buffer: vec![0.0; BLOCK_FRAMES * 2],
            gain: 1.0,
            gain_target: 1.0,
            send: 1.0,
            send_target: 1.0,
            params: BusParams::default(),
            filter: Filter::new(),
            compressor: Compressor::new(),
            reverb: Reverb::new(rate),
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.gain = 1.0;
        self.gain_target = 1.0;
        self.send = 1.0;
        self.send_target = 1.0;
        self.params = BusParams::default();
        self.filter.reset();
        self.compressor.reset();
        self.reverb.reset();
    }
}

struct Assigned {
    voice: Playing,
    bus: Option<String>,
    index: usize,
    sequence: u64,
}

pub(crate) struct Mixer {
    rate: u32,
    commands: Consumer<Command>,
    events: Producer<Event>,
    voices: Vec<Option<Assigned>>,
    retiring: [Option<Assigned>; RETIRING],
    buses: Vec<Bus>,
    layout: Arc<Layout>,
    used: usize,
    active: Arc<AtomicUsize>,
    master: f32,
    master_target: f32,
    pause: f32,
    paused: bool,
    sequence: u64,
}

impl Mixer {
    pub(crate) fn new(
        rate: u32,
        commands: Consumer<Command>,
        events: Producer<Event>,
        active: Arc<AtomicUsize>,
    ) -> Self {
        let layout = Arc::new(Layout::master());
        let buses = (0..MAX_BUSES).map(|_| Bus::new(rate)).collect::<Vec<_>>();
        Self {
            rate,
            commands,
            events,
            voices: (0..MAX_VOICES).map(|_| None).collect(),
            retiring: [const { None }; RETIRING],
            buses,
            layout,
            used: 1,
            active,
            master: 1.0,
            master_target: 1.0,
            pause: 1.0,
            paused: false,
            sequence: 0,
        }
    }

    pub(crate) fn render(&mut self, out: &mut [f32]) {
        while let Some(command) = self.commands.take() {
            self.apply(command);
        }
        out.fill(0.0);
        let frames = out.len() / 2;
        if self.paused && self.pause <= 0.0 || frames == 0 {
            self.active
                .store(self.voices.iter().flatten().count(), Ordering::Relaxed);
            return;
        }
        let layout = Arc::clone(&self.layout);
        for chunk in out.chunks_mut(BLOCK_FRAMES * 2) {
            self.render_block(chunk, &layout);
        }
        let step = (self.master_target - self.master) / frames as f32;
        let held = (if self.paused { 0.0 } else { 1.0 } - self.pause) / frames as f32;
        for frame in 0..frames {
            self.master += step;
            self.pause += held;
            let master = self.master.clamp(0.0, 1.0) * self.pause.clamp(0.0, 1.0);
            out[frame * 2] = (out[frame * 2] * master).clamp(-1.0, 1.0);
            out[frame * 2 + 1] = (out[frame * 2 + 1] * master).clamp(-1.0, 1.0);
        }
        self.master = self.master_target;
        self.pause = if self.paused { 0.0 } else { 1.0 };
        self.active
            .store(self.voices.iter().flatten().count(), Ordering::Relaxed);
    }

    fn render_block(&mut self, out: &mut [f32], layout: &Layout) {
        let frames = out.len() / 2;
        let samples = frames * 2;
        for bus in &mut self.buses[..self.used] {
            bus.buffer[..samples].fill(0.0);
        }
        for slot in &mut self.retiring {
            let Some(assigned) = slot else {
                continue;
            };
            let index = assigned.index;
            if assigned
                .voice
                .mix(&mut self.buses[index].buffer[..samples], self.rate)
            {
                *slot = None;
            }
        }
        for slot in &mut self.voices {
            let Some(assigned) = slot else {
                continue;
            };
            let bus = &mut self.buses[assigned.index];
            if assigned.voice.mix(&mut bus.buffer[..samples], self.rate) {
                let id = assigned.voice.id();
                *slot = None;
                let _ = self.events.send(Event::Finished(id));
            }
        }
        for at in 0..layout.order().len() {
            let index = layout.order()[at];
            let mut buffer = std::mem::take(&mut self.buses[index].buffer);
            {
                let bus = &mut self.buses[index];
                let step = (bus.gain_target - bus.gain) / frames as f32;
                for frame in 0..frames {
                    bus.gain += step;
                    let gain = bus.gain.clamp(0.0, 8.0);
                    buffer[frame * 2] *= gain;
                    buffer[frame * 2 + 1] *= gain;
                }
                bus.gain = bus.gain_target;
                bus.filter
                    .process(&bus.params.filter, &mut buffer[..samples], self.rate);
                bus.compressor
                    .process(&bus.params.compressor, &mut buffer[..samples], self.rate);
                bus.reverb
                    .process(&bus.params.reverb, &mut buffer[..samples]);
            }
            if layout.root() == index {
                for (output, sample) in out.iter_mut().zip(&buffer[..samples]) {
                    *output += *sample;
                }
            }
            if let Some(parent) = layout.buses()[index].parent() {
                mix_into(
                    &mut self.buses[parent].buffer[..samples],
                    &buffer[..samples],
                );
            }
            if let Some(send) = layout.buses()[index].send() {
                let start = self.buses[index].send;
                let target = self.buses[index].send_target;
                let step = (target - start) / frames as f32;
                let held = &mut self.buses[send].buffer[..samples];
                for frame in 0..frames {
                    let gain = (start + step * frame as f32).clamp(0.0, 8.0);
                    held[frame * 2] += buffer[frame * 2] * gain;
                    held[frame * 2 + 1] += buffer[frame * 2 + 1] * gain;
                }
                self.buses[index].send = target;
            }
            self.buses[index].buffer = buffer;
        }
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Play {
                voice,
                input,
                bus,
                gain,
                pan,
                cutoff,
                looping,
                pitch,
                priority,
                start,
            } => {
                let settings = Settings {
                    gain,
                    pan,
                    cutoff,
                    looping,
                    pitch,
                    priority,
                    start,
                };
                let Some(index) = self.slot(priority, gain) else {
                    let _ = self.events.send(Event::Dropped(voice));
                    return;
                };
                let bus_index = bus
                    .as_deref()
                    .and_then(|id| self.layout.index(id))
                    .unwrap_or_else(|| self.layout.root());
                let bus_index = bus_index.min(self.used - 1);
                let sequence = self.sequence;
                self.sequence = self.sequence.wrapping_add(1);
                self.voices[index] = Some(Assigned {
                    voice: Playing::new(voice, input, settings, self.rate),
                    bus,
                    index: bus_index,
                    sequence,
                });
            }
            Command::Update {
                voice,
                gain,
                pan,
                cutoff,
            } => {
                if let Some(held) = self.voice_mut(voice) {
                    held.update(gain, pan, cutoff);
                }
            }
            Command::Stop { voice } => {
                if let Some(slot) = self
                    .voices
                    .iter_mut()
                    .find(|slot| slot.as_ref().is_some_and(|held| held.voice.id() == voice))
                {
                    let mut assigned = slot.take().expect("the stopped slot holds its voice");
                    assigned.voice.stop();
                    self.retire(assigned);
                }
            }
            Command::Replace { voice, source } => {
                let rate = self.rate;
                if let Some(held) = self.voice_mut(voice) {
                    let position = held.position();
                    let looping = held.looping();
                    let input = source
                        .open(looping, position)
                        .unwrap_or_else(|error| panic!("{error}"));
                    held.replace(input, rate);
                }
            }
            Command::Buses(layout) => {
                self.used = layout.len();
                for bus in &mut self.buses[..self.used] {
                    bus.reset();
                }
                let used = self.used;
                for assigned in self
                    .voices
                    .iter_mut()
                    .flatten()
                    .chain(self.retiring.iter_mut().flatten())
                {
                    assigned.index = bus_index(assigned.bus.as_deref(), &layout, used);
                }
                self.layout = layout;
            }
            Command::Params(params) => {
                assert_eq!(
                    params.len(),
                    self.used,
                    "the audio mixer holds {} buses for {} parameter sets",
                    self.used,
                    params.len(),
                );
                for (bus, params) in self.buses[..self.used].iter_mut().zip(params.iter()) {
                    bus.params = *params;
                    bus.gain_target = if params.mute {
                        0.0
                    } else {
                        decibels(params.gain_db)
                    };
                    bus.send_target = decibels(params.send_db);
                }
            }
            Command::Master { gain } => {
                self.master_target = if gain.is_finite() {
                    gain.clamp(0.0, 1.0)
                } else {
                    1.0
                };
            }
            Command::Paused { paused } => self.paused = paused,
        }
    }

    fn slot(&mut self, priority: i32, gain: f32) -> Option<usize> {
        if let Some(index) = self.voices.iter().position(Option::is_none) {
            return Some(index);
        }
        let victim = self
            .voices
            .iter()
            .enumerate()
            .filter_map(|(index, assigned)| assigned.as_ref().map(|assigned| (index, assigned)))
            .min_by(|(_, left), (_, right)| {
                left.voice
                    .priority()
                    .cmp(&right.voice.priority())
                    .then_with(|| left.voice.gain().total_cmp(&right.voice.gain()))
                    .then_with(|| left.sequence.cmp(&right.sequence))
            })
            .map(|(index, _)| index)?;
        let assigned = self.voices[victim]
            .as_ref()
            .expect("a victim of voice stealing holds its voice");
        if assigned.voice.priority() > priority
            || (assigned.voice.priority() == priority && assigned.voice.gain() > gain)
        {
            return None;
        }
        let id = assigned.voice.id();
        let position = assigned.voice.position();
        let mut held = self.voices[victim]
            .take()
            .expect("a victim of voice stealing holds its voice");
        held.voice.stop();
        self.retire(held);
        let _ = self.events.send(Event::Stolen {
            voice: id,
            position,
        });
        Some(victim)
    }

    fn retire(&mut self, mut assigned: Assigned) {
        let free = self
            .retiring
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                self.retiring
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, slot)| slot.as_ref().map_or(u64::MAX, |held| held.sequence))
                    .map(|(index, _)| index)
                    .expect("a retiring pool holds the slot it replaces")
            });
        assigned.sequence = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        self.retiring[free] = Some(assigned);
    }

    fn voice_mut(&mut self, voice: VoiceId) -> Option<&mut Playing> {
        self.voices
            .iter_mut()
            .flatten()
            .find(|held| held.voice.id() == voice)
            .map(|held| &mut held.voice)
    }
}

fn bus_index(bus: Option<&str>, layout: &Layout, used: usize) -> usize {
    bus.and_then(|id| layout.index(id))
        .unwrap_or_else(|| layout.root())
        .min(used - 1)
}

fn mix_into(target: &mut [f32], source: &[f32]) {
    for (target, source) in target.iter_mut().zip(source) {
        *target += *source;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::{AudioFilter, FilterKind};
    use crate::graph::BusLink;
    use crate::ring;
    use crate::sound::{Channels, Sound};
    use crate::source::Input;
    use std::sync::Arc;

    fn tone(frames: Vec<f32>) -> Arc<Sound> {
        Arc::new(Sound::new(48_000, Channels::Mono, frames))
    }

    fn command(voice: u32, sound: Arc<Sound>, bus: Option<&str>) -> Command {
        Command::Play {
            voice: VoiceId(voice),
            input: Input::Decoded(sound),
            bus: bus.map(String::from),
            gain: 1.0,
            pan: 0.0,
            cutoff: f32::INFINITY,
            looping: true,
            pitch: 1.0,
            priority: 0,
            start: 0,
        }
    }

    fn buses(buses: &[(&str, Option<&str>, Option<&str>)]) -> Arc<Layout> {
        let links = buses
            .iter()
            .map(|(id, parent, send)| BusLink {
                id,
                parent: *parent,
                send: *send,
            })
            .collect::<Vec<_>>();
        let layout = crate::graph::layout_of(&links).expect("the test buses hold a graph");
        Arc::new(layout)
    }

    #[test]
    fn a_replaced_voice_continues_from_the_frame_it_reached() {
        let (mut producer, command_consumer) = ring::channel(8);
        let (event_producer, _events) = ring::channel(8);
        let active = Arc::new(AtomicUsize::new(0));
        let mut mixer = Mixer::new(
            48_000,
            command_consumer,
            event_producer,
            Arc::clone(&active),
        );
        let ramp = || tone((0..4_800).map(|at| at as f32 / 1_000.0).collect());
        let reloaded = || {
            tone(
                (0..4_800)
                    .map(|at| (at as f32 + 1_000.0) / 2_000.0)
                    .collect(),
            )
        };
        producer
            .send(command(1, ramp(), None))
            .expect("the mixer accepts a voice");
        let mut block = [0.0f32; 512];
        mixer.render(&mut block);
        producer
            .send(Command::Replace {
                voice: VoiceId(1),
                source: crate::source::Source::decoded(reloaded()),
            })
            .expect("the mixer replaces the source of a voice");
        let mut held = [0.0f32; 2];
        mixer.render(&mut held);
        let carried = (256.0 + 1_000.0) / 2_000.0 * std::f32::consts::FRAC_PI_4.cos();
        assert!(
            (held[0] - carried).abs() < 1e-2,
            "a replaced voice continues at {carried} rather than {}",
            held[0],
        );
    }

    #[test]
    fn a_stopped_voice_ramps_out_of_the_mix() {
        let (mut producer, command_consumer) = ring::channel(8);
        let (event_producer, mut events) = ring::channel(8);
        let active = Arc::new(AtomicUsize::new(0));
        let mut mixer = Mixer::new(
            48_000,
            command_consumer,
            event_producer,
            Arc::clone(&active),
        );
        producer
            .send(command(1, tone(vec![1.0; 48_000]), None))
            .expect("the mixer accepts a voice");
        let mut block = [0.0f32; 960];
        mixer.render(&mut block);
        let level = block[958].abs();
        assert!(level > 0.5, "a held voice mixes at {level}");
        producer
            .send(Command::Stop { voice: VoiceId(1) })
            .expect("the mixer stops a voice");
        mixer.render(&mut block);
        assert_eq!(active.load(Ordering::Relaxed), 0);
        assert!(
            block[0].abs() > level * 0.5,
            "a stopped voice steps out of the mix at {} where it played {level}",
            block[0],
        );
        assert!(
            block[958].abs() < 1.0e-4,
            "a stopped voice leaves {} where it fades into silence",
            block[958],
        );
        for pair in block[..956].as_chunks::<2>().0.windows(2) {
            let left = pair[0][0].abs().max(pair[0][1].abs());
            let right = pair[1][0].abs().max(pair[1][1].abs());
            assert!(
                left + 1.0e-6 >= right,
                "a stopped voice ramps down from {left} to {right}",
            );
        }
        assert!(events.take().is_none());
    }

    #[test]
    fn a_mixer_pans_and_finishes_voices() {
        let (commands, command_consumer) = ring::channel(8);
        let (event_producer, mut events) = ring::channel(8);
        let active = Arc::new(AtomicUsize::new(0));
        let mut mixer = Mixer::new(
            48_000,
            command_consumer,
            event_producer,
            Arc::clone(&active),
        );
        let mut producer = commands;
        let mut block = [0.0f32; 960];
        producer
            .send(command(1, tone(vec![1.0; 4_800]), None))
            .expect("the mixer accepts a voice");
        mixer.render(&mut block);
        mixer.render(&mut block);
        assert_eq!(active.load(Ordering::Relaxed), 1);
        assert!((block[958] - 0.707).abs() < 0.01);
        assert!((block[959] - 0.707).abs() < 0.01);
        producer
            .send(Command::Update {
                voice: VoiceId(1),
                gain: 1.0,
                pan: 1.0,
                cutoff: f32::INFINITY,
            })
            .expect("the mixer updates a voice");
        mixer.render(&mut block);
        mixer.render(&mut block);
        assert!(block[958].abs() < 0.01);
        assert!((block[959] - 1.0).abs() < 0.01);
        producer
            .send(Command::Stop { voice: VoiceId(1) })
            .expect("the mixer stops a voice");
        mixer.render(&mut block);
        assert_eq!(active.load(Ordering::Relaxed), 0);
        assert!(events.take().is_none());
        producer
            .send(Command::Play {
                voice: VoiceId(2),
                input: Input::Decoded(tone(vec![1.0; 2])),
                bus: None,
                gain: 1.0,
                pan: 0.0,
                cutoff: f32::INFINITY,
                looping: false,
                pitch: 1.0,
                priority: 0,
                start: 0,
            })
            .expect("the mixer accepts a one shot");
        mixer.render(&mut block);
        assert!(matches!(events.take(), Some(Event::Finished(VoiceId(2)))));
    }

    #[test]
    fn a_mixer_steals_the_quietest_voice_of_the_lowest_priority() {
        let (mut producer, command_consumer) = ring::channel(1024);
        let (event_producer, mut events) = ring::channel(1024);
        let active = Arc::new(AtomicUsize::new(0));
        let mut mixer = Mixer::new(
            48_000,
            command_consumer,
            event_producer,
            Arc::clone(&active),
        );
        for voice in 0..MAX_VOICES as u32 {
            let mut held = command(voice, tone(vec![1.0; 4_800]), None);
            if let Command::Play { priority, gain, .. } = &mut held {
                *priority = 10;
                *gain = 1.0;
            }
            producer.send(held).expect("the mixer accepts its music");
        }
        let mut block = [0.0f32; 960];
        mixer.render(&mut block);
        assert_eq!(active.load(Ordering::Relaxed), MAX_VOICES);
        let mut quiet = command(MAX_VOICES as u32 + 1, tone(vec![1.0; 4_800]), None);
        if let Command::Play { priority, .. } = &mut quiet {
            *priority = 0;
        }
        producer
            .send(quiet)
            .expect("the mixer refuses the quietest voice");
        mixer.render(&mut block);
        assert!(matches!(
            events.take(),
            Some(Event::Dropped(VoiceId(voice))) if voice == MAX_VOICES as u32 + 1
        ));
        let mut loud = command(MAX_VOICES as u32 + 2, tone(vec![1.0; 4_800]), None);
        if let Command::Play { priority, gain, .. } = &mut loud {
            *priority = 20;
            *gain = 1.0;
        }
        producer
            .send(loud)
            .expect("the mixer steals the quietest music");
        mixer.render(&mut block);
        assert!(matches!(
            events.take(),
            Some(Event::Stolen { voice, .. }) if voice == VoiceId(0)
        ));
        assert_eq!(active.load(Ordering::Relaxed), MAX_VOICES);
    }

    #[test]
    fn a_mixer_sends_a_bus_into_a_reverb_tail() {
        let (mut producer, command_consumer) = ring::channel(8);
        let (event_producer, _events) = ring::channel(8);
        let active = Arc::new(AtomicUsize::new(0));
        let mut mixer = Mixer::new(
            48_000,
            command_consumer,
            event_producer,
            Arc::clone(&active),
        );
        let layout = buses(&[
            ("master", None, None),
            ("quiet", Some("master"), Some("hall")),
            ("hall", Some("master"), None),
        ]);
        let mut params = vec![BusParams::default(); 3];
        params[2].reverb = crate::bus::AudioReverb::new(true, 0.9, 0.3, 1.0);
        producer
            .send(Command::Buses(Arc::clone(&layout)))
            .expect("the mixer accepts its layout");
        producer
            .send(Command::Params(Arc::new(params)))
            .expect("the mixer accepts its parameters");
        producer
            .send(Command::Play {
                voice: VoiceId(1),
                input: Input::Decoded(tone(vec![1.0; 64])),
                bus: Some(String::from("quiet")),
                gain: 1.0,
                pan: 0.0,
                cutoff: f32::INFINITY,
                looping: false,
                pitch: 1.0,
                priority: 0,
                start: 0,
            })
            .expect("the mixer accepts a one shot");
        let mut block = [0.0f32; 960];
        for _ in 0..6 {
            mixer.render(&mut block);
        }
        assert!(
            block.iter().any(|sample| sample.abs() > 1e-6),
            "a reverb send rings after its one shot ends",
        );
    }

    #[test]
    fn a_mixer_mixes_buses_through_gain_and_filter() {
        let (mut producer, command_consumer) = ring::channel(8);
        let (event_producer, _events) = ring::channel(8);
        let active = Arc::new(AtomicUsize::new(0));
        let mut mixer = Mixer::new(
            48_000,
            command_consumer,
            event_producer,
            Arc::clone(&active),
        );
        let layout = buses(&[("master", None, None), ("quiet", Some("master"), None)]);
        let mut params = vec![BusParams::default(); 2];
        params[1].gain_db = -12.0;
        params[1].filter = AudioFilter::new(true, FilterKind::LowPass, 200.0, 0.707, 0.0);
        producer
            .send(Command::Buses(Arc::clone(&layout)))
            .expect("the mixer accepts its layout");
        producer
            .send(Command::Params(Arc::new(params.clone())))
            .expect("the mixer accepts its parameters");
        producer
            .send(command(1, tone(vec![1.0; 4_800]), Some("quiet")))
            .expect("the mixer accepts a voice");
        let mut block = [0.0f32; 960];
        for _ in 0..8 {
            mixer.render(&mut block);
        }
        let quiet = (block[958].abs() + block[959].abs()) / 2.0;
        params[1].filter.set_enabled(false);
        params[1].mute = true;
        producer
            .send(Command::Params(Arc::new(params)))
            .expect("the mixer accepts muted parameters");
        for _ in 0..8 {
            mixer.render(&mut block);
        }
        let muted = (block[958].abs() + block[959].abs()) / 2.0;
        assert!(quiet < 0.3, "a quiet bus of {quiet} holds its gain");
        assert!(muted < 0.001, "a muted bus of {muted} holds silence");
    }
}

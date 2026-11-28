use crate::bus::decibels;
use crate::command::{Command, Voice};
use crate::effect::{Compressor, Filter, Reverb};
use crate::garbage::{self, Garbage};
use crate::graph::{BLOCK_FRAMES, BusParams, Layout, MAX_BUSES};
use crate::playback::{Playback, State};
use crate::ring::Consumer;
use crate::voice::{Settings, Voice as Playing};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) const MAX_VOICES: usize = 256;

struct Bus {
    id: Arc<str>,
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
    fn new(rate: u32, id: Arc<str>) -> Self {
        Self {
            id,
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

    fn follow(&mut self, params: BusParams) {
        self.params = params;
        self.gain_target = if params.mute {
            0.0
        } else {
            decibels(params.gain_db)
        };
        self.send_target = decibels(params.send_db);
    }

    fn snap(&mut self, params: BusParams) {
        self.follow(params);
        self.gain = self.gain_target;
        self.send = self.send_target;
    }
}

struct Assigned {
    voice: Voice,
    playing: Playing,
    state: State,
    bus: Option<String>,
    index: usize,
}

impl Assigned {
    fn discard(self) -> Garbage {
        Garbage::Voice {
            input: self.playing.release(),
            bus: self.bus,
        }
    }

    fn holds(&self, voice: Voice) -> bool {
        self.voice.token == voice.token
    }
}

struct Slot {
    active: Option<Assigned>,
    retiring: Option<Assigned>,
}

impl Slot {
    fn new() -> Self {
        Self {
            active: None,
            retiring: None,
        }
    }
}

pub(crate) struct Mixer {
    rate: u32,
    commands: Consumer<Command>,
    garbage: garbage::Queue,
    playback: Arc<Playback>,
    voices: Box<[Slot; MAX_VOICES]>,
    buses: Vec<Bus>,
    spares: Vec<Bus>,
    pool: Vec<Option<Bus>>,
    matched: Vec<Option<usize>>,
    layout: Arc<Layout>,
    active: Arc<AtomicUsize>,
    master: f32,
    master_target: f32,
    pause: f32,
    paused: bool,
}

impl Mixer {
    pub(crate) fn new(
        rate: u32,
        commands: Consumer<Command>,
        garbage: garbage::Queue,
        playback: Arc<Playback>,
        active: Arc<AtomicUsize>,
    ) -> Self {
        let layout = Arc::new(Layout::master());
        let vacant = || Arc::from("");
        let buses = vec![Bus::new(
            rate,
            Arc::clone(&layout.buses()[layout.root()].id),
        )];
        let spares = (1..MAX_BUSES)
            .map(|_| Bus::new(rate, vacant()))
            .collect::<Vec<_>>();
        assert_eq!(
            playback.len(),
            MAX_VOICES,
            "the mixer addresses {} voices where its playback table holds {}",
            MAX_VOICES,
            playback.len(),
        );
        Self {
            rate,
            commands,
            garbage,
            playback,
            voices: Box::new(std::array::from_fn(|_| Slot::new())),
            buses,
            spares,
            pool: Vec::with_capacity(MAX_BUSES),
            matched: Vec::with_capacity(MAX_BUSES),
            layout,
            active,
            master: 1.0,
            master_target: 1.0,
            pause: 1.0,
            paused: false,
        }
    }

    pub(crate) fn render(&mut self, out: &mut [f32]) {
        self.playback.tick();
        self.garbage.flush();
        while let Some(command) = self.commands.take() {
            self.apply(command);
        }
        out.fill(0.0);
        let frames = out.len() / 2;
        let silent = self.paused && self.pause <= 0.0;
        if silent {
            self.drain_retiring();
        }
        if silent || frames == 0 {
            self.republish();
            self.active.store(self.count(), Ordering::Relaxed);
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
        self.active.store(self.count(), Ordering::Relaxed);
    }

    fn count(&self) -> usize {
        self.voices
            .iter()
            .map(|slot| usize::from(slot.active.is_some()) + usize::from(slot.retiring.is_some()))
            .sum()
    }

    fn drain_retiring(&mut self) {
        for slot in self.voices.iter_mut() {
            if let Some(assigned) = slot.retiring.take() {
                self.garbage.discard(assigned.discard());
            }
        }
    }

    fn republish(&mut self) {
        for (at, slot) in self.voices.iter_mut().enumerate() {
            let Some(assigned) = slot.active.as_mut() else {
                continue;
            };
            let position = assigned.playing.position();
            let voice = assigned.voice;
            let state = &mut assigned.state;
            self.playback
                .publish(at, voice.token, state, State::Playing, position);
        }
    }

    fn render_block(&mut self, out: &mut [f32], layout: &Layout) {
        let frames = out.len() / 2;
        let samples = frames * 2;
        for bus in &mut self.buses {
            bus.buffer[..samples].fill(0.0);
        }
        for slot in self.voices.iter_mut() {
            let Some(assigned) = slot.retiring.as_mut() else {
                continue;
            };
            let index = assigned.index;
            let finished = assigned
                .playing
                .mix(&mut self.buses[index].buffer[..samples], self.rate);
            if finished && let Some(assigned) = slot.retiring.take() {
                self.garbage.discard(assigned.discard());
            }
        }
        for (at, slot) in self.voices.iter_mut().enumerate() {
            let Some(assigned) = slot.active.as_mut() else {
                continue;
            };
            let index = assigned.index;
            let finished = assigned
                .playing
                .mix(&mut self.buses[index].buffer[..samples], self.rate);
            let position = assigned.playing.position();
            let voice = assigned.voice;
            let state = &mut assigned.state;
            let next = if finished {
                State::Finished
            } else {
                State::Playing
            };
            self.playback
                .publish(at, voice.token, state, next, position);
            if finished && let Some(assigned) = slot.active.take() {
                self.garbage.discard(assigned.discard());
            }
        }
        for &index in layout.order() {
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

    fn adopt(&mut self, layout: &Arc<Layout>, params: &[BusParams]) {
        let used = layout.len();
        assert_eq!(
            params.len(),
            used,
            "an audio layout of {used} buses carries {} parameter sets",
            params.len(),
        );
        assert!(
            used <= MAX_BUSES,
            "an audio layout of {used} buses outruns the {MAX_BUSES} the mixer holds",
        );
        let mut pool = std::mem::take(&mut self.pool);
        pool.clear();
        let mut buses = std::mem::take(&mut self.buses);
        pool.extend(buses.drain(..).map(Some));
        let mut matched = std::mem::take(&mut self.matched);
        matched.clear();
        matched.resize(used, None);
        for at in 0..used {
            let id = &layout.buses()[at].id;
            matched[at] = pool.iter().enumerate().position(|(index, held)| {
                matched.iter().flatten().all(|taken| *taken != index)
                    && held.as_ref().is_some_and(|bus| bus.id == *id)
            });
        }
        for (index, held) in pool.iter_mut().enumerate() {
            if !matched.contains(&Some(index))
                && let Some(bus) = held.take()
            {
                self.spares.push(bus);
            }
        }
        for at in 0..used {
            let node = &layout.buses()[at];
            let mut bus = match matched[at] {
                Some(index) => pool[index]
                    .take()
                    .expect("a bus migrates to the layout entry that names it"),
                None => {
                    let mut spare = self
                        .spares
                        .pop()
                        .expect("a layout of at most every bus leaves one vacant");
                    spare.reset();
                    spare
                }
            };
            bus.id = Arc::clone(&node.id);
            if matched[at].is_some() {
                bus.follow(params[at]);
            } else {
                bus.snap(params[at]);
            }
            buses.push(bus);
        }
        self.buses = buses;
        self.pool = pool;
        self.matched = matched;
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
                start,
            } => {
                let Some(slot) = slot_of(voice) else {
                    self.garbage.discard(Garbage::Voice { input, bus });
                    return;
                };
                if let Some(held) = self.voices[slot].active.take() {
                    self.retire(held);
                }
                let settings = Settings {
                    gain,
                    pan,
                    cutoff,
                    looping,
                    pitch,
                    start,
                };
                let index = bus_index(bus.as_deref(), &self.layout);
                let mut state = State::Pending;
                self.playback
                    .publish(slot, voice.token, &mut state, State::Playing, start);
                self.voices[slot].active = Some(Assigned {
                    voice,
                    playing: Playing::new(input, settings, self.rate),
                    state,
                    bus,
                    index,
                });
            }
            Command::Update {
                voice,
                gain,
                pan,
                cutoff,
            } => {
                if let Some(held) = self.assigned_mut(voice) {
                    held.playing.update(gain, pan, cutoff);
                }
            }
            Command::Replace { voice, input, seek } => {
                let rate = self.rate;
                let replaced = match self.assigned_mut(voice) {
                    Some(held) => held.playing.replace(input, seek, rate),
                    None => input,
                };
                self.garbage.discard(Garbage::Voice {
                    input: replaced,
                    bus: None,
                });
            }
            Command::Stop { voice } => {
                let Some(slot) = slot_of(voice) else {
                    return;
                };
                let held = self.voices[slot]
                    .active
                    .as_ref()
                    .is_some_and(|held| held.holds(voice));
                if held && let Some(assigned) = self.voices[slot].active.take() {
                    self.retire(assigned);
                }
            }
            Command::Layout { layout, params } => {
                self.adopt(&layout, &params);
                for assigned in self
                    .voices
                    .iter_mut()
                    .flat_map(|slot| slot.active.iter_mut().chain(slot.retiring.iter_mut()))
                {
                    assigned.index = bus_index(assigned.bus.as_deref(), &layout);
                }
                let replaced = std::mem::replace(&mut self.layout, layout);
                self.garbage.discard(Garbage::Layout(replaced));
                self.garbage.discard(Garbage::Params(params));
            }
            Command::Params { params } => {
                for (bus, held) in self.buses.iter_mut().zip(params.iter()) {
                    bus.follow(*held);
                }
                self.garbage.discard(Garbage::Params(params));
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

    fn retire(&mut self, mut assigned: Assigned) {
        assigned.playing.stop();
        assigned.state = State::Fading;
        let Some(slot) = slot_of(assigned.voice) else {
            self.garbage.discard(assigned.discard());
            return;
        };
        if let Some(displaced) = self.voices[slot].retiring.replace(assigned) {
            self.garbage.discard(displaced.discard());
        }
    }

    fn assigned_mut(&mut self, voice: Voice) -> Option<&mut Assigned> {
        self.voices[slot_of(voice)?]
            .active
            .as_mut()
            .filter(|held| held.holds(voice))
    }
}

fn slot_of(voice: Voice) -> Option<usize> {
    let slot = voice.slot as usize;
    (slot < MAX_VOICES).then_some(slot)
}

fn bus_index(bus: Option<&str>, layout: &Layout) -> usize {
    bus.and_then(|id| layout.index(id))
        .unwrap_or_else(|| layout.root())
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

    fn tone(frames: Vec<f32>) -> Arc<Sound> {
        Arc::new(Sound::new(48_000, Channels::Mono, frames))
    }

    fn voice(token: u32) -> Voice {
        Voice { slot: 0, token }
    }

    fn command(token: u32, sound: Arc<Sound>, bus: Option<&str>) -> Command {
        Command::Play {
            voice: voice(token),
            input: Input::Decoded(sound),
            bus: bus.map(String::from),
            gain: 1.0,
            pan: 0.0,
            cutoff: f32::INFINITY,
            looping: true,
            pitch: 1.0,
            start: 0,
        }
    }

    fn buses(links: &[(&str, Option<&str>, Option<&str>)]) -> Arc<Layout> {
        let links = links
            .iter()
            .map(|(id, parent, send)| BusLink {
                id,
                parent: *parent,
                send: *send,
            })
            .collect::<Vec<_>>();
        Arc::new(crate::graph::layout_of(&links).expect("the test buses hold a graph"))
    }

    fn layout_of(layout: Arc<Layout>, params: Vec<BusParams>) -> Command {
        Command::Layout {
            layout,
            params: Arc::new(params),
        }
    }

    fn params_of(params: Vec<BusParams>) -> Command {
        Command::Params {
            params: Arc::new(params),
        }
    }

    fn mixer(
        commands: usize,
    ) -> (
        ring::Producer<Command>,
        Arc<Playback>,
        garbage::Bin,
        Mixer,
        Arc<AtomicUsize>,
    ) {
        let (producer, command_consumer) = ring::channel(commands);
        let active = Arc::new(AtomicUsize::new(0));
        let playback = Playback::new(MAX_VOICES);
        let (garbage, bin) = garbage::channel(64);
        let mixer = Mixer::new(
            48_000,
            command_consumer,
            garbage,
            Arc::clone(&playback),
            Arc::clone(&active),
        );
        (producer, playback, bin, mixer, active)
    }

    fn start(producer: &mut ring::Producer<Command>, playback: &Playback, command: Command) {
        if let Command::Play { voice, .. } = &command {
            playback.begin(voice.slot as usize, voice.token);
        }
        producer.send(command).expect("the mixer accepts a voice");
    }

    #[test]
    fn a_replaced_voice_continues_from_the_frame_it_reported() {
        let (mut producer, playback, _bin, mut mixer, _active) = mixer(64);
        let ramp = || tone((0..4_800).map(|at| at as f32 / 1_000.0).collect());
        let reloaded = || {
            tone(
                (0..4_800)
                    .map(|at| (at as f32 + 1_000.0) / 2_000.0)
                    .collect(),
            )
        };
        start(&mut producer, &playback, command(1, ramp(), None));
        let mut block = [0.0f32; 512];
        mixer.render(&mut block);
        let report = playback
            .report(0)
            .expect("a playing voice reports through its slot");
        assert_eq!(report.state, State::Playing);
        assert_eq!(report.position, 256);
        producer
            .send(Command::Replace {
                voice: voice(1),
                input: Input::Decoded(reloaded()),
                seek: report.position,
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
        let (mut producer, playback, _bin, mut mixer, active) = mixer(64);
        start(
            &mut producer,
            &playback,
            command(1, tone(vec![1.0; 48_000]), None),
        );
        let mut block = [0.0f32; 960];
        mixer.render(&mut block);
        let level = block[958].abs();
        assert!(level > 0.5, "a held voice mixes at {level}");
        producer
            .send(Command::Stop { voice: voice(1) })
            .expect("the mixer stops a voice");
        mixer.render(&mut block);
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
        assert_eq!(active.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_slot_retires_more_voices_than_the_mixer_holds_without_panicking() {
        let (mut producer, playback, mut bin, mut mixer, active) = mixer(MAX_VOICES * 2 + 2);
        let sound = tone(vec![1.0; 48_000]);
        let play = |slot: usize, token: u32| Command::Play {
            voice: Voice {
                slot: slot as u32,
                token,
            },
            input: Input::Decoded(Arc::clone(&sound)),
            bus: None,
            gain: 1.0,
            pan: 0.0,
            cutoff: f32::INFINITY,
            looping: true,
            pitch: 1.0,
            start: 0,
        };
        let fading = |mixer: &Mixer| {
            mixer
                .voices
                .iter()
                .filter(|slot| slot.retiring.is_some())
                .count()
        };
        let mut block = [0.0f32; 960];
        for slot in 0..MAX_VOICES {
            start(&mut producer, &playback, play(slot, 1));
        }
        mixer.render(&mut block);
        assert_eq!(mixer.count(), MAX_VOICES);
        for at in 0..MAX_VOICES * 2 + 1 {
            start(
                &mut producer,
                &playback,
                play(at % MAX_VOICES, at as u32 + 2),
            );
        }
        let mut frame = [0.0f32; 2];
        mixer.render(&mut frame);
        assert_eq!(
            fading(&mixer),
            MAX_VOICES,
            "a callback retires at most one fading voice per slot",
        );
        assert!(
            bin.empty() > 0,
            "a displaced fading voice waits for the thread that releases it",
        );
        mixer.render(&mut block);
        assert_eq!(fading(&mixer), 0);
        assert!(
            block.iter().any(|sample| sample.abs() > 0.1),
            "a block of retired and playing voices renders",
        );
        producer
            .send(Command::Paused { paused: true })
            .expect("the mixer pauses");
        mixer.render(&mut block);
        for _ in 0..4 {
            for slot in 0..MAX_VOICES {
                start(&mut producer, &playback, play(slot, slot as u32 + 2));
            }
            for slot in 0..MAX_VOICES {
                producer
                    .send(Command::Stop {
                        voice: Voice {
                            slot: slot as u32,
                            token: slot as u32 + 2,
                        },
                    })
                    .expect("the mixer stops a paused voice");
            }
            mixer.render(&mut block);
            assert_eq!(
                fading(&mixer),
                0,
                "a paused callback drains the voices it retires",
            );
            assert_eq!(active.load(Ordering::Relaxed), 0);
        }
    }

    #[test]
    fn a_finished_voice_hands_its_status_and_its_frames_to_the_main_thread() {
        let (mut producer, playback, mut bin, mut mixer, _active) = mixer(64);
        start(
            &mut producer,
            &playback,
            Command::Play {
                voice: voice(1),
                input: Input::Decoded(tone(vec![1.0; 4_800])),
                bus: None,
                gain: 1.0,
                pan: 1.0,
                cutoff: f32::INFINITY,
                looping: false,
                pitch: 1.0,
                start: 0,
            },
        );
        let mut block = [0.0f32; 960];
        mixer.render(&mut block);
        assert!(
            block[958].abs() < 0.01 && (block[959] - 1.0).abs() < 0.01,
            "a voice panned right mixes {} and {}",
            block[958],
            block[959],
        );
        for _ in 0..16 {
            mixer.render(&mut block);
        }
        let report = playback
            .report(0)
            .expect("a finished voice holds its report");
        assert_eq!(report.state, State::Finished);
        assert!(
            bin.empty() > 0,
            "a finished voice waits for the thread that releases it",
        );
        mixer.render(&mut block);
        assert_eq!(
            playback.report(0),
            Some(report),
            "a report stays until the main thread releases its slot",
        );
    }

    #[test]
    fn a_mixer_sends_a_bus_into_a_reverb_tail() {
        let (mut producer, _playback, _bin, mut mixer, _active) = mixer(64);
        let layout = buses(&[
            ("master", None, None),
            ("quiet", Some("master"), Some("hall")),
            ("hall", Some("master"), None),
        ]);
        let mut params = vec![BusParams::default(); 3];
        params[2].reverb = crate::bus::AudioReverb::new(true, 0.9, 0.3, 1.0);
        producer
            .send(layout_of(layout, params))
            .expect("the mixer accepts its topology");
        producer
            .send(Command::Play {
                voice: voice(1),
                input: Input::Decoded(tone(vec![1.0; 64])),
                bus: Some(String::from("quiet")),
                gain: 1.0,
                pan: 0.0,
                cutoff: f32::INFINITY,
                looping: false,
                pitch: 1.0,
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
    fn a_new_bus_starts_at_the_gain_its_layout_declares() {
        let (mut producer, playback, _bin, mut mixer, _active) = mixer(64);
        let layout = buses(&[("master", None, None)]);
        let mut params = vec![BusParams::default()];
        params[0].gain_db = -20.0;
        producer
            .send(layout_of(layout, params))
            .expect("the mixer accepts its topology");
        start(
            &mut producer,
            &playback,
            command(1, tone(vec![1.0; 4_800]), None),
        );
        let mut block = [0.0f32; 960];
        mixer.render(&mut block);
        let ceiling = decibels(-20.0) + 1e-4;
        let peak = block
            .iter()
            .fold(0.0f32, |held, sample| held.max(sample.abs()));
        assert!(
            peak <= ceiling,
            "a bus that opens at -20 decibels plays {peak} where {ceiling} stands",
        );
    }

    #[test]
    fn a_mixer_mixes_buses_through_gain_and_filter() {
        let (mut producer, _playback, _bin, mut mixer, _active) = mixer(64);
        let layout = buses(&[("master", None, None), ("quiet", Some("master"), None)]);
        let mut params = vec![BusParams::default(); 2];
        params[1].gain_db = -12.0;
        params[1].filter = AudioFilter::new(true, FilterKind::LowPass, 200.0, 0.707, 0.0);
        producer
            .send(layout_of(Arc::clone(&layout), params.clone()))
            .expect("the mixer accepts its topology");
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
            .send(params_of(params))
            .expect("the mixer accepts muted parameters");
        for _ in 0..8 {
            mixer.render(&mut block);
        }
        let muted = (block[958].abs() + block[959].abs()) / 2.0;
        assert!(quiet < 0.3, "a quiet bus of {quiet} holds its gain");
        assert!(muted < 0.001, "a muted bus of {muted} holds silence");
    }

    #[test]
    fn a_parameter_change_keeps_the_bus_it_speaks_to() {
        let (mut producer, _playback, _bin, mut mixer, _active) = mixer(64);
        let layout = buses(&[("master", None, None), ("quiet", Some("master"), None)]);
        let mut params = vec![BusParams::default(); 2];
        params[1].gain_db = -24.0;
        producer
            .send(layout_of(layout, params.clone()))
            .expect("the mixer accepts its topology");
        producer
            .send(command(1, tone(vec![1.0; 48_000]), Some("quiet")))
            .expect("the mixer accepts a voice");
        let mut block = [0.0f32; 960];
        for _ in 0..8 {
            mixer.render(&mut block);
        }
        let held = (block[958].abs() + block[959].abs()) / 2.0;
        params[1].send_db = -6.0;
        producer
            .send(params_of(params))
            .expect("the mixer accepts a send change");
        mixer.render(&mut block);
        let stepped = (block[0].abs() + block[1].abs()) / 2.0;
        assert!(
            stepped <= held * 1.5,
            "a bus of {held} keeps the level a send change leaves at {stepped}",
        );
    }
}

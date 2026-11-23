use crate::bus::{AudioCompressor, AudioFilter, AudioReverb, FilterKind, decibels};
use std::f32::consts::TAU;

pub(crate) struct Filter {
    states: [[f32; 2]; 2],
}

impl Filter {
    pub(crate) fn new() -> Self {
        Self {
            states: [[0.0; 2]; 2],
        }
    }

    pub(crate) fn reset(&mut self) {
        self.states = [[0.0; 2]; 2];
    }

    pub(crate) fn process(&mut self, settings: &AudioFilter, buffer: &mut [f32], rate: u32) {
        if !settings.enabled() || buffer.is_empty() {
            return;
        }
        let coefficients = Coefficients::of(settings, rate);
        for [left, right] in buffer.as_chunks_mut::<2>().0 {
            *left = self.sample(0, *left, &coefficients);
            *right = self.sample(1, *right, &coefficients);
        }
    }

    fn sample(&mut self, channel: usize, input: f32, coefficients: &Coefficients) -> f32 {
        let state = &mut self.states[channel];
        let output = coefficients.b0 * input + state[0];
        state[0] = coefficients.b1 * input - coefficients.a1 * output + state[1];
        state[1] = coefficients.b2 * input - coefficients.a2 * output;
        output
    }
}

struct Coefficients {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Coefficients {
    fn of(settings: &AudioFilter, rate: u32) -> Self {
        let frequency = settings.frequency().min(rate as f32 * 0.45).max(20.0);
        let q = settings.q();
        let omega = TAU * frequency / rate as f32;
        let cos = omega.cos();
        let sin = omega.sin();
        let alpha = sin / (2.0 * q);
        let a = 10.0f32.powf(settings.gain_db() / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match settings.kind() {
            FilterKind::LowPass => (
                (1.0 - cos) / 2.0,
                1.0 - cos,
                (1.0 - cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            FilterKind::HighPass => (
                (1.0 + cos) / 2.0,
                -(1.0 + cos),
                (1.0 + cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            FilterKind::BandPass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            FilterKind::Peaking => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            FilterKind::LowShelf => {
                let root = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + root),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - root),
                    (a + 1.0) + (a - 1.0) * cos + root,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - root,
                )
            }
            FilterKind::HighShelf => {
                let root = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + root),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - root),
                    (a + 1.0) - (a - 1.0) * cos + root,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - root,
                )
            }
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }
}

pub(crate) struct Compressor {
    envelope: f32,
}

impl Compressor {
    pub(crate) fn new() -> Self {
        Self { envelope: 0.0 }
    }

    pub(crate) fn reset(&mut self) {
        self.envelope = 0.0;
    }

    pub(crate) fn process(&mut self, settings: &AudioCompressor, buffer: &mut [f32], rate: u32) {
        if !settings.enabled() || buffer.is_empty() {
            return;
        }
        let threshold = decibels(settings.threshold_db());
        let makeup = decibels(settings.makeup_db());
        let attack = coefficient(settings.attack_ms(), rate);
        let release = coefficient(settings.release_ms(), rate);
        let exponent = 1.0 / settings.ratio() - 1.0;
        for [left, right] in buffer.as_chunks_mut::<2>().0 {
            let peak = left.abs().max(right.abs());
            let coefficient = if peak > self.envelope {
                attack
            } else {
                release
            };
            self.envelope = coefficient * self.envelope + (1.0 - coefficient) * peak;
            let over = self.envelope / threshold;
            let gain = if over > 1.0 {
                over.powf(exponent) * makeup
            } else {
                makeup
            };
            *left *= gain;
            *right *= gain;
        }
    }
}

fn coefficient(milliseconds: f32, rate: u32) -> f32 {
    (-1.0 / (milliseconds * 0.001 * rate as f32)).exp()
}

const COMB: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;
const INPUT_GAIN: f32 = 0.15;

struct Comb {
    buffer: Vec<f32>,
    cursor: usize,
    store: f32,
}

impl Comb {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length],
            cursor: 0,
            store: 0.0,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.cursor = 0;
        self.store = 0.0;
    }

    fn process(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let output = self.buffer[self.cursor];
        self.store = output * (1.0 - damping) + self.store * damping;
        self.buffer[self.cursor] = input + self.store * feedback;
        self.cursor = (self.cursor + 1) % self.buffer.len();
        output
    }
}

struct Allpass {
    buffer: Vec<f32>,
    cursor: usize,
}

impl Allpass {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length],
            cursor: 0,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.cursor = 0;
    }

    fn process(&mut self, input: f32) -> f32 {
        let stored = self.buffer[self.cursor];
        let output = -input + stored;
        self.buffer[self.cursor] = input + stored * 0.5;
        self.cursor = (self.cursor + 1) % self.buffer.len();
        output
    }
}

pub(crate) struct Reverb {
    combs: [[Comb; 8]; 2],
    allpasses: [[Allpass; 4]; 2],
}

impl Reverb {
    pub(crate) fn new(rate: u32) -> Self {
        let scale = rate as f32 / 44_100.0;
        let length = |samples: usize, spread: usize| {
            (((samples + spread) as f32) * scale).round().max(1.0) as usize
        };
        Self {
            combs: std::array::from_fn(|channel| {
                std::array::from_fn(|at| {
                    Comb::new(length(COMB[at], if channel == 1 { SPREAD } else { 0 }))
                })
            }),
            allpasses: std::array::from_fn(|channel| {
                std::array::from_fn(|at| {
                    Allpass::new(length(ALLPASS[at], if channel == 1 { SPREAD } else { 0 }))
                })
            }),
        }
    }

    pub(crate) fn reset(&mut self) {
        for channel in &mut self.combs {
            for comb in channel {
                comb.reset();
            }
        }
        for channel in &mut self.allpasses {
            for allpass in channel {
                allpass.reset();
            }
        }
    }

    pub(crate) fn process(&mut self, settings: &AudioReverb, buffer: &mut [f32]) {
        if !settings.enabled() || buffer.is_empty() {
            return;
        }
        let feedback = 0.28 + settings.room() * 0.7;
        let damping = settings.damping() * 0.4;
        let mix = settings.mix();
        for [left, right] in buffer.as_chunks_mut::<2>().0 {
            let wet_left = self.wet(0, *left, feedback, damping);
            let wet_right = self.wet(1, *right, feedback, damping);
            *left = *left * (1.0 - mix) + wet_left * mix;
            *right = *right * (1.0 - mix) + wet_right * mix;
        }
    }

    fn wet(&mut self, channel: usize, input: f32, feedback: f32, damping: f32) -> f32 {
        let mut wet = 0.0;
        for comb in &mut self.combs[channel] {
            wet += comb.process(input * INPUT_GAIN, feedback, damping);
        }
        for allpass in &mut self.allpasses[channel] {
            wet = allpass.process(wet);
        }
        wet
    }
}

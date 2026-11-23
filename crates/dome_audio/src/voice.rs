use crate::command::VoiceId;
use crate::sound::Channels;
use crate::source::Input;
use std::f32::consts::{FRAC_PI_4, TAU};

const MIN_PITCH: f32 = 0.01;
const MAX_PITCH: f32 = 16.0;
const MIN_GAIN: f32 = 0.0;
const MAX_GAIN: f32 = 8.0;
const FADE_STEP: f32 = 1.0 / 256.0;

pub(crate) struct Settings {
    pub(crate) gain: f32,
    pub(crate) pan: f32,
    pub(crate) cutoff: f32,
    pub(crate) looping: bool,
    pub(crate) pitch: f32,
    pub(crate) priority: i32,
    pub(crate) start: u64,
}

pub(crate) struct Voice {
    id: VoiceId,
    input: Input,
    cursor: f64,
    step: f64,
    pitch: f32,
    looping: bool,
    priority: i32,
    gain: f32,
    pan: f32,
    cutoff: f32,
    alpha: f32,
    states: [f32; 2],
    left: f32,
    right: f32,
    fade: f32,
    fading: bool,
}

impl Voice {
    pub(crate) fn new(id: VoiceId, input: Input, settings: Settings, rate: u32) -> Self {
        let pitch = if settings.pitch.is_finite() {
            settings.pitch.clamp(MIN_PITCH, MAX_PITCH)
        } else {
            1.0
        };
        let step = f64::from(input.sample_rate()) / f64::from(rate) * f64::from(pitch);
        Self {
            id,
            input,
            cursor: settings.start as f64,
            step,
            pitch,
            looping: settings.looping,
            priority: settings.priority,
            gain: finite(settings.gain, MIN_GAIN, MAX_GAIN, 0.0),
            pan: finite(settings.pan, -1.0, 1.0, 0.0),
            cutoff: settings.cutoff,
            alpha: 1.0,
            states: [0.0; 2],
            left: 0.0,
            right: 0.0,
            fade: 1.0,
            fading: false,
        }
    }

    pub(crate) fn replace(&mut self, input: Input, rate: u32) {
        self.step = f64::from(input.sample_rate()) / f64::from(rate) * f64::from(self.pitch);
        self.input = input;
    }

    pub(crate) fn looping(&self) -> bool {
        self.looping
    }

    pub(crate) fn stop(&mut self) {
        self.fading = true;
    }

    pub(crate) fn id(&self) -> VoiceId {
        self.id
    }

    pub(crate) fn priority(&self) -> i32 {
        self.priority
    }

    pub(crate) fn gain(&self) -> f32 {
        self.gain
    }

    pub(crate) fn position(&self) -> u64 {
        self.input.playhead(self.cursor)
    }

    pub(crate) fn update(&mut self, gain: f32, pan: f32, cutoff: f32) {
        self.gain = finite(gain, MIN_GAIN, MAX_GAIN, 0.0);
        self.pan = finite(pan, -1.0, 1.0, 0.0);
        self.cutoff = cutoff;
    }

    pub(crate) fn mix(&mut self, out: &mut [f32], rate: u32) -> bool {
        let frames = out.len() / 2;
        if frames == 0 {
            return false;
        }
        let (target_left, target_right) = self.targets();
        let left_step = (target_left - self.left) / frames as f32;
        let right_step = (target_right - self.right) / frames as f32;
        self.alpha = if self.cutoff.is_finite() && self.cutoff > 0.0 {
            let alpha = 1.0 - (-TAU * self.cutoff / rate as f32).exp();
            alpha.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let mut finished = false;
        let mut fade = self.fade;
        for frame in 0..frames {
            if self.input.finished(self.cursor, self.looping) {
                finished = true;
                break;
            }
            let held = self.input.sample(self.cursor, self.looping);
            self.states[0] += self.alpha * (held[0] - self.states[0]);
            self.states[1] += self.alpha * (held[1] - self.states[1]);
            self.left += left_step;
            self.right += right_step;
            out[frame * 2] += self.states[0] * self.left * fade;
            out[frame * 2 + 1] += self.states[1] * self.right * fade;
            self.cursor += self.step;
            if self.fading {
                fade -= FADE_STEP;
                if fade <= 0.0 {
                    fade = 0.0;
                    finished = true;
                    break;
                }
            }
            if let Some(count) = self.loop_frames()
                && self.cursor >= count as f64
            {
                self.cursor = self.cursor.rem_euclid(count as f64);
            }
        }
        self.fade = fade;
        self.left = target_left;
        self.right = target_right;
        finished
    }

    fn loop_frames(&self) -> Option<u64> {
        if self.looping {
            self.input.decoded_frames()
        } else {
            None
        }
    }

    fn targets(&self) -> (f32, f32) {
        match self.input.channels() {
            Channels::Mono => {
                let angle = (self.pan + 1.0) * FRAC_PI_4;
                (self.gain * angle.cos(), self.gain * angle.sin())
            }
            Channels::Stereo => (
                self.gain * (1.0 - self.pan).min(1.0),
                self.gain * (1.0 + self.pan).min(1.0),
            ),
        }
    }
}

fn finite(value: f32, low: f32, high: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(low, high)
    } else {
        fallback
    }
}

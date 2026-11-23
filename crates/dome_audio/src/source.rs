use crate::sound::{Channels, Sound};
use crate::stream::{Stream, Track};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

const LOOKAHEAD: usize = 4;

#[derive(Clone)]
pub enum Source {
    Decoded(Arc<Sound>),
    Stream(Arc<Stream>),
}

pub(crate) enum Input {
    Decoded(Arc<Sound>),
    Stream(Streamed),
}

impl std::fmt::Debug for Source {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decoded(sound) => formatter.debug_tuple("Decoded").field(sound).finish(),
            Self::Stream(stream) => formatter.debug_tuple("Stream").field(stream).finish(),
        }
    }
}

impl std::fmt::Debug for Input {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decoded(sound) => formatter.debug_tuple("Decoded").field(sound).finish(),
            Self::Stream(stream) => formatter.debug_tuple("Stream").field(stream).finish(),
        }
    }
}

impl std::fmt::Debug for Streamed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Streamed")
            .field("track", &self.track)
            .field("base", &self.base)
            .field("stalled", &self.stalled)
            .finish()
    }
}

pub(crate) struct Streamed {
    track: Track,
    window: VecDeque<[f32; 2]>,
    base: u64,
    held: [f32; 2],
    stalled: u64,
}

impl Source {
    pub fn decoded(sound: Arc<Sound>) -> Self {
        Self::Decoded(sound)
    }

    pub fn stream(path: impl Into<PathBuf>) -> Result<Self, String> {
        Ok(Self::Stream(Arc::new(Stream::open(path)?)))
    }

    pub fn identity(&self) -> usize {
        match self {
            Self::Decoded(sound) => Arc::as_ptr(sound) as usize,
            Self::Stream(stream) => Arc::as_ptr(stream) as usize,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        match self {
            Self::Decoded(sound) => sound.sample_rate(),
            Self::Stream(stream) => stream.sample_rate(),
        }
    }

    pub fn channels(&self) -> Channels {
        match self {
            Self::Decoded(sound) => sound.channels(),
            Self::Stream(stream) => stream.channels(),
        }
    }

    pub fn frame_count(&self) -> Option<u64> {
        match self {
            Self::Decoded(sound) => Some(sound.frame_count() as u64),
            Self::Stream(stream) => stream.frame_count(),
        }
    }

    pub(crate) fn open(&self, looping: bool, start: u64) -> Result<Input, String> {
        match self {
            Self::Decoded(sound) => Ok(Input::Decoded(Arc::clone(sound))),
            Self::Stream(stream) => Ok(Input::Stream(Streamed::new(
                stream.track(looping, start)?,
                start,
            ))),
        }
    }
}

impl Input {
    pub(crate) fn sample_rate(&self) -> u32 {
        match self {
            Self::Decoded(sound) => sound.sample_rate(),
            Self::Stream(stream) => stream.track.sample_rate(),
        }
    }

    pub(crate) fn channels(&self) -> Channels {
        match self {
            Self::Decoded(sound) => sound.channels(),
            Self::Stream(stream) => stream.track.channels(),
        }
    }

    pub(crate) fn sample(&mut self, position: f64, looping: bool) -> [f32; 2] {
        match self {
            Self::Decoded(sound) => decoded(sound, position, looping),
            Self::Stream(stream) => stream.sample(position),
        }
    }

    pub(crate) fn finished(&self, position: f64, looping: bool) -> bool {
        match self {
            Self::Decoded(sound) => {
                let frames = sound.frame_count() as f64;
                frames == 0.0 || (!looping && position >= frames)
            }
            Self::Stream(stream) => stream
                .track
                .total()
                .is_some_and(|total| position >= total as f64),
        }
    }

    pub(crate) fn playhead(&self, position: f64) -> u64 {
        let at = position.max(0.0) as u64;
        match self {
            Self::Decoded(_) => at,
            Self::Stream(stream) => stream.track.period().map_or(at, |period| at % period),
        }
    }

    pub(crate) fn decoded_frames(&self) -> Option<u64> {
        match self {
            Self::Decoded(sound) => Some(sound.frame_count() as u64),
            Self::Stream(_) => None,
        }
    }
}

impl Streamed {
    pub(crate) fn new(track: Track, start: u64) -> Self {
        Self {
            track,
            window: VecDeque::with_capacity(LOOKAHEAD),
            base: start,
            held: [0.0; 2],
            stalled: 0,
        }
    }

    fn sample(&mut self, position: f64) -> [f32; 2] {
        let floor = position.floor();
        let at = floor.max(0.0) as u64;
        while at + 2 >= self.base + self.window.len() as u64 {
            let frame = self.pull();
            self.window.push_back(frame);
        }
        let read = |index: u64| -> [f32; 2] {
            if index < self.base {
                return self.window.front().copied().unwrap_or(self.held);
            }
            self.window
                .get((index - self.base) as usize)
                .copied()
                .unwrap_or(self.held)
        };
        let previous = read(at.saturating_sub(1));
        let held = read(at);
        let next = read(at + 1);
        let after = read(at + 2);
        let fraction = (position - floor) as f32;
        let frame = [
            interpolate(previous[0], held[0], next[0], after[0], fraction),
            interpolate(previous[1], held[1], next[1], after[1], fraction),
        ];
        while self.window.len() > LOOKAHEAD {
            self.window.pop_front();
            self.base += 1;
        }
        frame
    }

    fn pull(&mut self) -> [f32; 2] {
        loop {
            if self.stalled > 0 {
                match self.track.take() {
                    Some(_) => {
                        self.stalled -= 1;
                        continue;
                    }
                    None => {
                        self.stalled += 1;
                        return [0.0; 2];
                    }
                }
            }
            match self.track.take() {
                Some(frame) => {
                    self.held = frame;
                    return frame;
                }
                None => {
                    self.stalled += 1;
                    return [0.0; 2];
                }
            }
        }
    }
}

fn decoded(sound: &Sound, position: f64, looping: bool) -> [f32; 2] {
    let base = position.floor();
    let fraction = (position - base) as f32;
    let at = base as isize;
    let end = sound.frame_count() as isize;
    let sample = |channel: usize| {
        interpolate(
            held(sound, at - 1, channel, end, looping),
            held(sound, at, channel, end, looping),
            held(sound, at + 1, channel, end, looping),
            held(sound, at + 2, channel, end, looping),
            fraction,
        )
    };
    match sound.channels() {
        Channels::Mono => {
            let left = sample(0);
            [left, left]
        }
        Channels::Stereo => [sample(0), sample(1)],
    }
}

fn held(sound: &Sound, index: isize, channel: usize, end: isize, looping: bool) -> f32 {
    let index = if looping {
        index.rem_euclid(end)
    } else {
        index.clamp(0, end - 1)
    } as usize;
    sound.samples()[index * sound.channels().count() + channel]
}

fn interpolate(previous: f32, held: f32, next: f32, after: f32, fraction: f32) -> f32 {
    let a = -previous + 3.0 * held - 3.0 * next + after;
    let b = 2.0 * previous - 5.0 * held + 4.0 * next - after;
    let c = next - previous;
    0.5 * (a * fraction * fraction * fraction + b * fraction * fraction + c * fraction + 2.0 * held)
}

use crate::wav;
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channels {
    Mono,
    Stereo,
}

impl Channels {
    pub fn count(self) -> usize {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }
}

#[derive(Clone)]
pub struct Sound {
    sample_rate: u32,
    channels: Channels,
    frames: Arc<[f32]>,
}

impl Sound {
    pub fn new(sample_rate: u32, channels: Channels, frames: Vec<f32>) -> Self {
        assert!(
            sample_rate > 0,
            "a sound of {sample_rate} hertz holds no frame",
        );
        assert!(!frames.is_empty(), "a sound of no sample plays no frame",);
        assert_eq!(
            frames.len() % channels.count(),
            0,
            "a sound of {} samples holds no whole {} frame",
            frames.len(),
            channels.name(),
        );
        Self {
            sample_rate,
            channels,
            frames: Arc::from(frames),
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let bytes = fs::read(path)
            .unwrap_or_else(|error| panic!("the sound {} reads no bytes: {error}", path.display()));
        Self::decode(&bytes, &format!("the sound {}", path.display()))
    }

    pub fn decode(bytes: &[u8], what: &str) -> Self {
        if bytes.len() >= 4 && &bytes[..4] == b"OggS" {
            crate::vorbis::decode(bytes, what)
        } else {
            wav::decode(bytes, what)
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> Channels {
        self.channels
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len() / self.channels.count()
    }

    pub(crate) fn samples(&self) -> &[f32] {
        &self.frames
    }
}

impl fmt::Debug for Sound {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Sound")
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .field("frames", &self.frame_count())
            .finish()
    }
}

impl Channels {
    fn name(self) -> &'static str {
        match self {
            Self::Mono => "mono",
            Self::Stereo => "stereo",
        }
    }
}

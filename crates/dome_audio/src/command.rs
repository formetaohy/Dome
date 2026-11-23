use crate::graph::{BusParams, Layout};
use crate::source::Input;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct VoiceId(pub(crate) u32);

#[derive(Debug)]
pub(crate) enum Command {
    Play {
        voice: VoiceId,
        input: Input,
        bus: Option<String>,
        gain: f32,
        pan: f32,
        cutoff: f32,
        looping: bool,
        pitch: f32,
        priority: i32,
        start: u64,
    },
    Update {
        voice: VoiceId,
        gain: f32,
        pan: f32,
        cutoff: f32,
    },
    Replace {
        voice: VoiceId,
        source: crate::source::Source,
    },
    Stop {
        voice: VoiceId,
    },
    Buses(Arc<Layout>),
    Params(Arc<Vec<BusParams>>),
    Master {
        gain: f32,
    },
    Paused {
        paused: bool,
    },
}

#[derive(Debug)]
pub(crate) enum Event {
    Finished(VoiceId),
    Stolen { voice: VoiceId, position: u64 },
    Dropped(VoiceId),
}

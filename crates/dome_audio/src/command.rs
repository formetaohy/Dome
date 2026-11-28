use crate::graph::{BusParams, Layout};
use crate::source::Input;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Voice {
    pub(crate) slot: u32,
    pub(crate) token: u32,
}

#[derive(Debug)]
pub(crate) enum Command {
    Play {
        voice: Voice,
        input: Input,
        bus: Option<String>,
        gain: f32,
        pan: f32,
        cutoff: f32,
        looping: bool,
        pitch: f32,
        start: u64,
    },
    Update {
        voice: Voice,
        gain: f32,
        pan: f32,
        cutoff: f32,
    },
    Replace {
        voice: Voice,
        input: Input,
        seek: u64,
    },
    Stop {
        voice: Voice,
    },
    Layout {
        layout: Arc<Layout>,
        params: Arc<Vec<BusParams>>,
    },
    Params {
        params: Arc<Vec<BusParams>>,
    },
    Master {
        gain: f32,
    },
    Paused {
        paused: bool,
    },
}

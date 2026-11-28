use std::fmt::{self, Display, Formatter};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuEvent {
    DeviceLost {
        reason: wgpu::DeviceLostReason,
        message: String,
    },
    SurfaceOutdated,
    SurfaceLost,
    Fault(String),
}

impl GpuEvent {
    pub fn device_lost(&self) -> bool {
        matches!(self, Self::DeviceLost { .. })
    }

    pub fn fault(&self) -> Option<&str> {
        match self {
            Self::Fault(message) => Some(message),
            _ => None,
        }
    }
}

impl Display for GpuEvent {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceLost { reason, message } => {
                write!(out, "the graphics device is lost: {reason:?}: {message}")
            }
            Self::SurfaceOutdated => write!(out, "the present surface is outdated"),
            Self::SurfaceLost => write!(out, "the present surface is lost"),
            Self::Fault(message) => write!(out, "the graphics device fails: {message}"),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct EventLog {
    events: Arc<Mutex<Vec<GpuEvent>>>,
}

impl EventLog {
    pub(crate) fn emit(&self, event: GpuEvent) {
        let mut events = self
            .events
            .lock()
            .expect("a graphics event log is never poisoned");
        if event.device_lost() && events.iter().any(GpuEvent::device_lost) {
            return;
        }
        events.push(event);
    }

    pub(crate) fn cursor(&self) -> GpuEventCursor {
        GpuEventCursor {
            log: self.clone(),
            next: 0,
        }
    }

    pub(crate) fn device_lost(&self) -> bool {
        self.events
            .lock()
            .expect("a graphics event log is never poisoned")
            .iter()
            .any(|event| matches!(event, GpuEvent::DeviceLost { .. }))
    }
}

pub struct GpuEventCursor {
    log: EventLog,
    next: usize,
}

impl GpuEventCursor {
    pub fn unhealthy(&self) -> bool {
        self.log
            .events
            .lock()
            .expect("a graphics event log is never poisoned")
            .iter()
            .any(|event| event.device_lost() || event.fault().is_some())
    }

    pub fn poll(&mut self) -> Vec<GpuEvent> {
        let events = self
            .log
            .events
            .lock()
            .expect("a graphics event log is never poisoned");
        let unread = events.get(self.next..).unwrap_or(&[]).to_vec();
        self.next = events.len();
        unread
    }
}

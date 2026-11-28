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

const LOG_CAPACITY: usize = 64;

#[derive(Default)]
struct Ledger {
    base: u64,
    events: Vec<GpuEvent>,
}

#[derive(Clone, Default)]
pub(crate) struct EventLog {
    log: Arc<Mutex<Ledger>>,
}

impl EventLog {
    pub(crate) fn emit(&self, event: GpuEvent) {
        let mut log = self
            .log
            .lock()
            .expect("a graphics event log is never poisoned");
        if event.device_lost() && log.events.iter().any(GpuEvent::device_lost) {
            return;
        }
        log.events.push(event);
        if log.events.len() > LOG_CAPACITY {
            let dropped = log.events.len() - LOG_CAPACITY;
            log.events.drain(..dropped);
            log.base += dropped as u64;
        }
    }

    pub(crate) fn cursor(&self) -> GpuEventCursor {
        let next = {
            let log = self
                .log
                .lock()
                .expect("a graphics event log is never poisoned");
            log.base + log.events.len() as u64
        };
        GpuEventCursor {
            log: self.clone(),
            next,
            lost: false,
        }
    }

    pub(crate) fn device_lost(&self) -> bool {
        self.log
            .lock()
            .expect("a graphics event log is never poisoned")
            .events
            .iter()
            .any(|event| matches!(event, GpuEvent::DeviceLost { .. }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuCondition {
    Healthy,
    Lost,
    Fault(String),
}

pub struct GpuEventCursor {
    log: EventLog,
    next: u64,
    lost: bool,
}

impl GpuEventCursor {
    pub fn unhealthy(&self) -> bool {
        let log = self
            .log
            .log
            .lock()
            .expect("a graphics event log is never poisoned");
        (!self.lost && log.events.iter().any(GpuEvent::device_lost))
            || log
                .events
                .iter()
                .skip(self.next.saturating_sub(log.base) as usize)
                .any(|event| event.fault().is_some())
    }

    pub fn condition(&mut self) -> GpuCondition {
        let mut fault = None;
        for event in self.poll() {
            if event.device_lost() {
                return GpuCondition::Lost;
            }
            if let Some(message) = event.fault() {
                fault = Some(String::from(message));
            }
        }
        match fault {
            Some(message) => GpuCondition::Fault(message),
            None => GpuCondition::Healthy,
        }
    }

    pub fn poll(&mut self) -> Vec<GpuEvent> {
        let log = self
            .log
            .log
            .lock()
            .expect("a graphics event log is never poisoned");
        let start = self
            .next
            .saturating_sub(log.base)
            .min(log.events.len() as u64);
        let mut unread = log.events[start as usize..].to_vec();
        self.next = log.base + log.events.len() as u64;
        if !self.lost
            && let Some(loss) = log.events.iter().find(|event| event.device_lost())
        {
            self.lost = true;
            unread.push(loss.clone());
        }
        unread
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_loss_outranks_the_errors_it_raises() {
        let log = EventLog::default();
        log.emit(GpuEvent::Fault(String::from(
            "wgpu: the device is lost before it submits",
        )));
        log.emit(GpuEvent::DeviceLost {
            reason: wgpu::DeviceLostReason::Destroyed,
            message: String::new(),
        });
        assert_eq!(log.cursor().condition(), GpuCondition::Lost);
    }

    #[test]
    fn a_fault_reaches_only_the_cursor_that_was_open_when_it_rose() {
        let log = EventLog::default();
        let mut watching = log.cursor();
        log.emit(GpuEvent::Fault(String::from("a lost binding")));
        assert_eq!(
            watching.condition(),
            GpuCondition::Fault(String::from("a lost binding")),
        );
        assert_eq!(
            log.cursor().condition(),
            GpuCondition::Healthy,
            "a cursor opened after a recovered fault starts at the head of its log",
        );
    }
}

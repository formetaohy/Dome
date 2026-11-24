use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ResourceId(u64);

impl ResourceId {
    pub(crate) fn next() -> Self {
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Retired {
    Resource(ResourceId),
    Generation(ResourceId, u64),
}

#[derive(Default)]
pub(crate) struct Retirements {
    queue: Mutex<Vec<Retired>>,
    pending: AtomicBool,
}

impl Retirements {
    fn retire(&self, retired: Retired) {
        self.queue
            .lock()
            .expect("a retirement queue is never poisoned")
            .push(retired);
        self.pending.store(true, Ordering::Release);
    }

    pub(crate) fn drain(&self) -> Vec<Retired> {
        if !self.pending.swap(false, Ordering::AcqRel) {
            return Vec::new();
        }
        let mut queue = self
            .queue
            .lock()
            .expect("a retirement queue is never poisoned");
        std::mem::take(&mut *queue)
    }
}

pub(crate) struct Liveness {
    id: ResourceId,
    retirements: Weak<Retirements>,
}

impl Liveness {
    pub(crate) fn new(retirements: &Arc<Retirements>) -> Arc<Self> {
        Arc::new(Self {
            id: ResourceId::next(),
            retirements: Arc::downgrade(retirements),
        })
    }

    pub(crate) fn id(&self) -> ResourceId {
        self.id
    }

    pub(crate) fn supersede(&self, generation: u64) {
        if let Some(retirements) = self.retirements.upgrade() {
            retirements.retire(Retired::Generation(self.id, generation));
        }
    }
}

impl Drop for Liveness {
    fn drop(&mut self) {
        if let Some(retirements) = self.retirements.upgrade() {
            retirements.retire(Retired::Resource(self.id));
        }
    }
}

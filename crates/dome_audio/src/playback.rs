use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const FREE: u32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Pending = 1,
    Playing = 2,
    Fading = 3,
    Finished = 4,
}

impl State {
    fn of(code: u32) -> Option<Self> {
        match code {
            1 => Some(Self::Pending),
            2 => Some(Self::Playing),
            3 => Some(Self::Fading),
            4 => Some(Self::Finished),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    pub(crate) token: u32,
    pub(crate) state: State,
    pub(crate) position: u64,
}

struct Slot {
    packed: AtomicU64,
    position: AtomicU64,
}

fn pack(token: u32, state: State) -> u64 {
    u64::from(token) << 32 | state as u64
}

fn unpack(packed: u64) -> (u32, Option<State>) {
    ((packed >> 32) as u32, State::of(packed as u32))
}

impl Slot {
    fn report(&self) -> Option<Report> {
        let (token, state) = unpack(self.packed.load(Ordering::Acquire));
        let state = state?;
        (token != FREE).then(|| Report {
            token,
            state,
            position: self.position.load(Ordering::Relaxed),
        })
    }
}

pub(crate) struct Playback {
    slots: Box<[Slot]>,
    beat: AtomicU64,
}

impl Playback {
    pub(crate) fn new(capacity: usize) -> Arc<Self> {
        assert!(
            capacity > 0 && capacity <= u32::MAX as usize / 2,
            "a playback table of {capacity} voices spans no slot",
        );
        Arc::new(Self {
            slots: (0..capacity)
                .map(|_| Slot {
                    packed: AtomicU64::new(0),
                    position: AtomicU64::new(0),
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            beat: AtomicU64::new(0),
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    pub(crate) fn tick(&self) {
        self.beat.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn beat(&self) -> u64 {
        self.beat.load(Ordering::Relaxed)
    }

    pub(crate) fn report(&self, slot: usize) -> Option<Report> {
        self.slots.get(slot)?.report()
    }

    pub(crate) fn begin(&self, slot: usize, token: u32) {
        assert!(
            token != FREE,
            "a voice token of {token} names the free slot",
        );
        self.slots[slot]
            .packed
            .store(pack(token, State::Pending), Ordering::Release);
    }

    pub(crate) fn release(&self, slot: usize, token: u32) {
        let slot = &self.slots[slot];
        if slot.packed.load(Ordering::Acquire) >> 32 == u64::from(token) {
            slot.packed.store(0, Ordering::Release);
        }
    }

    pub(crate) fn release_all(&self) {
        for slot in &self.slots {
            slot.packed.store(0, Ordering::Release);
            slot.position.store(0, Ordering::Release);
        }
    }

    pub(crate) fn publish(
        &self,
        slot: usize,
        token: u32,
        state: &mut State,
        next: State,
        position: u64,
    ) {
        let Some(slot) = self.slots.get(slot) else {
            return;
        };
        if slot.packed.load(Ordering::Acquire) >> 32 == u64::from(token) {
            slot.position.store(position, Ordering::Relaxed);
        }
        if slot
            .packed
            .compare_exchange(
                pack(token, *state),
                pack(token, next),
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            *state = next;
        }
    }
}

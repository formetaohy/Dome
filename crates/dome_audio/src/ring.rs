use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Slot<T> {
    sequence: AtomicUsize,
    value: UnsafeCell<MaybeUninit<T>>,
}

struct Shared<T> {
    slots: Box<[Slot<T>]>,
    mask: usize,
    capacity: usize,
}

unsafe impl<T: Send> Send for Shared<T> {}
unsafe impl<T: Send> Sync for Shared<T> {}

pub(crate) struct Producer<T: Send> {
    shared: Arc<Shared<T>>,
    tail: usize,
}

pub(crate) struct Consumer<T: Send> {
    shared: Arc<Shared<T>>,
    head: usize,
}

pub(crate) fn channel<T: Send>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    let capacity = capacity.next_power_of_two().max(2);
    assert!(
        capacity <= isize::MAX as usize / 2,
        "a queue of {capacity} items outruns its indices",
    );
    let slots = (0..capacity)
        .map(|index| Slot {
            sequence: AtomicUsize::new(index),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let shared = Arc::new(Shared {
        slots,
        mask: capacity - 1,
        capacity,
    });
    (
        Producer {
            shared: Arc::clone(&shared),
            tail: 0,
        },
        Consumer { shared, head: 0 },
    )
}

impl<T: Send> Producer<T> {
    pub(crate) fn send(&mut self, value: T) -> Result<(), T> {
        let slot = &self.shared.slots[self.tail & self.shared.mask];
        let sequence = slot.sequence.load(Ordering::Acquire);
        if sequence.wrapping_sub(self.tail) as isize != 0 {
            return Err(value);
        }
        unsafe {
            (*slot.value.get()).write(value);
        }
        slot.sequence
            .store(self.tail.wrapping_add(1), Ordering::Release);
        self.tail = self.tail.wrapping_add(1);
        Ok(())
    }
}

impl<T: Send> Consumer<T> {
    pub(crate) fn take(&mut self) -> Option<T> {
        let slot = &self.shared.slots[self.head & self.shared.mask];
        let sequence = slot.sequence.load(Ordering::Acquire);
        if sequence.wrapping_sub(self.head.wrapping_add(1)) as isize != 0 {
            return None;
        }
        let value = unsafe { (*slot.value.get()).assume_init_read() };
        slot.sequence.store(
            self.head.wrapping_add(self.shared.capacity),
            Ordering::Release,
        );
        self.head = self.head.wrapping_add(1);
        Some(value)
    }
}

impl<T: Send> Drop for Consumer<T> {
    fn drop(&mut self) {
        while self.take().is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_queue_orders_items_and_reports_its_full_state() {
        let (mut producer, mut consumer) = channel(2);
        assert!(producer.send(1).is_ok());
        assert!(producer.send(2).is_ok());
        assert_eq!(producer.send(3), Err(3));
        assert_eq!(consumer.take(), Some(1));
        assert!(producer.send(3).is_ok());
        assert_eq!(consumer.take(), Some(2));
        assert_eq!(consumer.take(), Some(3));
        assert_eq!(consumer.take(), None);
    }
}

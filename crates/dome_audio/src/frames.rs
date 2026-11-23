use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const CHANNELS: usize = 2;

struct Shared {
    samples: Box<[UnsafeCell<f32>]>,
    capacity: usize,
    read: AtomicUsize,
    write: AtomicUsize,
    closed: AtomicBool,
}

unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

pub(crate) struct Producer {
    shared: Arc<Shared>,
}

pub(crate) struct Consumer {
    shared: Arc<Shared>,
}

pub(crate) fn channel(capacity: usize) -> (Producer, Consumer) {
    assert!(
        capacity > 0 && capacity.is_power_of_two(),
        "a frame queue of {capacity} frames holds no power of two",
    );
    let samples = (0..capacity * CHANNELS)
        .map(|_| UnsafeCell::new(0.0f32))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let shared = Arc::new(Shared {
        samples,
        capacity,
        read: AtomicUsize::new(0),
        write: AtomicUsize::new(0),
        closed: AtomicBool::new(false),
    });
    (
        Producer {
            shared: Arc::clone(&shared),
        },
        Consumer { shared },
    )
}

impl Producer {
    pub(crate) fn write(&mut self, frames: &[[f32; 2]]) -> usize {
        let read = self.shared.read.load(Ordering::Acquire);
        let write = self.shared.write.load(Ordering::Relaxed);
        let free = self.shared.capacity - write.wrapping_sub(read);
        let count = free.min(frames.len());
        let mask = self.shared.capacity - 1;
        for (offset, frame) in frames[..count].iter().enumerate() {
            let at = (write.wrapping_add(offset) & mask) * CHANNELS;
            unsafe {
                *self.shared.samples[at].get() = frame[0];
                *self.shared.samples[at + 1].get() = frame[1];
            }
        }
        self.shared
            .write
            .store(write.wrapping_add(count), Ordering::Release);
        count
    }

    pub(crate) fn closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }
}

impl Consumer {
    pub(crate) fn take(&mut self) -> Option<[f32; 2]> {
        let write = self.shared.write.load(Ordering::Acquire);
        let read = self.shared.read.load(Ordering::Relaxed);
        if read == write {
            return None;
        }
        let mask = self.shared.capacity - 1;
        let at = (read & mask) * CHANNELS;
        let frame = unsafe {
            [
                *self.shared.samples[at].get(),
                *self.shared.samples[at + 1].get(),
            ]
        };
        self.shared
            .read
            .store(read.wrapping_add(1), Ordering::Release);
        Some(frame)
    }
}

impl Drop for Consumer {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stereo(value: f32) -> [f32; 2] {
        [value, -value]
    }

    #[test]
    fn a_frame_queue_wraps_without_losing_samples() {
        let (mut producer, mut consumer) = channel(2);
        assert_eq!(producer.write(&[stereo(0.0), stereo(1.0), stereo(2.0)]), 2);
        assert_eq!(consumer.take(), Some(stereo(0.0)));
        assert_eq!(producer.write(&[stereo(2.0), stereo(3.0)]), 1);
        assert_eq!(consumer.take(), Some(stereo(1.0)));
        assert_eq!(consumer.take(), Some(stereo(2.0)));
        assert_eq!(producer.write(&[stereo(3.0)]), 1);
        assert_eq!(consumer.take(), Some(stereo(3.0)));
        assert_eq!(consumer.take(), None);
    }
}

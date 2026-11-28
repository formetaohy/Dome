use crate::graph::{BusParams, Layout};
use crate::ring;
use crate::source::Input;
use std::sync::Arc;

pub(crate) enum Garbage {
    Layout(Arc<Layout>),
    Params(Arc<Vec<BusParams>>),
    Voice { input: Input, bus: Option<String> },
}

pub(crate) fn channel(capacity: usize) -> (Queue, Bin) {
    let (producer, consumer) = ring::channel(capacity);
    (
        Queue {
            producer,
            pending: Vec::with_capacity(capacity),
        },
        Bin { consumer },
    )
}

pub(crate) struct Queue {
    producer: ring::Producer<Garbage>,
    pending: Vec<Garbage>,
}

impl Queue {
    pub(crate) fn discard(&mut self, garbage: Garbage) {
        if let Err(held) = self.producer.send(garbage) {
            self.pending.push(held);
        }
    }

    pub(crate) fn flush(&mut self) {
        while let Some(held) = self.pending.pop() {
            if let Err(held) = self.producer.send(held) {
                self.pending.push(held);
                return;
            }
        }
    }
}

pub(crate) struct Bin {
    consumer: ring::Consumer<Garbage>,
}

impl Bin {
    pub(crate) fn empty(&mut self) -> usize {
        let mut emptied = 0;
        while let Some(garbage) = self.consumer.take() {
            match garbage {
                Garbage::Layout(layout) => drop(layout),
                Garbage::Params(params) => drop(params),
                Garbage::Voice { input, bus } => {
                    drop(input);
                    drop(bus);
                }
            }
            emptied += 1;
        }
        emptied
    }
}

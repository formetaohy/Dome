use crate::body::{Body, BodyKind};

pub(crate) const SLEEP_LINEAR: f32 = 0.03;
pub(crate) const SLEEP_ANGULAR: f32 = 0.03;
pub(crate) const SLEEP_TIME: f32 = 0.5;

#[derive(Default)]
pub(crate) struct Islands {
    parents: Vec<usize>,
    slow: Vec<bool>,
    moving: Vec<bool>,
    clock: Vec<f32>,
    next: Vec<f32>,
}

impl Islands {
    pub(crate) fn settle(
        &mut self,
        bodies: &mut [Body],
        links: &[(usize, usize)],
        driven: &[bool],
        delta: f32,
    ) {
        let count = bodies.len();
        self.parents.clear();
        self.parents.extend(0..count);
        for (first, second) in links {
            if bodies[*first].rigid.kind != BodyKind::Dynamic
                || bodies[*second].rigid.kind != BodyKind::Dynamic
            {
                continue;
            }
            self.join(*first, *second);
        }
        self.slow.clear();
        self.slow.resize(count, true);
        self.moving.clear();
        self.moving.resize(count, false);
        self.clock.clear();
        self.clock.resize(count, f32::INFINITY);
        for (at, body) in bodies.iter().enumerate() {
            if body.rigid.kind != BodyKind::Dynamic {
                continue;
            }
            let root = self.root(at);
            let held = body.slow();
            if !held || driven[at] || !body.rigid.can_sleep() {
                self.slow[root] = false;
            }
            if !held {
                self.moving[root] = true;
            }
            let time = if body.rigid.is_sleeping() {
                SLEEP_TIME
            } else {
                body.rigid.sleep_time()
            };
            self.clock[root] = self.clock[root].min(time);
        }
        self.next.clear();
        self.next.resize(count, 0.0);
        for at in 0..count {
            if !self.owns(at) {
                continue;
            }
            self.next[at] = if self.slow[at] {
                self.clock[at].min(SLEEP_TIME) + delta
            } else {
                0.0
            };
        }
        for (at, body) in bodies.iter_mut().enumerate() {
            if body.rigid.kind != BodyKind::Dynamic {
                continue;
            }
            let root = self.root(at);
            let moving = self.moving[root];
            let next = self.next[root];
            if moving {
                body.wake();
            } else {
                body.rigid.set_sleep_time(next);
                if next >= SLEEP_TIME {
                    body.sleep();
                }
            }
        }
    }

    fn root(&mut self, at: usize) -> usize {
        let mut held = at;
        while self.parents[held] != held {
            self.parents[held] = self.parents[self.parents[held]];
            held = self.parents[held];
        }
        held
    }

    fn join(&mut self, first: usize, second: usize) {
        let left = self.root(first);
        let right = self.root(second);
        if left != right {
            self.parents[right] = left;
        }
    }

    fn owns(&mut self, at: usize) -> bool {
        self.root(at) == at
    }
}

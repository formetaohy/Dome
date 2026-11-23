use crate::body::{Body, BodyKind};

pub(crate) const SLEEP_LINEAR: f32 = 0.03;
pub(crate) const SLEEP_ANGULAR: f32 = 0.03;
pub(crate) const SLEEP_TIME: f32 = 0.5;

pub(crate) fn settle(bodies: &mut [Body], links: &[(usize, usize)], driven: &[bool], delta: f32) {
    let count = bodies.len();
    let mut sets = Sets::new(count);
    for (first, second) in links {
        sets.join(*first, *second);
    }
    let mut slow = vec![true; count];
    let mut moving = vec![false; count];
    let mut clock = vec![f32::INFINITY; count];
    for (at, body) in bodies.iter().enumerate() {
        if body.rigid.kind != BodyKind::Dynamic {
            continue;
        }
        let root = sets.root(at);
        let held = body.slow();
        if !held || driven[at] || !body.rigid.can_sleep() {
            slow[root] = false;
        }
        if !held {
            moving[root] = true;
        }
        let time = if body.rigid.is_sleeping() {
            SLEEP_TIME
        } else {
            body.rigid.sleep_time()
        };
        clock[root] = clock[root].min(time);
    }
    let mut next = vec![0.0f32; count];
    for at in 0..count {
        if !sets.owns(at) {
            continue;
        }
        next[at] = if slow[at] {
            clock[at].min(SLEEP_TIME) + delta
        } else {
            0.0
        };
    }
    for (at, body) in bodies.iter_mut().enumerate() {
        if body.rigid.kind != BodyKind::Dynamic {
            continue;
        }
        let root = sets.root(at);
        if moving[root] {
            body.wake();
        } else {
            body.rigid.set_sleep_time(next[root]);
            if next[root] >= SLEEP_TIME {
                body.sleep();
            }
        }
    }
}

struct Sets {
    parents: Vec<usize>,
}

impl Sets {
    fn new(count: usize) -> Self {
        Self {
            parents: (0..count).collect(),
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

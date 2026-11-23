use dome_ecs::Entity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionPhase {
    Begin,
    End,
}

impl CollisionPhase {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Begin => "begin",
            Self::End => "end",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionEvent {
    pub first: Entity,
    pub second: Entity,
    pub phase: CollisionPhase,
    pub sensor: bool,
}

#[derive(Default)]
pub struct Collisions {
    active: Vec<(Entity, Entity, bool)>,
    events: Vec<CollisionEvent>,
}

impl Collisions {
    pub fn events(&self) -> &[CollisionEvent] {
        &self.events
    }

    pub(crate) fn touching(&self) -> &[(Entity, Entity, bool)] {
        &self.active
    }

    pub(crate) fn settle(&mut self, touched: &[(Entity, Entity, bool)]) {
        self.events.clear();
        let previous = std::mem::take(&mut self.active);
        let (mut at, mut was) = (0, 0);
        while at < touched.len() || was < previous.len() {
            let held = touched.get(at);
            let before = previous.get(was);
            let (pair, phase, sensor) = match (held, before) {
                (Some(held), Some(before)) => {
                    let left = (held.0, held.1);
                    let right = (before.0, before.1);
                    match left.cmp(&right) {
                        std::cmp::Ordering::Less => (left, Some(CollisionPhase::Begin), held.2),
                        std::cmp::Ordering::Greater => (right, Some(CollisionPhase::End), before.2),
                        std::cmp::Ordering::Equal => {
                            at += 1;
                            was += 1;
                            continue;
                        }
                    }
                }
                (Some(held), None) => ((held.0, held.1), Some(CollisionPhase::Begin), held.2),
                (None, Some(before)) => ((before.0, before.1), Some(CollisionPhase::End), before.2),
                (None, None) => break,
            };
            if held.is_some_and(|held| (held.0, held.1) == pair) {
                at += 1;
            } else {
                was += 1;
            }
            if let Some(phase) = phase {
                self.events.push(CollisionEvent {
                    first: pair.0,
                    second: pair.1,
                    phase,
                    sensor,
                });
            }
        }
        self.active = previous;
        self.active.clear();
        self.active.extend_from_slice(touched);
    }
}

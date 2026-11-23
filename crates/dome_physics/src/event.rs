use dome_ecs::Entity;
use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionEvent {
    pub first: Entity,
    pub second: Entity,
    pub phase: CollisionPhase,
    pub sensor: bool,
}

#[derive(Default)]
pub struct Collisions {
    active: BTreeMap<(Entity, Entity), bool>,
    events: Vec<CollisionEvent>,
}

impl Collisions {
    pub fn events(&self) -> &[CollisionEvent] {
        &self.events
    }

    pub(crate) fn settle(&mut self, active: BTreeMap<(Entity, Entity), bool>) {
        self.events.clear();
        let pairs = active
            .keys()
            .chain(self.active.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        for (first, second) in pairs {
            let pair = (first, second);
            let phase = match (active.get(&pair), self.active.get(&pair)) {
                (Some(_), None) => CollisionPhase::Begin,
                (None, Some(_)) => CollisionPhase::End,
                _ => continue,
            };
            let sensor = *active
                .get(&pair)
                .or_else(|| self.active.get(&pair))
                .expect("a settled pair touches a side of the step");
            self.events.push(CollisionEvent {
                first,
                second,
                phase,
                sensor,
            });
        }
        self.active = active;
    }
}

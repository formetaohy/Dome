use dome_ecs::Entity;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct ContactCache {
    points: HashMap<(Entity, Entity, usize, usize, u64), CachedPoint>,
}

#[derive(Clone, Copy)]
pub(crate) struct CachedPoint {
    pub normal: f32,
    pub tangent: [f32; 2],
    pub rebound: f32,
}

impl ContactCache {
    pub(crate) fn pairs(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        self.points.keys().map(|held| (held.0, held.1))
    }

    pub(crate) fn take(
        &mut self,
        first: Entity,
        second: Entity,
        first_part: usize,
        second_part: usize,
        feature: u64,
    ) -> Option<CachedPoint> {
        self.points.remove(&contact_key(
            first,
            second,
            first_part,
            second_part,
            feature,
        ))
    }

    pub(crate) fn insert(
        &mut self,
        first: Entity,
        second: Entity,
        first_part: usize,
        second_part: usize,
        points: impl IntoIterator<Item = (u64, CachedPoint)>,
    ) {
        for (feature, point) in points {
            self.points.insert(
                contact_key(first, second, first_part, second_part, feature),
                point,
            );
        }
    }
}

pub(crate) fn contact_key(
    first: Entity,
    second: Entity,
    first_part: usize,
    second_part: usize,
    feature: u64,
) -> (Entity, Entity, usize, usize, u64) {
    if first <= second {
        (first, second, first_part, second_part, feature)
    } else {
        (second, first, second_part, first_part, feature)
    }
}

pub(crate) fn key(first: Entity, second: Entity) -> (Entity, Entity) {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

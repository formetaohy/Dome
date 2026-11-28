use dome_ecs::Entity;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct ContactCache {
    points: HashMap<(Entity, Entity, Entity, Entity, u64), CachedPoint>,
}

#[derive(Clone, Copy)]
pub(crate) struct CachedPoint {
    pub normal: f32,
    pub tangent: [f32; 2],
    pub rebound: f32,
}

impl ContactCache {
    pub(crate) fn clear(&mut self) {
        self.points.clear();
    }

    pub(crate) fn forget(&mut self, entity: Entity) {
        self.points
            .retain(|key, _| key.0 != entity && key.1 != entity);
    }

    pub(crate) fn take(
        &mut self,
        first: Entity,
        second: Entity,
        first_part: Entity,
        second_part: Entity,
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
        first_part: Entity,
        second_part: Entity,
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
    first_part: Entity,
    second_part: Entity,
    feature: u64,
) -> (Entity, Entity, Entity, Entity, u64) {
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

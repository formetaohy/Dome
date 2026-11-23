use dome_ecs::Entity;
use dome_math::Transform;

pub(crate) const NONE: u32 = u32::MAX;

pub(crate) struct Node {
    pub entity: Entity,
    pub local: Transform,
    pub relative: Transform,
    pub parent: u32,
    pub part: u32,
}

impl Node {
    pub(crate) fn new(entity: Entity, local: Transform, relative: Transform, parent: u32) -> Self {
        Self {
            entity,
            local,
            relative,
            parent,
            part: NONE,
        }
    }
}

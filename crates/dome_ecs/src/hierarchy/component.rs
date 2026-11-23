use crate::Entity;
use dome_math::Transform;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalTransform(pub Transform);

#[derive(Debug, PartialEq, Eq)]
pub struct Parent {
    entity: Entity,
}

impl Parent {
    pub(crate) fn new(entity: Entity) -> Self {
        Self { entity }
    }

    pub const fn entity(&self) -> Entity {
        self.entity
    }
}

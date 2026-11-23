use crate::collider::Collider;
use crate::shape::Shape;
use crate::solid::Solid;
use dome_ecs::Entity;
use dome_math::{Aabb, Quat, Transform, Vec3};

pub(crate) struct Part {
    pub entity: Entity,
    pub collider: Collider,
    pub shape: Shape,
    pub offset: Vec3,
    pub rotation: Quat,
    pub relative: Transform,
}

impl Part {
    pub(crate) fn new(
        entity: Entity,
        collider: Collider,
        relative: Transform,
        root_scale: Vec3,
    ) -> Self {
        let scale = root_scale * relative.scale;
        for held in [root_scale, scale] {
            let tolerance = 1e-4 * held.x.abs().max(held.y.abs()).max(held.z.abs());
            assert!(
                held.x > 0.0
                    && (held.x - held.y).abs() <= tolerance
                    && (held.x - held.z).abs() <= tolerance,
                "the collider of entity {} shrinks its body unevenly",
                entity.index(),
            );
        }
        let center = relative
            .rotation
            .rotate_vec3(relative.scale * collider.center());
        let offset = (relative.translation + center) * root_scale;
        Self {
            entity,
            shape: collider.shape().scaled(scale.x),
            collider,
            offset,
            rotation: relative.rotation,
            relative,
        }
    }

    pub(crate) fn world_position(&self, position: Vec3, rotation: Quat) -> Vec3 {
        position + rotation.rotate_vec3(self.offset)
    }

    pub(crate) fn world_rotation(&self, rotation: Quat) -> Quat {
        rotation * self.rotation
    }

    pub(crate) fn aabb(&self, position: Vec3, rotation: Quat) -> Aabb {
        self.shape.aabb(
            self.world_position(position, rotation),
            self.world_rotation(rotation),
        )
    }

    pub(crate) fn solid(&self, position: Vec3, rotation: Quat) -> Solid {
        Solid::new(
            self.world_position(position, rotation),
            self.world_rotation(rotation),
            self.shape.clone(),
        )
    }
}

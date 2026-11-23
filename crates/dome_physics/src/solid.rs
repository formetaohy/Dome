use crate::Shape;
use dome_math::{Quat, Vec3};

#[derive(Clone, Debug)]
pub(crate) struct Solid {
    pub position: Vec3,
    pub rotation: Quat,
    pub shape: Shape,
}

impl Solid {
    pub(crate) fn new(position: Vec3, rotation: Quat, shape: Shape) -> Self {
        Self {
            position,
            rotation,
            shape,
        }
    }

    pub(crate) fn to_local(&self, point: Vec3) -> Vec3 {
        self.rotation.inverse().rotate_vec3(point - self.position)
    }

    pub(crate) fn to_world(&self, point: Vec3) -> Vec3 {
        self.position + self.rotation.rotate_vec3(point)
    }
}

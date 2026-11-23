use crate::{Mat4, Quat, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub const IDENTITY: Self =
        Self::from_scale_rotation_translation(Vec3::ONE, Quat::IDENTITY, Vec3::ZERO);

    pub const fn from_translation(translation: Vec3) -> Self {
        Self::from_scale_rotation_translation(Vec3::ONE, Quat::IDENTITY, translation)
    }

    pub const fn from_rotation(rotation: Quat) -> Self {
        Self::from_scale_rotation_translation(Vec3::ONE, rotation, Vec3::ZERO)
    }

    pub const fn from_scale(scale: Vec3) -> Self {
        Self::from_scale_rotation_translation(scale, Quat::IDENTITY, Vec3::ZERO)
    }

    pub const fn from_scale_rotation_translation(
        scale: Vec3,
        rotation: Quat,
        translation: Vec3,
    ) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    pub fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    pub fn transform_point(self, point: Vec3) -> Vec3 {
        self.rotation.rotate_vec3(self.scale * point) + self.translation
    }

    pub fn transform_vector(self, vector: Vec3) -> Vec3 {
        self.rotation.rotate_vec3(self.scale * vector)
    }

    pub fn compose(self, local: Self) -> Self {
        Self {
            translation: self.transform_point(local.translation),
            rotation: self.rotation * local.rotation,
            scale: Vec3::new(
                self.scale.x * local.scale.x,
                self.scale.y * local.scale.y,
                self.scale.z * local.scale.z,
            ),
        }
    }

    pub fn decompose(self, world: Self) -> Self {
        assert!(
            self.scale.x != 0.0 && self.scale.y != 0.0 && self.scale.z != 0.0,
            "a transform of scale {:?} maps no local point to a world point",
            self.scale,
        );
        let inverse = self.rotation.inverse();
        Self {
            translation: inverse.rotate_vec3(Vec3::new(
                (world.translation.x - self.translation.x) / self.scale.x,
                (world.translation.y - self.translation.y) / self.scale.y,
                (world.translation.z - self.translation.z) / self.scale.z,
            )),
            rotation: inverse * world.rotation,
            scale: Vec3::new(
                world.scale.x / self.scale.x,
                world.scale.y / self.scale.y,
                world.scale.z / self.scale.z,
            ),
        }
    }

    pub fn inverse_transform_point(self, point: Vec3) -> Vec3 {
        assert!(
            self.scale.x != 0.0 && self.scale.y != 0.0 && self.scale.z != 0.0,
            "a transform of scale {:?} maps no world point to a local point",
            self.scale,
        );
        self.rotation.inverse().rotate_vec3(Vec3::new(
            (point.x - self.translation.x) / self.scale.x,
            (point.y - self.translation.y) / self.scale.y,
            (point.z - self.translation.z) / self.scale.z,
        ))
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

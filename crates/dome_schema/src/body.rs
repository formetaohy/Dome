use dome_math::Vec3;
use dome_physics::{BodyKind, RigidBody};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyRef {
    pub kind: BodyKind,
    pub mass: f32,
    pub gravity_scale: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub can_sleep: bool,
    pub sleeping: bool,
}

impl BodyRef {
    pub fn new(kind: BodyKind) -> Self {
        Self {
            kind,
            mass: 1.0,
            gravity_scale: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            can_sleep: true,
            sleeping: false,
        }
    }

    pub fn rigid(&self) -> RigidBody {
        let mut body = RigidBody::new(self.kind);
        body.set_mass(self.mass);
        body.set_gravity_scale(self.gravity_scale);
        body.set_linear_damping(self.linear_damping);
        body.set_angular_damping(self.angular_damping);
        body.set_linear_velocity(self.linear_velocity);
        body.set_angular_velocity(self.angular_velocity);
        body.set_can_sleep(self.can_sleep);
        body
    }

    pub(crate) fn observe(body: &RigidBody) -> Self {
        Self {
            kind: body.kind(),
            mass: body.mass(),
            gravity_scale: body.gravity_scale(),
            linear_damping: body.linear_damping(),
            angular_damping: body.angular_damping(),
            linear_velocity: body.linear_velocity(),
            angular_velocity: body.angular_velocity(),
            can_sleep: body.can_sleep(),
            sleeping: body.is_sleeping(),
        }
    }
}

impl Default for BodyRef {
    fn default() -> Self {
        Self::new(BodyKind::Dynamic)
    }
}

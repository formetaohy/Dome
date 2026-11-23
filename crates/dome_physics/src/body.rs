use crate::island::{SLEEP_ANGULAR, SLEEP_LINEAR};
use crate::part::Part;
use dome_ecs::{Entity, LocalTransform, World};
use dome_math::{Aabb, Mat3, Quat, Transform, Vec3};

const DEFAULT_MASS: f32 = 1.0;
const BROAD_MARGIN: f32 = 0.02;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Static,
    Dynamic,
    Kinematic,
}

impl BodyKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Dynamic => "dynamic",
            Self::Kinematic => "kinematic",
        }
    }

    pub fn of(name: &str) -> Option<Self> {
        match name {
            "static" => Some(Self::Static),
            "dynamic" => Some(Self::Dynamic),
            "kinematic" => Some(Self::Kinematic),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidBody {
    pub(crate) kind: BodyKind,
    pub(crate) mass: f32,
    pub(crate) gravity_scale: f32,
    pub(crate) linear_damping: f32,
    pub(crate) angular_damping: f32,
    pub(crate) linear_velocity: Vec3,
    pub(crate) angular_velocity: Vec3,
    pub(crate) force: Vec3,
    pub(crate) torque: Vec3,
    pub(crate) can_sleep: bool,
    pub(crate) sleeping: bool,
    pub(crate) sleep_time: f32,
}

impl RigidBody {
    pub fn new(kind: BodyKind) -> Self {
        Self {
            kind,
            mass: DEFAULT_MASS,
            gravity_scale: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            force: Vec3::ZERO,
            torque: Vec3::ZERO,
            can_sleep: true,
            sleeping: false,
            sleep_time: 0.0,
        }
    }

    pub fn kind(&self) -> BodyKind {
        self.kind
    }

    pub fn set_kind(&mut self, kind: BodyKind) {
        self.kind = kind;
        self.wake();
    }

    pub fn mass(&self) -> f32 {
        self.mass
    }

    pub fn set_mass(&mut self, mass: f32) {
        assert!(
            mass.is_finite() && mass > 0.0,
            "a rigid body of mass {mass} carries no inertia",
        );
        self.mass = mass;
    }

    pub fn gravity_scale(&self) -> f32 {
        self.gravity_scale
    }

    pub fn set_gravity_scale(&mut self, scale: f32) {
        assert!(
            scale.is_finite(),
            "a rigid body of gravity scale {scale} falls nowhere",
        );
        self.gravity_scale = scale;
    }

    pub fn linear_damping(&self) -> f32 {
        self.linear_damping
    }

    pub fn set_linear_damping(&mut self, damping: f32) {
        assert!(
            damping.is_finite() && damping >= 0.0,
            "a rigid body of linear damping {damping} gains speed",
        );
        self.linear_damping = damping;
    }

    pub fn angular_damping(&self) -> f32 {
        self.angular_damping
    }

    pub fn set_angular_damping(&mut self, damping: f32) {
        assert!(
            damping.is_finite() && damping >= 0.0,
            "a rigid body of angular damping {damping} spins faster",
        );
        self.angular_damping = damping;
    }

    pub fn linear_velocity(&self) -> Vec3 {
        self.linear_velocity
    }

    pub fn set_linear_velocity(&mut self, velocity: Vec3) {
        assert!(
            velocity.x.is_finite() && velocity.y.is_finite() && velocity.z.is_finite(),
            "a rigid body of linear velocity {velocity:?} moves nowhere",
        );
        self.linear_velocity = velocity;
        self.wake();
    }

    pub fn angular_velocity(&self) -> Vec3 {
        self.angular_velocity
    }

    pub fn set_angular_velocity(&mut self, velocity: Vec3) {
        assert!(
            velocity.x.is_finite() && velocity.y.is_finite() && velocity.z.is_finite(),
            "a rigid body of angular velocity {velocity:?} spins nowhere",
        );
        self.angular_velocity = velocity;
        self.wake();
    }

    pub fn apply_force(&mut self, force: Vec3) {
        assert!(
            force.x.is_finite() && force.y.is_finite() && force.z.is_finite(),
            "a rigid body of force {force:?} pushes nowhere",
        );
        self.force += force;
        self.wake();
    }

    pub fn apply_torque(&mut self, torque: Vec3) {
        assert!(
            torque.x.is_finite() && torque.y.is_finite() && torque.z.is_finite(),
            "a rigid body of torque {torque:?} turns nowhere",
        );
        self.torque += torque;
        self.wake();
    }

    pub fn can_sleep(&self) -> bool {
        self.can_sleep
    }

    pub fn set_can_sleep(&mut self, can_sleep: bool) {
        self.can_sleep = can_sleep;
    }

    pub fn is_sleeping(&self) -> bool {
        self.sleeping
    }

    pub fn set_sleeping(&mut self, sleeping: bool) {
        self.sleeping = sleeping && self.can_sleep;
        self.sleep_time = 0.0;
        if self.sleeping {
            self.linear_velocity = Vec3::ZERO;
            self.angular_velocity = Vec3::ZERO;
            self.force = Vec3::ZERO;
            self.torque = Vec3::ZERO;
        }
    }

    pub(crate) fn sleep_time(&self) -> f32 {
        self.sleep_time
    }

    pub(crate) fn set_sleep_time(&mut self, sleep_time: f32) {
        self.sleep_time = sleep_time;
    }

    pub fn wake(&mut self) {
        self.sleeping = false;
        self.sleep_time = 0.0;
    }

    pub(crate) fn clear_accumulators(&mut self) {
        self.force = Vec3::ZERO;
        self.torque = Vec3::ZERO;
    }
}

impl Default for RigidBody {
    fn default() -> Self {
        Self::new(BodyKind::Dynamic)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Travel {
    pub linear: Vec3,
    pub angular: f32,
}

impl Travel {
    pub(crate) const STILL: Self = Self {
        linear: Vec3::ZERO,
        angular: 0.0,
    };
}

#[derive(Clone, Copy)]
pub(crate) struct Velocity {
    pub linear: Vec3,
    pub angular: Vec3,
}

impl Velocity {
    pub(crate) const STILL: Self = Self {
        linear: Vec3::ZERO,
        angular: Vec3::ZERO,
    };

    pub(crate) fn of(previous: Transform, current: Transform, delta: f32) -> Self {
        let relative = current.rotation * previous.rotation.inverse();
        let (axis, angle) = relative.to_axis_angle();
        Self {
            linear: (current.translation - previous.translation) / delta,
            angular: axis * (angle / delta),
        }
    }

    pub(crate) fn moves(self) -> bool {
        self.linear.length_squared() > 1e-8 || self.angular.length_squared() > 1e-6
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LastPose(pub Transform);

pub(crate) struct Body {
    pub entity: Entity,
    pub rigid: RigidBody,
    pub has_body: bool,
    pub parts: Vec<Part>,
    pub position: Vec3,
    pub rotation: Quat,
    pub center_offset: Vec3,
    pub transform: Transform,
    pub frame: Option<Transform>,
    pub carried: Velocity,
    pub inverse_mass: f32,
    pub inverse_inertia: Mat3,
}

impl Body {
    pub(crate) fn new(
        entity: Entity,
        rigid: Option<RigidBody>,
        mut parts: Vec<Part>,
        transform: Transform,
        frame: Option<Transform>,
        carried: Velocity,
    ) -> Self {
        let has_body = rigid.is_some();
        let rigid = rigid.unwrap_or_else(|| RigidBody::new(BodyKind::Static));
        assert!(
            rigid.kind != BodyKind::Dynamic || !parts.is_empty(),
            "the dynamic body of entity {} holds no collider",
            entity.index(),
        );
        let rotation = transform.rotation.normalize();
        let dynamic = rigid.kind == BodyKind::Dynamic;
        let (center_offset, inverse_inertia) = if dynamic {
            let (center, inverse) = mass_properties(&parts, rigid.mass);
            for part in &mut parts {
                part.offset -= center;
            }
            let matrix = rotation.to_mat3();
            (center, matrix * inverse * matrix.transpose())
        } else {
            (Vec3::ZERO, Mat3::from_diagonal(Vec3::ZERO))
        };
        Self {
            entity,
            rigid,
            has_body,
            parts,
            position: transform.translation + rotation.rotate_vec3(center_offset),
            rotation,
            center_offset,
            transform,
            frame,
            carried,
            inverse_mass: if dynamic { 1.0 / rigid.mass } else { 0.0 },
            inverse_inertia,
        }
    }

    pub(crate) fn aabb(&self) -> Option<Aabb> {
        self.parts
            .iter()
            .map(|part| part.aabb(self.position, self.rotation))
            .reduce(Aabb::union)
            .map(|aabb| Aabb {
                min: aabb.min - Vec3::splat(BROAD_MARGIN),
                max: aabb.max + Vec3::splat(BROAD_MARGIN),
            })
    }

    pub(crate) fn swept_aabb(&self, delta: f32) -> Option<(Aabb, Travel)> {
        let aabb = self.aabb()?;
        if self.rigid.kind == BodyKind::Static {
            return Some((aabb, Travel::STILL));
        }
        let reach = aabb.half_extents().length();
        let velocity = self.velocity();
        let travel = Travel {
            linear: velocity.linear * delta,
            angular: velocity.angular.length() * reach * delta,
        };
        let margin = travel.linear.abs() + Vec3::splat(travel.angular);
        Some((Aabb::new(aabb.min - margin, aabb.max + margin), travel))
    }

    pub(crate) fn anchor_offset(&self, anchor: Vec3) -> Vec3 {
        let scale = self.transform.scale;
        let scaled = Vec3::new(anchor.x * scale.x, anchor.y * scale.y, anchor.z * scale.z);
        self.rotation.rotate_vec3(scaled - self.center_offset)
    }

    pub(crate) fn velocity(&self) -> Velocity {
        Velocity {
            linear: self.rigid.linear_velocity + self.carried.linear,
            angular: self.rigid.angular_velocity + self.carried.angular,
        }
    }

    pub(crate) fn active(&self) -> bool {
        match self.rigid.kind {
            BodyKind::Static => false,
            BodyKind::Dynamic => !self.rigid.sleeping,
            BodyKind::Kinematic => self.velocity().moves(),
        }
    }

    pub(crate) fn pushable(&self) -> bool {
        self.rigid.kind == BodyKind::Dynamic
    }

    pub(crate) fn movable(&self) -> bool {
        self.rigid.kind != BodyKind::Static
    }

    pub(crate) fn senses(&self) -> bool {
        self.parts.iter().any(|part| part.collider.is_sensor())
    }

    pub(crate) fn sleeping(&self) -> bool {
        self.rigid.sleeping
    }

    pub(crate) fn wake(&mut self) {
        self.rigid.wake();
    }

    pub(crate) fn point_velocity(&self, offset: Vec3) -> Vec3 {
        self.rigid.linear_velocity
            + self.rigid.angular_velocity.cross(offset)
            + self.carried.linear
            + self.carried.angular.cross(offset)
    }

    pub(crate) fn apply_impulse(&mut self, offset: Vec3, impulse: Vec3) {
        self.rigid.linear_velocity += impulse * self.inverse_mass;
        self.rigid.angular_velocity += self.inverse_inertia.mul_vec3(offset.cross(impulse));
    }

    pub(crate) fn apply_angular_impulse(&mut self, impulse: Vec3) {
        self.rigid.angular_velocity += self.inverse_inertia.mul_vec3(impulse);
    }

    pub(crate) fn apply_position_impulse(&mut self, offset: Vec3, impulse: Vec3) {
        self.position += impulse * self.inverse_mass;
        self.rotation = integrate_rotation(
            self.rotation,
            self.inverse_inertia.mul_vec3(offset.cross(impulse)),
        );
    }

    pub(crate) fn apply_angular_position_impulse(&mut self, impulse: Vec3) {
        self.rotation = integrate_rotation(self.rotation, self.inverse_inertia.mul_vec3(impulse));
    }

    pub(crate) fn integrate_velocities(&mut self, gravity: Vec3, delta: f32) {
        if self.rigid.kind == BodyKind::Dynamic && !self.rigid.sleeping {
            let acceleration =
                gravity * self.rigid.gravity_scale + self.rigid.force * self.inverse_mass;
            self.rigid.linear_velocity += acceleration * delta;
            self.rigid.angular_velocity += self.inverse_inertia.mul_vec3(self.rigid.torque) * delta;
            self.rigid.linear_velocity *= reciprocal(1.0 + self.rigid.linear_damping * delta);
            self.rigid.angular_velocity *= reciprocal(1.0 + self.rigid.angular_damping * delta);
        }
        self.rigid.clear_accumulators();
    }

    pub(crate) fn integrate_position(&mut self, delta: f32) {
        if self.rigid.kind == BodyKind::Static
            || (self.rigid.kind == BodyKind::Dynamic && self.rigid.sleeping)
        {
            return;
        }
        self.position += self.rigid.linear_velocity * delta;
        self.rotation = integrate_rotation(self.rotation, self.rigid.angular_velocity * delta);
    }

    pub(crate) fn slow(&self) -> bool {
        self.rigid.linear_velocity.length_squared() <= SLEEP_LINEAR * SLEEP_LINEAR
            && self.rigid.angular_velocity.length_squared() <= SLEEP_ANGULAR * SLEEP_ANGULAR
    }

    pub(crate) fn sleep(&mut self) {
        self.rigid.sleeping = true;
        self.rigid.linear_velocity = Vec3::ZERO;
        self.rigid.angular_velocity = Vec3::ZERO;
        self.rigid.sleep_time = 0.0;
    }

    pub(crate) fn sync(&self, world: &mut World) {
        if self.has_body {
            world.insert(self.entity, self.rigid);
        }
        if self.has_body && self.rigid.kind != BodyKind::Static {
            let mut transform = self.transform;
            transform.translation = self.position - self.rotation.rotate_vec3(self.center_offset);
            transform.rotation = self.rotation;
            let local = match self.frame {
                Some(frame) => frame.decompose(transform),
                None => transform,
            };
            world.insert(self.entity, LastPose(transform));
            world.insert(self.entity, LocalTransform(local));
            world.insert(self.entity, transform);
            for part in &self.parts {
                if part.entity != self.entity {
                    world.insert(part.entity, transform.compose(part.relative));
                }
            }
        }
    }
}

fn mass_properties(parts: &[Part], mass: f32) -> (Vec3, Mat3) {
    let total = parts.iter().map(|part| part.shape.volume()).sum::<f32>();
    assert!(total > 0.0, "a dynamic body holds colliders of no volume",);
    let center = parts.iter().fold(Vec3::ZERO, |held, part| {
        held + part.offset * (part.shape.volume() / total)
    });
    let mut columns = [Vec3::ZERO; 3];
    for part in parts {
        let share = mass * (part.shape.volume() / total);
        let inertia = part.shape.inertia(share);
        let matrix = part.rotation.to_mat3();
        let tensor = matrix * Mat3::from_diagonal(inertia) * matrix.transpose();
        let span = part.offset - center;
        let parallel = (Mat3::from_diagonal(Vec3::splat(span.dot(span)))
            - Mat3::from_cols(span * span.x, span * span.y, span * span.z))
            * share;
        for (axis, column) in columns.iter_mut().enumerate() {
            *column += tensor.col(axis) + parallel.col(axis);
        }
    }
    let inverse = Mat3::from_cols(columns[0], columns[1], columns[2]).inverse();
    (center, inverse)
}

pub(crate) fn integrate_rotation(rotation: Quat, velocity: Vec3) -> Quat {
    let angle = velocity.length();
    if angle <= 1e-9 {
        return rotation;
    }
    (Quat::from_axis_angle(velocity / angle, angle) * rotation).normalize()
}

fn reciprocal(value: f32) -> f32 {
    if value > 0.0 { 1.0 / value } else { 0.0 }
}

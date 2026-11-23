use dome_ecs::Entity;
use dome_math::{Quat, Vec3};
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointKind {
    Fixed,
    Spherical,
    Hinge,
    Prismatic,
    Distance,
}

const LOCKED: &[Coordinate] = &[];
const TWIST: &[Coordinate] = &[Coordinate::Twist];
const BALL: &[Coordinate] = &[Coordinate::Twist, Coordinate::Swing];
const SLIDE: &[Coordinate] = &[Coordinate::Slide];
const SPAN: &[Coordinate] = &[Coordinate::Span];

impl JointKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Spherical => "spherical",
            Self::Hinge => "hinge",
            Self::Prismatic => "prismatic",
            Self::Distance => "distance",
        }
    }

    pub fn of(name: &str) -> Option<Self> {
        match name {
            "fixed" => Some(Self::Fixed),
            "spherical" => Some(Self::Spherical),
            "hinge" => Some(Self::Hinge),
            "prismatic" => Some(Self::Prismatic),
            "distance" => Some(Self::Distance),
            _ => None,
        }
    }

    pub const fn coordinates(self) -> &'static [Coordinate] {
        match self {
            Self::Fixed => LOCKED,
            Self::Spherical => BALL,
            Self::Hinge => TWIST,
            Self::Prismatic => SLIDE,
            Self::Distance => SPAN,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coordinate {
    Twist,
    Swing,
    Slide,
    Span,
}

pub(crate) const COORDINATES: usize = Coordinate::ALL.len();

impl Coordinate {
    pub const ALL: [Self; 4] = [Self::Twist, Self::Swing, Self::Slide, Self::Span];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Twist => "twist",
            Self::Swing => "swing",
            Self::Slide => "slide",
            Self::Span => "span",
        }
    }

    pub const fn angular(self) -> bool {
        matches!(self, Self::Twist | Self::Swing)
    }

    pub(crate) const fn slot(self) -> usize {
        match self {
            Self::Twist => 0,
            Self::Swing => 1,
            Self::Slide => 2,
            Self::Span => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limit {
    lower: f32,
    upper: f32,
}

impl Limit {
    pub fn new(lower: f32, upper: f32) -> Self {
        assert!(
            lower.is_finite() && upper.is_finite() && lower <= upper,
            "a joint limit of {lower} to {upper} spans no coordinate",
        );
        Self { lower, upper }
    }

    pub fn lower(self) -> f32 {
        self.lower
    }

    pub fn upper(self) -> f32 {
        self.upper
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motor {
    speed: f32,
    force: f32,
}

impl Motor {
    pub fn new(speed: f32, force: f32) -> Self {
        assert!(
            speed.is_finite(),
            "a joint motor of speed {speed} drives nowhere",
        );
        assert!(
            force.is_finite() && force > 0.0,
            "a joint motor of force {force} drives nothing",
        );
        Self { speed, force }
    }

    pub fn speed(self) -> f32 {
        self.speed
    }

    pub fn force(self) -> f32 {
        self.force
    }

    pub fn set_speed(&mut self, speed: f32) {
        assert!(
            speed.is_finite(),
            "a joint motor of speed {speed} drives nowhere",
        );
        self.speed = speed;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Drive {
    limit: Option<Limit>,
    motor: Option<Motor>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DriveCache {
    pub(crate) limit: [f32; 2],
    pub(crate) motor: f32,
    pub(crate) value: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct JointCache {
    pub(crate) linear: [f32; 3],
    pub(crate) angular: [f32; 3],
    pub(crate) drives: [DriveCache; COORDINATES],
}

#[derive(Clone, Copy, Debug)]
pub struct Joint {
    other: Entity,
    kind: JointKind,
    anchor: Vec3,
    other_anchor: Vec3,
    axis: Vec3,
    other_axis: Vec3,
    reference: Quat,
    rest_length: f32,
    collide: bool,
    drives: [Drive; COORDINATES],
    cache: JointCache,
}

struct Frame {
    anchor: Vec3,
    other_anchor: Vec3,
    axis: Vec3,
    other_axis: Vec3,
    reference: Quat,
}

impl Joint {
    pub fn fixed(
        other: Entity,
        anchor: Vec3,
        other_anchor: Vec3,
        first: Quat,
        second: Quat,
    ) -> Self {
        Self::new(
            other,
            JointKind::Fixed,
            Frame {
                anchor,
                other_anchor,
                axis: Vec3::Y,
                other_axis: Vec3::Y,
                reference: reference_of(first, second),
            },
            0.0,
        )
    }

    pub fn spherical(
        other: Entity,
        anchor: Vec3,
        other_anchor: Vec3,
        axis: Vec3,
        first: Quat,
        second: Quat,
    ) -> Self {
        Self::new(
            other,
            JointKind::Spherical,
            Frame {
                anchor,
                other_anchor,
                axis,
                other_axis: Vec3::Y,
                reference: reference_of(first, second),
            },
            0.0,
        )
    }

    pub fn hinge(
        other: Entity,
        anchor: Vec3,
        other_anchor: Vec3,
        axis: Vec3,
        other_axis: Vec3,
        first: Quat,
        second: Quat,
    ) -> Self {
        Self::new(
            other,
            JointKind::Hinge,
            Frame {
                anchor,
                other_anchor,
                axis,
                other_axis,
                reference: reference_of(first, second),
            },
            0.0,
        )
    }

    pub fn prismatic(
        other: Entity,
        anchor: Vec3,
        other_anchor: Vec3,
        axis: Vec3,
        other_axis: Vec3,
        first: Quat,
        second: Quat,
    ) -> Self {
        Self::new(
            other,
            JointKind::Prismatic,
            Frame {
                anchor,
                other_anchor,
                axis,
                other_axis,
                reference: reference_of(first, second),
            },
            0.0,
        )
    }

    pub fn distance(other: Entity, anchor: Vec3, other_anchor: Vec3, rest_length: f32) -> Self {
        Self::new(
            other,
            JointKind::Distance,
            Frame {
                anchor,
                other_anchor,
                axis: Vec3::Y,
                other_axis: Vec3::Y,
                reference: Quat::IDENTITY,
            },
            rest_length,
        )
    }

    fn new(other: Entity, kind: JointKind, frame: Frame, rest_length: f32) -> Self {
        for point in [frame.anchor, frame.other_anchor] {
            assert!(
                point.x.is_finite() && point.y.is_finite() && point.z.is_finite(),
                "a joint anchor at {point:?} sits nowhere",
            );
        }
        assert!(
            rest_length.is_finite() && rest_length >= 0.0,
            "a joint of rest length {rest_length} spans no distance",
        );
        Self {
            other,
            kind,
            anchor: frame.anchor,
            other_anchor: frame.other_anchor,
            axis: unit(frame.axis, "the joint axis"),
            other_axis: unit(frame.other_axis, "the joint other axis"),
            reference: frame.reference.normalize(),
            rest_length,
            collide: false,
            drives: [Drive::default(); COORDINATES],
            cache: JointCache::default(),
        }
    }

    pub fn with_collide(mut self, collide: bool) -> Self {
        self.collide = collide;
        self
    }

    pub fn with_limit(mut self, lower: f32, upper: f32) -> Self {
        let coordinate = self.driving();
        self.drives[coordinate.slot()].limit = Some(Limit::new(lower, upper));
        self
    }

    pub fn with_cone(mut self, angle: f32) -> Self {
        assert!(
            angle.is_finite() && (0.0..=PI).contains(&angle),
            "a spherical joint of cone {angle} swings nowhere",
        );
        assert!(
            self.kind.coordinates().contains(&Coordinate::Swing),
            "a {} joint carries no cone to limit",
            self.kind.name(),
        );
        self.drives[Coordinate::Swing.slot()].limit = Some(Limit::new(0.0, angle));
        self
    }

    pub fn with_motor(mut self, speed: f32, force: f32) -> Self {
        let coordinate = self.driving();
        self.drives[coordinate.slot()].motor = Some(Motor::new(speed, force));
        self
    }

    pub fn set_motor_speed(&mut self, speed: f32) {
        let coordinate = self.driving();
        let Some(motor) = self.drives[coordinate.slot()].motor.as_mut() else {
            panic!("a joint without a motor drives no coordinate");
        };
        motor.set_speed(speed);
    }

    pub fn other(&self) -> Entity {
        self.other
    }

    pub fn kind(&self) -> JointKind {
        self.kind
    }

    pub fn anchor(&self) -> Vec3 {
        self.anchor
    }

    pub fn other_anchor(&self) -> Vec3 {
        self.other_anchor
    }

    pub fn axis(&self) -> Vec3 {
        self.axis
    }

    pub fn other_axis(&self) -> Vec3 {
        self.other_axis
    }

    pub fn reference(&self) -> Quat {
        self.reference
    }

    pub fn rest_length(&self) -> f32 {
        self.rest_length
    }

    pub fn collide(&self) -> bool {
        self.collide
    }

    pub fn limit(&self, coordinate: Coordinate) -> Option<Limit> {
        self.drives[coordinate.slot()].limit
    }

    pub fn motor(&self, coordinate: Coordinate) -> Option<Motor> {
        self.drives[coordinate.slot()].motor
    }

    pub fn value(&self, coordinate: Coordinate) -> f32 {
        self.cache.drives[coordinate.slot()].value
    }

    pub fn coordinate(&self) -> f32 {
        self.primary()
            .map_or(0.0, |coordinate| self.value(coordinate))
    }

    pub fn driven(&self) -> bool {
        self.kind
            .coordinates()
            .iter()
            .any(|coordinate| self.motor(*coordinate).is_some())
    }

    pub(crate) fn cache(&self) -> JointCache {
        self.cache
    }

    pub(crate) fn set_cache(&mut self, cache: JointCache) {
        self.cache = cache;
    }

    fn primary(&self) -> Option<Coordinate> {
        self.kind.coordinates().first().copied()
    }

    fn driving(&self) -> Coordinate {
        self.primary().unwrap_or_else(|| {
            panic!(
                "a {} joint carries no coordinate to limit or drive",
                self.kind.name(),
            )
        })
    }
}

fn unit(direction: Vec3, what: &str) -> Vec3 {
    assert!(
        direction.x.is_finite() && direction.y.is_finite() && direction.z.is_finite(),
        "{what} of {direction:?} points nowhere",
    );
    assert!(
        direction.length_squared() > 1e-12,
        "{what} of {direction:?} points nowhere",
    );
    direction.normalize()
}

fn reference_of(first: Quat, second: Quat) -> Quat {
    first.normalize().inverse() * second.normalize()
}

use crate::CollisionFilter;
use crate::Shape;
use dome_math::Vec3;

#[derive(Clone, Debug, PartialEq)]
pub struct Collider {
    shape: Shape,
    center: Vec3,
    friction: f32,
    restitution: f32,
    sensor: bool,
    filter: CollisionFilter,
}

impl Collider {
    pub fn new(shape: Shape) -> Self {
        shape.validate();
        Self {
            shape,
            center: Vec3::ZERO,
            friction: 0.5,
            restitution: 0.0,
            sensor: false,
            filter: CollisionFilter::DEFAULT,
        }
    }

    pub fn shape(&self) -> Shape {
        self.shape.clone()
    }

    pub fn set_shape(&mut self, shape: Shape) {
        shape.validate();
        self.shape = shape;
    }

    pub fn center(&self) -> Vec3 {
        self.center
    }

    pub fn set_center(&mut self, center: Vec3) {
        assert!(
            center.x.is_finite() && center.y.is_finite() && center.z.is_finite(),
            "a collider centered at {center:?} sits nowhere",
        );
        self.center = center;
    }

    pub fn friction(&self) -> f32 {
        self.friction
    }

    pub fn set_friction(&mut self, friction: f32) {
        assert!(
            friction.is_finite() && (0.0..=1.0).contains(&friction),
            "a collider of friction {friction} lies outside the unit range",
        );
        self.friction = friction;
    }

    pub fn restitution(&self) -> f32 {
        self.restitution
    }

    pub fn set_restitution(&mut self, restitution: f32) {
        assert!(
            restitution.is_finite() && (0.0..=1.0).contains(&restitution),
            "a collider of restitution {restitution} lies outside the unit range",
        );
        self.restitution = restitution;
    }

    pub fn is_sensor(&self) -> bool {
        self.sensor
    }

    pub fn set_sensor(&mut self, sensor: bool) {
        self.sensor = sensor;
    }

    pub fn filter(&self) -> CollisionFilter {
        self.filter
    }

    pub fn set_filter(&mut self, filter: CollisionFilter) {
        self.filter = filter;
    }
}

impl Default for Collider {
    fn default() -> Self {
        Self::new(Shape::cuboid(Vec3::splat(0.5)))
    }
}

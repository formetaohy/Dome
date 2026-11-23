use crate::assets::Assets;
use dome_math::Vec3;
use dome_physics::{ALL_LAYERS, Collider, CollisionFilter, Shape};
use dome_scene::MeshId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Sphere,
    Cuboid,
    Capsule,
    Mesh,
}

impl ShapeKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sphere => "sphere",
            Self::Cuboid => "cuboid",
            Self::Capsule => "capsule",
            Self::Mesh => "mesh",
        }
    }

    pub fn of(name: &str) -> Option<Self> {
        match name {
            "sphere" => Some(Self::Sphere),
            "cuboid" => Some(Self::Cuboid),
            "capsule" => Some(Self::Capsule),
            "mesh" => Some(Self::Mesh),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColliderRef {
    pub shape: ShapeKind,
    pub mesh: Option<MeshId>,
    pub radius: f32,
    pub half_extents: Vec3,
    pub half_height: f32,
    pub center: Vec3,
    pub friction: f32,
    pub restitution: f32,
    pub sensor: bool,
    pub layer: u16,
    pub mask: u16,
}

impl ColliderRef {
    pub fn new() -> Self {
        Self {
            shape: ShapeKind::Cuboid,
            mesh: None,
            radius: 0.5,
            half_extents: Vec3::splat(0.5),
            half_height: 0.5,
            center: Vec3::ZERO,
            friction: 0.5,
            restitution: 0.0,
            sensor: false,
            layer: CollisionFilter::DEFAULT.layer(),
            mask: ALL_LAYERS,
        }
    }

    pub fn collider(&self, assets: &dyn Assets) -> Collider {
        let shape = match self.shape {
            ShapeKind::Sphere => Shape::sphere(self.radius),
            ShapeKind::Cuboid => Shape::cuboid(self.half_extents),
            ShapeKind::Capsule => Shape::capsule(self.radius, self.half_height),
            ShapeKind::Mesh => {
                let mesh = self
                    .mesh
                    .as_ref()
                    .unwrap_or_else(|| panic!("a mesh collider names no mesh"));
                Shape::mesh(assets.mesh_shape(mesh.as_str()))
            }
        };
        let mut collider = Collider::new(shape);
        collider.set_center(self.center);
        collider.set_friction(self.friction);
        collider.set_restitution(self.restitution);
        collider.set_sensor(self.sensor);
        collider.set_filter(CollisionFilter::new(self.layer, self.mask));
        collider
    }

    pub(crate) fn observe(collider: &Collider) -> Self {
        let mut held = Self::new();
        held.shape = match collider.shape() {
            Shape::Sphere { .. } => ShapeKind::Sphere,
            Shape::Cuboid { .. } => ShapeKind::Cuboid,
            Shape::Capsule { .. } => ShapeKind::Capsule,
            Shape::Mesh { .. } => ShapeKind::Mesh,
        };
        held.mesh = None;
        match collider.shape() {
            Shape::Sphere { radius } => held.radius = radius,
            Shape::Cuboid { half_extents } => held.half_extents = half_extents,
            Shape::Capsule {
                radius,
                half_height,
            } => {
                held.radius = radius;
                held.half_height = half_height;
            }
            Shape::Mesh { .. } => {}
        }
        held.center = collider.center();
        held.friction = collider.friction();
        held.restitution = collider.restitution();
        held.sensor = collider.is_sensor();
        held.layer = collider.filter().layer();
        held.mask = collider.filter().mask();
        held
    }
}

impl Default for ColliderRef {
    fn default() -> Self {
        Self::new()
    }
}

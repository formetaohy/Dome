use crate::mesh::TriangleMesh;
use dome_math::{Aabb, Quat, Vec3};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum Shape {
    Sphere { radius: f32 },
    Cuboid { half_extents: Vec3 },
    Capsule { radius: f32, half_height: f32 },
    Mesh { mesh: Arc<TriangleMesh>, scale: f32 },
}

impl PartialEq for Shape {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Sphere { radius: left }, Self::Sphere { radius: right }) => left == right,
            (
                Self::Cuboid { half_extents: left },
                Self::Cuboid {
                    half_extents: right,
                },
            ) => left == right,
            (
                Self::Capsule {
                    radius: left_radius,
                    half_height: left_height,
                },
                Self::Capsule {
                    radius: right_radius,
                    half_height: right_height,
                },
            ) => left_radius == right_radius && left_height == right_height,
            (
                Self::Mesh {
                    mesh: left_mesh,
                    scale: left_scale,
                },
                Self::Mesh {
                    mesh: right_mesh,
                    scale: right_scale,
                },
            ) => Arc::ptr_eq(left_mesh, right_mesh) && left_scale == right_scale,
            _ => false,
        }
    }
}

impl Shape {
    pub fn sphere(radius: f32) -> Self {
        assert!(
            radius.is_finite() && radius > 0.0,
            "a sphere of radius {radius} encloses no volume",
        );
        Self::Sphere { radius }
    }

    pub fn cuboid(half_extents: Vec3) -> Self {
        assert!(
            half_extents.x.is_finite()
                && half_extents.y.is_finite()
                && half_extents.z.is_finite()
                && half_extents.x > 0.0
                && half_extents.y > 0.0
                && half_extents.z > 0.0,
            "a cuboid of half extents {half_extents:?} encloses no volume",
        );
        Self::Cuboid { half_extents }
    }

    pub fn capsule(radius: f32, half_height: f32) -> Self {
        assert!(
            radius.is_finite() && radius > 0.0,
            "a capsule of radius {radius} encloses no volume",
        );
        assert!(
            half_height.is_finite() && half_height >= 0.0,
            "a capsule of half height {half_height} folds into its caps",
        );
        Self::Capsule {
            radius,
            half_height,
        }
    }

    pub fn mesh(mesh: Arc<TriangleMesh>) -> Self {
        Self::Mesh { mesh, scale: 1.0 }
    }

    pub(crate) fn validate(&self) {
        match self {
            Self::Sphere { radius } => assert!(
                radius.is_finite() && *radius > 0.0,
                "a sphere of radius {radius} encloses no volume",
            ),
            Self::Cuboid { half_extents } => assert!(
                half_extents.x.is_finite()
                    && half_extents.y.is_finite()
                    && half_extents.z.is_finite()
                    && half_extents.x > 0.0
                    && half_extents.y > 0.0
                    && half_extents.z > 0.0,
                "a cuboid of half extents {half_extents:?} encloses no volume",
            ),
            Self::Capsule {
                radius,
                half_height,
            } => {
                assert!(
                    radius.is_finite() && *radius > 0.0,
                    "a capsule of radius {radius} encloses no volume",
                );
                assert!(
                    half_height.is_finite() && *half_height >= 0.0,
                    "a capsule of half height {half_height} folds into its caps",
                );
            }
            Self::Mesh { mesh, scale } => {
                assert!(
                    mesh.triangle_count() > 0,
                    "a mesh collider holds no triangle",
                );
                assert!(
                    scale.is_finite() && *scale > 0.0,
                    "a mesh collider of scale {scale} encloses no volume",
                );
            }
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Sphere { .. } => "sphere",
            Self::Cuboid { .. } => "cuboid",
            Self::Capsule { .. } => "capsule",
            Self::Mesh { .. } => "mesh",
        }
    }

    pub(crate) fn scaled(&self, scale: f32) -> Self {
        assert!(
            scale.is_finite() && scale > 0.0,
            "a collider of scale {scale} shrinks no shape",
        );
        match self {
            Self::Sphere { radius } => Self::Sphere {
                radius: radius * scale,
            },
            Self::Cuboid { half_extents } => Self::Cuboid {
                half_extents: *half_extents * scale,
            },
            Self::Capsule {
                radius,
                half_height,
            } => Self::Capsule {
                radius: radius * scale,
                half_height: half_height * scale,
            },
            Self::Mesh { mesh, scale: held } => Self::Mesh {
                mesh: Arc::clone(mesh),
                scale: held * scale,
            },
        }
    }

    pub(crate) fn inertia(&self, mass: f32) -> Vec3 {
        let volume_factor = 1.0 / 3.0;
        match self {
            Self::Sphere { radius } => {
                let inertia = 0.4 * mass * radius * radius;
                Vec3::splat(inertia)
            }
            Self::Cuboid { half_extents } => {
                Vec3::new(
                    half_extents.y * half_extents.y + half_extents.z * half_extents.z,
                    half_extents.x * half_extents.x + half_extents.z * half_extents.z,
                    half_extents.x * half_extents.x + half_extents.y * half_extents.y,
                ) * (mass * volume_factor)
            }
            Self::Capsule {
                radius,
                half_height,
            } => {
                let cylinder = std::f32::consts::PI * radius * radius * (2.0 * half_height);
                let sphere = 4.0 / 3.0 * std::f32::consts::PI * radius * radius * radius;
                let total = cylinder + sphere;
                let cylinder_mass = mass * cylinder / total;
                let sphere_mass = mass - cylinder_mass;
                let transverse = cylinder_mass
                    * (3.0 * radius * radius + 4.0 * half_height * half_height)
                    / 12.0
                    + sphere_mass * (0.4 * radius * radius + half_height * half_height);
                let axial =
                    0.5 * cylinder_mass * radius * radius + 0.4 * sphere_mass * radius * radius;
                Vec3::new(transverse, axial, transverse)
            }
            Self::Mesh { .. } => panic!("a triangle mesh carries no inertia to simulate"),
        }
    }

    pub(crate) fn volume(&self) -> f32 {
        match self {
            Self::Sphere { radius } => 4.0 / 3.0 * std::f32::consts::PI * radius * radius * radius,
            Self::Cuboid { half_extents } => 8.0 * half_extents.x * half_extents.y * half_extents.z,
            Self::Capsule {
                radius,
                half_height,
            } => {
                std::f32::consts::PI * radius * radius * (2.0 * half_height)
                    + 4.0 / 3.0 * std::f32::consts::PI * radius * radius * radius
            }
            Self::Mesh { .. } => panic!("a triangle mesh carries no volume to simulate"),
        }
    }

    pub(crate) fn aabb(&self, position: Vec3, rotation: Quat) -> Aabb {
        match self {
            Self::Sphere { radius } => {
                Aabb::from_center_half_extents(position, Vec3::splat(*radius))
            }
            Self::Cuboid { half_extents } => {
                Aabb::from_center_half_extents(position, oriented(*half_extents, rotation))
            }
            Self::Capsule {
                radius,
                half_height,
            } => {
                let axis = rotation.rotate_vec3(Vec3::Y) * *half_height;
                let low = (position - axis).min(position + axis) - Vec3::splat(*radius);
                let high = (position - axis).max(position + axis) + Vec3::splat(*radius);
                Aabb::new(low, high)
            }
            Self::Mesh { mesh, scale } => {
                let bounds = mesh.bounds();
                Aabb::from_center_half_extents(
                    position + rotation.rotate_vec3(bounds.center() * *scale),
                    oriented(bounds.half_extents() * *scale, rotation),
                )
            }
        }
    }
}

fn oriented(half_extents: Vec3, rotation: Quat) -> Vec3 {
    let matrix = rotation.to_mat3();
    let x = matrix.col(0) * half_extents.x;
    let y = matrix.col(1) * half_extents.y;
    let z = matrix.col(2) * half_extents.z;
    Vec3::new(
        x.x.abs() + y.x.abs() + z.x.abs(),
        x.y.abs() + y.y.abs() + z.y.abs(),
        x.z.abs() + y.z.abs() + z.z.abs(),
    )
}

pub(crate) fn capsule_segment(position: Vec3, rotation: Quat, half_height: f32) -> (Vec3, Vec3) {
    let axis = rotation.rotate_vec3(Vec3::Y) * half_height;
    (position - axis, position + axis)
}

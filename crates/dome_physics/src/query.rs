use crate::solid::Solid;
use crate::{Collider, CollisionFilter, Shape};
use dome_ecs::{Entity, LocalTransform, Parent, World, world_transform};
use dome_math::{Quat, Transform, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub entity: Entity,
    pub distance: f32,
    pub point: Vec3,
    pub normal: Vec3,
}

pub fn raycast(
    world: &World,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    filter: CollisionFilter,
) -> Option<RayHit> {
    assert!(
        origin.x.is_finite() && origin.y.is_finite() && origin.z.is_finite(),
        "a ray from {origin:?} starts nowhere",
    );
    assert!(
        direction.x.is_finite() && direction.y.is_finite() && direction.z.is_finite(),
        "a ray of direction {direction:?} points nowhere",
    );
    assert!(
        max_distance.is_finite() && max_distance > 0.0,
        "a ray of reach {max_distance} reaches nowhere",
    );
    let direction = direction.normalize();
    let mut colliders = world
        .iter::<Collider>()
        .map(|(entity, collider)| (entity, collider.clone()))
        .collect::<Vec<_>>();
    colliders.sort_by_key(|(entity, _)| (entity.index(), entity.generation()));
    let mut best: Option<RayHit> = None;
    for (entity, collider) in colliders {
        if !filter.accepts(collider.filter()) {
            continue;
        }
        let transform = match world.get::<Parent>(entity) {
            Some(_) => world_transform(world, entity),
            None => world
                .get::<LocalTransform>(entity)
                .map(|held| held.0)
                .or_else(|| world.get::<Transform>(entity).copied())
                .unwrap_or_else(|| {
                    panic!(
                        "the collider of entity {} holds no local transform",
                        entity.index(),
                    )
                }),
        };
        let scale = transform.scale;
        let tolerance = 1e-4 * scale.x.abs().max(scale.y.abs()).max(scale.z.abs());
        assert!(
            scale.x > 0.0
                && (scale.x - scale.y).abs() <= tolerance
                && (scale.x - scale.z).abs() <= tolerance,
            "a collider of scale {scale:?} shrinks its body unevenly",
        );
        let rotation = transform.rotation.normalize();
        let solid = Solid::new(
            transform.translation + rotation.rotate_vec3(collider.center() * scale.x),
            rotation,
            collider.shape().scaled(scale.x),
        );
        let Some((distance, normal)) = intersect(&solid, origin, direction, max_distance) else {
            continue;
        };
        if !(0.0..=max_distance).contains(&distance) {
            continue;
        }
        if best.is_some_and(|held| held.distance <= distance) {
            continue;
        }
        best = Some(RayHit {
            entity,
            distance,
            point: origin + direction * distance,
            normal,
        });
    }
    best
}

fn intersect(
    solid: &Solid,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<(f32, Vec3)> {
    match &solid.shape {
        Shape::Sphere { radius } => sphere(solid.position, *radius, origin, direction),
        Shape::Cuboid { half_extents } => cuboid(
            solid.position,
            solid.rotation,
            *half_extents,
            origin,
            direction,
        ),
        Shape::Capsule {
            radius,
            half_height,
        } => capsule(
            solid.position,
            solid.rotation,
            *radius,
            *half_height,
            origin,
            direction,
        ),
        Shape::Mesh { mesh, scale } => {
            let rotation = solid.rotation.inverse();
            let local = rotation.rotate_vec3(origin - solid.position) / *scale;
            let direction = rotation.rotate_vec3(direction);
            let (distance, normal) = mesh.raycast(local, direction, max_distance / *scale)?;
            Some((distance * *scale, solid.rotation.rotate_vec3(normal)))
        }
    }
}

fn sphere(center: Vec3, radius: f32, origin: Vec3, direction: Vec3) -> Option<(f32, Vec3)> {
    let offset = origin - center;
    let projection = offset.dot(direction);
    let constant = offset.length_squared() - radius * radius;
    if constant > 0.0 && projection > 0.0 {
        return None;
    }
    let discriminant = projection * projection - constant;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let mut distance = -projection - root;
    if distance < 0.0 {
        distance = 0.0;
    }
    let offset = origin + direction * distance - center;
    let normal = if offset.length_squared() > 1e-12 {
        offset.normalize()
    } else {
        -direction
    };
    Some((distance, normal))
}

fn cuboid(
    center: Vec3,
    rotation: Quat,
    half_extents: Vec3,
    origin: Vec3,
    direction: Vec3,
) -> Option<(f32, Vec3)> {
    let inverse = rotation.inverse();
    let local_origin = inverse.rotate_vec3(origin - center);
    let local_direction = inverse.rotate_vec3(direction);
    let origin = local_origin.to_array();
    let direction = local_direction.to_array();
    let half = half_extents.to_array();
    let mut near = f32::NEG_INFINITY;
    let mut far = f32::INFINITY;
    let mut axis = 0;
    let mut sign = 1.0;
    for held in 0..3 {
        if direction[held].abs() < 1e-9 {
            if origin[held].abs() > half[held] {
                return None;
            }
            continue;
        }
        let mut low = (-half[held] - origin[held]) / direction[held];
        let mut high = (half[held] - origin[held]) / direction[held];
        let mut held_sign = -1.0;
        if low > high {
            std::mem::swap(&mut low, &mut high);
            held_sign = 1.0;
        }
        if low > near {
            near = low;
            axis = held;
            sign = held_sign;
        }
        far = far.min(high);
        if near > far {
            return None;
        }
    }
    if far < 0.0 {
        return None;
    }
    let distance = near.max(0.0);
    let normal = if near < 0.0 {
        rotation.rotate_vec3(-local_direction)
    } else {
        let mut local = [0.0; 3];
        local[axis] = sign;
        rotation.rotate_vec3(Vec3::from_array(local))
    };
    Some((distance, normal))
}

fn capsule(
    center: Vec3,
    rotation: Quat,
    radius: f32,
    half_height: f32,
    origin: Vec3,
    direction: Vec3,
) -> Option<(f32, Vec3)> {
    let axis = rotation.rotate_vec3(Vec3::Y);
    let start = center - axis * half_height;
    let end = center + axis * half_height;
    let mut best: Option<(f32, Vec3)> = None;
    let offset = origin - start;
    let along = direction.dot(axis);
    let offset_along = offset.dot(axis);
    let perpendicular = direction - axis * along;
    let perpendicular_offset = offset - axis * offset_along;
    let quadratic = perpendicular.length_squared();
    let linear = 2.0 * perpendicular_offset.dot(perpendicular);
    let constant = perpendicular_offset.length_squared() - radius * radius;
    if quadratic > 1e-9 {
        let discriminant = linear * linear - 4.0 * quadratic * constant;
        if discriminant >= 0.0 {
            let root = discriminant.sqrt();
            for distance in [
                (-linear - root) / (2.0 * quadratic),
                (-linear + root) / (2.0 * quadratic),
            ] {
                if distance < 0.0 {
                    continue;
                }
                let height = offset_along + along * distance;
                if !(0.0..=half_height * 2.0).contains(&height) {
                    continue;
                }
                let point = origin + direction * distance;
                let offset = point - (start + axis * height);
                let normal = if offset.length_squared() > 1e-12 {
                    offset.normalize()
                } else {
                    -direction
                };
                consider(&mut best, distance, normal);
            }
        }
    }
    for (cap, limit) in [(start, -1.0), (end, 1.0)] {
        if let Some((distance, normal)) = sphere(cap, radius, origin, direction) {
            let point = origin + direction * distance;
            let height = (point - start).dot(axis);
            let beyond = if limit < 0.0 {
                height <= 0.0
            } else {
                height >= 2.0 * half_height
            };
            if beyond {
                consider(&mut best, distance, normal);
            }
        }
    }
    if best.is_none() {
        let closest = closest_on_segment(origin, start, end);
        if (origin - closest).length() <= radius {
            let normal = if (origin - closest).length_squared() > 0.0 {
                (closest - origin).normalize()
            } else {
                -direction
            };
            return Some((0.0, normal));
        }
    }
    best
}

fn consider(best: &mut Option<(f32, Vec3)>, distance: f32, normal: Vec3) {
    if best.is_none_or(|held| distance < held.0) {
        *best = Some((distance, normal));
    }
}

fn closest_on_segment(point: Vec3, start: Vec3, end: Vec3) -> Vec3 {
    let direction = end - start;
    let length = direction.length_squared();
    if length <= 1e-9 {
        return start;
    }
    start + direction * ((point - start).dot(direction) / length).clamp(0.0, 1.0)
}

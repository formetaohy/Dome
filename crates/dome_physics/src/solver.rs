use crate::body::Body;
use crate::cache::{CachedPoint, ContactCache};
use crate::contact::Manifold;
use dome_math::Vec3;

pub(crate) const VELOCITY_ITERATIONS: usize = 8;
pub(crate) const POSITION_ITERATIONS: usize = 3;
pub(crate) const LINEAR_SLOP: f32 = 0.005;
pub(crate) const MAX_CORRECTION: f32 = 0.2;
const RESTITUTION_VELOCITY: f32 = 1.0;

pub(crate) struct Contact {
    pub first: usize,
    pub second: usize,
    pub first_part: usize,
    pub second_part: usize,
    pub normal: Vec3,
    pub friction: f32,
    pub restitution: f32,
    pub points: Vec<ContactPoint>,
}

pub(crate) struct ContactPoint {
    feature: u64,
    local_first: Vec3,
    local_second: Vec3,
    offset_first: Vec3,
    offset_second: Vec3,
    normal_local_first: Vec3,
    separation: f32,
    tangent: [Vec3; 2],
    normal_mass: f32,
    tangent_mass: [f32; 2],
    normal_impulse: f32,
    tangent_impulse: [f32; 2],
    active: bool,
    target: f32,
    carried: f32,
    rebound: f32,
}

pub(crate) fn contact(
    first: usize,
    second: usize,
    first_part: usize,
    second_part: usize,
    manifold: Manifold,
    bodies: &[Body],
    cache: &mut ContactCache,
) -> Contact {
    let held = &bodies[first];
    let other = &bodies[second];
    let held_collider = &held.parts[first_part].collider;
    let other_collider = &other.parts[second_part].collider;
    let friction = (held_collider.friction() * other_collider.friction()).sqrt();
    let restitution = held_collider
        .restitution()
        .max(other_collider.restitution());
    let normal_local_first = held.rotation.inverse().rotate_vec3(manifold.normal);
    let tangent = tangent_basis(manifold.normal);
    let points = manifold
        .points
        .into_iter()
        .map(|point| {
            let local_first = held
                .rotation
                .inverse()
                .rotate_vec3(point.position - held.position);
            let local_second = other
                .rotation
                .inverse()
                .rotate_vec3(point.position - other.position);
            let cached = cache.take(
                held.entity,
                other.entity,
                first_part,
                second_part,
                point.feature,
            );
            let (normal_impulse, tangent_impulse, carried) = match cached {
                Some(held) => (held.normal, held.tangent, held.rebound),
                None => (0.0, [0.0; 2], 0.0),
            };

            ContactPoint {
                feature: point.feature,
                local_first,
                local_second,
                offset_first: Vec3::ZERO,
                offset_second: Vec3::ZERO,
                normal_local_first,
                separation: point.separation,
                tangent,
                normal_mass: 0.0,
                tangent_mass: [0.0; 2],
                normal_impulse,
                tangent_impulse,
                active: true,
                target: 0.0,
                carried,
                rebound: 0.0,
            }
        })
        .collect();
    Contact {
        first,
        second,
        first_part,
        second_part,
        normal: manifold.normal,
        friction,
        restitution,
        points,
    }
}

pub(crate) fn prepare(bodies: &[Body], contacts: &mut [Contact], delta: f32) {
    for contact in contacts.iter_mut() {
        let first = &bodies[contact.first];
        let second = &bodies[contact.second];
        let normal = contact.normal;
        for point in &mut contact.points {
            let offset_first = first.rotation.rotate_vec3(point.local_first);
            let offset_second = second.rotation.rotate_vec3(point.local_second);
            point.offset_first = offset_first;
            point.offset_second = offset_second;
            point.normal_mass = effective_mass(first, second, offset_first, offset_second, normal);
            for axis in 0..2 {
                point.tangent_mass[axis] = effective_mass(
                    first,
                    second,
                    offset_first,
                    offset_second,
                    point.tangent[axis],
                );
            }
            let relative =
                second.point_velocity(offset_second) - first.point_velocity(offset_first);
            let velocity = relative.dot(normal);
            let rebound = if velocity < -RESTITUTION_VELOCITY {
                -contact.restitution * velocity
            } else {
                0.0
            };
            let carried = point.carried;
            point.rebound = 0.0;
            if point.separation > LINEAR_SLOP {
                if -velocity * delta > point.separation {
                    point.active = true;
                    point.target = -point.separation / delta;
                    point.rebound = rebound;
                } else {
                    point.active = false;
                    point.target = 0.0;
                }
            } else {
                point.active = true;
                point.target = carried.max(rebound);
            }
        }
    }
}

pub(crate) fn warm_start(bodies: &mut [Body], contacts: &[Contact]) {
    for contact in contacts {
        if resting(bodies, contact) {
            continue;
        }
        for point in &contact.points {
            if !point.active || point.separation > 0.0 {
                continue;
            }
            let impulse = contact.normal * point.normal_impulse
                + point.tangent[0] * point.tangent_impulse[0]
                + point.tangent[1] * point.tangent_impulse[1];
            if impulse.length_squared() <= 0.0 {
                continue;
            }
            let (first, second) = pair_mut(bodies, contact.first, contact.second);
            first.apply_impulse(point.offset_first, -impulse);
            second.apply_impulse(point.offset_second, impulse);
        }
    }
}

pub(crate) fn solve_velocity(bodies: &mut [Body], contacts: &mut [Contact]) {
    for contact in contacts.iter_mut() {
        if resting(bodies, contact) {
            continue;
        }
        let normal = contact.normal;
        let friction = contact.friction;
        for point in &mut contact.points {
            if !point.active {
                continue;
            }
            let (first, second) = pair_mut(bodies, contact.first, contact.second);
            let offset_first = point.offset_first;
            let offset_second = point.offset_second;
            for axis in 0..2 {
                let tangent = point.tangent[axis];
                let relative =
                    second.point_velocity(offset_second) - first.point_velocity(offset_first);
                let velocity = relative.dot(tangent);
                let limit = friction * point.normal_impulse;
                let total = (point.tangent_impulse[axis] - velocity * point.tangent_mass[axis])
                    .clamp(-limit, limit);
                let applied = total - point.tangent_impulse[axis];
                point.tangent_impulse[axis] = total;
                first.apply_impulse(offset_first, tangent * -applied);
                second.apply_impulse(offset_second, tangent * applied);
            }
            let relative =
                second.point_velocity(offset_second) - first.point_velocity(offset_first);
            let velocity = relative.dot(normal);
            let total =
                (point.normal_impulse + (point.target - velocity) * point.normal_mass).max(0.0);
            let applied = total - point.normal_impulse;
            point.normal_impulse = total;
            first.apply_impulse(offset_first, normal * -applied);
            second.apply_impulse(offset_second, normal * applied);
        }
    }
}

pub(crate) fn solve_position(bodies: &mut [Body], contacts: &[Contact]) {
    for contact in contacts {
        if resting(bodies, contact) {
            continue;
        }
        for point in &contact.points {
            if !point.active {
                continue;
            }
            let (first, second) = pair_mut(bodies, contact.first, contact.second);
            let offset_first = first.rotation.rotate_vec3(point.local_first);
            let offset_second = second.rotation.rotate_vec3(point.local_second);
            let normal = first.rotation.rotate_vec3(point.normal_local_first);
            let separation = (second.position + offset_second - first.position - offset_first)
                .dot(normal)
                + point.separation
                + LINEAR_SLOP;
            if separation >= 0.0 {
                continue;
            }
            let mass = effective_mass(first, second, offset_first, offset_second, normal);
            if mass <= 0.0 {
                continue;
            }
            let mut impulse = -separation * mass;
            let reach = impulse * (first.inverse_mass + second.inverse_mass);
            if reach > MAX_CORRECTION {
                impulse *= MAX_CORRECTION / reach;
            }
            first.apply_position_impulse(offset_first, normal * -impulse);
            second.apply_position_impulse(offset_second, normal * impulse);
        }
    }
}

pub(crate) fn cache(contact: &Contact) -> impl Iterator<Item = (u64, CachedPoint)> + '_ {
    contact.points.iter().filter_map(|point| {
        let held = CachedPoint {
            normal: point.normal_impulse,
            tangent: point.tangent_impulse,
            rebound: point.rebound,
        };
        (held.normal > 0.0
            || held.tangent[0] != 0.0
            || held.tangent[1] != 0.0
            || held.rebound > 0.0)
            .then_some((point.feature, held))
    })
}

pub(crate) fn effective_mass(
    first: &Body,
    second: &Body,
    offset_first: Vec3,
    offset_second: Vec3,
    axis: Vec3,
) -> f32 {
    let mut mass = first.inverse_mass + second.inverse_mass;
    if first.movable() {
        mass += axis.dot(
            first
                .inverse_inertia
                .mul_vec3(offset_first.cross(axis))
                .cross(offset_first),
        );
    }
    if second.movable() {
        mass += axis.dot(
            second
                .inverse_inertia
                .mul_vec3(offset_second.cross(axis))
                .cross(offset_second),
        );
    }
    if mass > 0.0 { 1.0 / mass } else { 0.0 }
}

fn tangent_basis(normal: Vec3) -> [Vec3; 2] {
    let reference = if normal.x.abs() < 0.577 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let first = normal.cross(reference).normalize();
    [first, normal.cross(first)]
}

fn resting(bodies: &[Body], contact: &Contact) -> bool {
    !bodies[contact.first].active() && !bodies[contact.second].active()
}

pub(crate) fn pair_mut<T>(items: &mut [T], first: usize, second: usize) -> (&mut T, &mut T) {
    assert_ne!(first, second, "a contact joins two bodies");
    if first < second {
        let (low, high) = items.split_at_mut(second);
        (&mut low[first], &mut high[0])
    } else {
        let (low, high) = items.split_at_mut(first);
        (&mut high[0], &mut low[second])
    }
}

use crate::{Collider, Joint, Shape};
use dome_ecs::{Entity, World};
use dome_math::{Transform, Vec3, Vec4};
use std::f32::consts::{PI, TAU};

const SEGMENTS: usize = 24;
const CAPSULE_SIDES: usize = 4;
const JOINT_AXIS: f32 = 0.25;

const SOLID: Vec4 = Vec4::new(0.24, 0.92, 0.46, 0.9);
const SENSOR: Vec4 = Vec4::new(0.98, 0.76, 0.22, 0.9);
const JOINT: Vec4 = Vec4::new(0.38, 0.66, 1.0, 0.95);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugLine {
    pub start: Vec3,
    pub end: Vec3,
    pub color: Vec4,
}

pub fn debug_lines(world: &World) -> Vec<DebugLine> {
    let mut lines = Vec::new();
    for (entity, collider) in world.iter::<Collider>() {
        let Some(transform) = world.get::<Transform>(entity) else {
            continue;
        };
        collider_lines(collider, *transform, &mut lines);
    }
    for (entity, joint) in world.iter::<Joint>() {
        joint_lines(world, entity, joint, &mut lines);
    }
    lines
}

fn collider_lines(collider: &Collider, transform: Transform, lines: &mut Vec<DebugLine>) {
    let color = if collider.is_sensor() { SENSOR } else { SOLID };
    let center = collider.center();
    match collider.shape() {
        Shape::Sphere { radius } => sphere(center, radius, transform, color, lines),
        Shape::Cuboid { half_extents } => cuboid(center, half_extents, transform, color, lines),
        Shape::Capsule {
            radius,
            half_height,
        } => capsule(center, radius, half_height, transform, color, lines),
        Shape::Mesh { mesh, scale } => {
            for edge in mesh.edges() {
                push(
                    lines,
                    transform,
                    color,
                    center + edge[0] * scale,
                    center + edge[1] * scale,
                );
            }
        }
    }
}

fn joint_lines(world: &World, entity: Entity, joint: &Joint, lines: &mut Vec<DebugLine>) {
    let Some(first) = world.get::<Transform>(entity) else {
        return;
    };
    let Some(second) = world.get::<Transform>(joint.other()) else {
        return;
    };
    let anchor = first.transform_point(joint.anchor());
    let other_anchor = second.transform_point(joint.other_anchor());
    let axis = first.rotation.rotate_vec3(joint.axis());
    let other_axis = second.rotation.rotate_vec3(joint.other_axis());
    lines.push(DebugLine {
        start: anchor,
        end: other_anchor,
        color: JOINT,
    });
    lines.push(DebugLine {
        start: anchor,
        end: anchor + axis * JOINT_AXIS,
        color: JOINT,
    });
    lines.push(DebugLine {
        start: other_anchor,
        end: other_anchor + other_axis * JOINT_AXIS,
        color: JOINT,
    });
}

fn push(lines: &mut Vec<DebugLine>, transform: Transform, color: Vec4, start: Vec3, end: Vec3) {
    lines.push(DebugLine {
        start: transform.transform_point(start),
        end: transform.transform_point(end),
        color,
    });
}

fn sphere(
    center: Vec3,
    radius: f32,
    transform: Transform,
    color: Vec4,
    lines: &mut Vec<DebugLine>,
) {
    for plane in 0..3 {
        let mut previous = center + direction(plane, 0) * radius;
        for step in 1..=SEGMENTS {
            let next = center + direction(plane, step) * radius;
            push(lines, transform, color, previous, next);
            previous = next;
        }
    }
}

fn direction(plane: usize, step: usize) -> Vec3 {
    let angle = TAU * step as f32 / SEGMENTS as f32;
    let (sin, cos) = angle.sin_cos();
    match plane {
        0 => Vec3::new(cos, sin, 0.0),
        1 => Vec3::new(cos, 0.0, sin),
        _ => Vec3::new(0.0, cos, sin),
    }
}

fn cuboid(
    center: Vec3,
    half_extents: Vec3,
    transform: Transform,
    color: Vec4,
    lines: &mut Vec<DebugLine>,
) {
    const EDGES: [(usize, usize); 12] = [
        (0, 1),
        (1, 3),
        (3, 2),
        (2, 0),
        (4, 5),
        (5, 7),
        (7, 6),
        (6, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    let corner = |x: f32, y: f32, z: f32| {
        center + Vec3::new(x * half_extents.x, y * half_extents.y, z * half_extents.z)
    };
    let corners = [
        corner(-1.0, -1.0, -1.0),
        corner(1.0, -1.0, -1.0),
        corner(-1.0, 1.0, -1.0),
        corner(1.0, 1.0, -1.0),
        corner(-1.0, -1.0, 1.0),
        corner(1.0, -1.0, 1.0),
        corner(-1.0, 1.0, 1.0),
        corner(1.0, 1.0, 1.0),
    ];
    for (first, second) in EDGES {
        push(lines, transform, color, corners[first], corners[second]);
    }
}

fn capsule(
    center: Vec3,
    radius: f32,
    half_height: f32,
    transform: Transform,
    color: Vec4,
    lines: &mut Vec<DebugLine>,
) {
    let top = center + Vec3::Y * half_height;
    let bottom = center - Vec3::Y * half_height;
    ring(top, radius, transform, color, lines);
    ring(bottom, radius, transform, color, lines);
    for side in 0..CAPSULE_SIDES {
        let angle = TAU * side as f32 / CAPSULE_SIDES as f32;
        let (sin, cos) = angle.sin_cos();
        let offset = Vec3::new(cos * radius, 0.0, sin * radius);
        push(lines, transform, color, bottom + offset, top + offset);
    }
    for plane in 0..2 {
        let mut previous_top = cap(center, half_height, radius, plane, 0, true);
        let mut previous_bottom = cap(center, half_height, radius, plane, 0, false);
        for step in 1..=SEGMENTS / 2 {
            let next_top = cap(center, half_height, radius, plane, step, true);
            let next_bottom = cap(center, half_height, radius, plane, step, false);
            push(lines, transform, color, previous_top, next_top);
            push(lines, transform, color, previous_bottom, next_bottom);
            previous_top = next_top;
            previous_bottom = next_bottom;
        }
    }
}

fn cap(center: Vec3, half_height: f32, radius: f32, plane: usize, step: usize, top: bool) -> Vec3 {
    let angle = PI * step as f32 / (SEGMENTS / 2) as f32;
    let (sin, cos) = angle.sin_cos();
    let bend = if top {
        half_height + sin * radius
    } else {
        -half_height - sin * radius
    };
    match plane {
        0 => center + Vec3::new(cos * radius, bend, 0.0),
        _ => center + Vec3::new(0.0, bend, cos * radius),
    }
}

fn ring(center: Vec3, radius: f32, transform: Transform, color: Vec4, lines: &mut Vec<DebugLine>) {
    let mut previous = center + Vec3::new(radius, 0.0, 0.0);
    for step in 1..=SEGMENTS {
        let angle = TAU * step as f32 / SEGMENTS as f32;
        let (sin, cos) = angle.sin_cos();
        let next = center + Vec3::new(cos * radius, 0.0, sin * radius);
        push(lines, transform, color, previous, next);
        previous = next;
    }
}

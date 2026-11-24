use crate::Shape;
use crate::contact::{Manifold, RawPoint};
use crate::segment::closest_segment_segment;
use crate::solid::Solid;
use dome_math::Vec3;

const AXES: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];
const EDGE_BIAS: f32 = 1.05;
const FACE_SECOND: u32 = 1 << 9;
const EDGE: u32 = 1 << 10;
const SUPPORT: u32 = 1 << 8;

#[derive(Clone, Copy)]
enum Kind {
    FaceFirst(usize),
    FaceSecond(usize),
    Edge(usize, usize),
}

#[derive(Clone, Copy)]
struct Axis {
    normal: Vec3,
    overlap: f32,
    kind: Kind,
    score: f32,
}

#[derive(Clone, Copy)]
struct Face<'a> {
    solid: &'a Solid,
    half_extents: Vec3,
    axis: usize,
    normal: Vec3,
}

pub(crate) fn box_box(first: &Solid, second: &Solid, gap: f32) -> Option<Manifold> {
    let best = best_axis(first, second)?;
    if -best.overlap > gap {
        return None;
    }
    let first_he = half_extents(&first.shape);
    let second_he = half_extents(&second.shape);
    let normal = first.rotation.rotate_vec3(best.normal);
    let points = match best.kind {
        Kind::FaceFirst(axis) => face_points(
            Face {
                solid: first,
                half_extents: first_he,
                axis,
                normal: best.normal,
            },
            second,
            second_he,
            gap,
            0,
        ),
        Kind::FaceSecond(axis) => face_points(
            Face {
                solid: second,
                half_extents: second_he,
                axis,
                normal: second.rotation.inverse().rotate_vec3(-normal),
            },
            first,
            first_he,
            gap,
            FACE_SECOND,
        ),
        Kind::Edge(first_axis, second_axis) => {
            let first_edge = support_edge(first, first_he, first_axis, normal);
            let second_edge = support_edge(second, second_he, second_axis, -normal);
            let (held, other) = closest_segment_segment(first_edge, second_edge);
            let feature = u64::from(EDGE + (first_axis * 3 + second_axis) as u32);
            vec![((held + other) * 0.5, -best.overlap, feature)]
        }
    };
    Manifold::new(normal, points, gap)
}

pub(crate) fn overlap(first: &Solid, second: &Solid) -> bool {
    best_axis(first, second).is_some_and(|axis| axis.overlap >= 0.0)
}

fn best_axis(first: &Solid, second: &Solid) -> Option<Axis> {
    let first_he = half_extents(&first.shape);
    let second_he = half_extents(&second.shape);
    let relative = first.rotation.inverse() * second.rotation;
    let second_axes = [
        relative.rotate_vec3(Vec3::X),
        relative.rotate_vec3(Vec3::Y),
        relative.rotate_vec3(Vec3::Z),
    ];
    let translation = first
        .rotation
        .inverse()
        .rotate_vec3(second.position - first.position);
    let mut axes = [Axis {
        normal: Vec3::ZERO,
        overlap: 0.0,
        kind: Kind::FaceFirst(0),
        score: 0.0,
    }; 15];
    let mut count = 0;
    for (axis, &direction) in AXES.iter().enumerate() {
        let distance = translation.dot(direction);
        let overlap = first_he.to_array()[axis] + projected(second_he, second_axes, direction)
            - distance.abs();
        axes[count] = Axis {
            normal: direction * sign(distance),
            overlap,
            kind: Kind::FaceFirst(axis),
            score: overlap,
        };
        count += 1;
    }
    for (axis, &direction) in second_axes.iter().enumerate() {
        let distance = translation.dot(direction);
        let overlap =
            projected(first_he, AXES, direction) + second_he.to_array()[axis] - distance.abs();
        axes[count] = Axis {
            normal: direction * sign(distance),
            overlap,
            kind: Kind::FaceSecond(axis),
            score: overlap,
        };
        count += 1;
    }
    for (first_axis, &first_direction) in AXES.iter().enumerate() {
        for (second_axis, &second_direction) in second_axes.iter().enumerate() {
            let direction = first_direction.cross(second_direction);
            let length_squared = direction.length_squared();
            if length_squared < 1e-8 {
                continue;
            }
            let direction = direction / length_squared.sqrt();
            if axes[..count]
                .iter()
                .any(|axis| axis.normal.cross(direction).length_squared() < 1e-8)
            {
                continue;
            }
            let distance = translation.dot(direction);
            let overlap = projected(first_he, AXES, direction)
                + projected(second_he, second_axes, direction)
                - distance.abs();
            axes[count] = Axis {
                normal: direction * sign(distance),
                overlap,
                kind: Kind::Edge(first_axis, second_axis),
                score: overlap * EDGE_BIAS,
            };
            count += 1;
        }
    }
    let separated = axes[..count].iter().any(|axis| axis.overlap < 0.0);
    let mut best: Option<Axis> = None;
    for axis in &axes[..count] {
        if (axis.overlap < 0.0) != separated {
            continue;
        }
        let replace = best.is_none_or(|current| {
            if separated {
                axis.overlap > current.overlap
            } else {
                axis.score < current.score
            }
        });
        if replace {
            best = Some(*axis);
        }
    }
    best
}

fn sign(value: f32) -> f32 {
    if value < 0.0 { -1.0 } else { 1.0 }
}

fn projected(half_extents: Vec3, axes: [Vec3; 3], direction: Vec3) -> f32 {
    half_extents.x * direction.dot(axes[0]).abs()
        + half_extents.y * direction.dot(axes[1]).abs()
        + half_extents.z * direction.dot(axes[2]).abs()
}

fn half_extents(shape: &Shape) -> Vec3 {
    match shape {
        Shape::Cuboid { half_extents } => *half_extents,
        _ => panic!("a cuboid collider holds a cuboid"),
    }
}

fn face_points(
    reference: Face<'_>,
    incident: &Solid,
    incident_half_extents: Vec3,
    gap: f32,
    base: u32,
) -> Vec<RawPoint> {
    let incident_normal = -reference.solid.rotation.rotate_vec3(reference.normal);
    let mut incident_axis = 0;
    let mut incident_dot: f32 = 0.0;
    for (axis, &held) in AXES.iter().enumerate() {
        let dot = incident.rotation.rotate_vec3(held).dot(incident_normal);
        if dot.abs() > incident_dot.abs() {
            incident_dot = dot;
            incident_axis = axis;
        }
    }
    let incident_sign = if incident_dot >= 0.0 { 1.0 } else { -1.0 };
    let (first_side, second_side) = match incident_axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let extent = incident_half_extents.to_array();
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let mut polygon = corners
        .iter()
        .enumerate()
        .map(|(at, (first_sign, second_sign))| {
            let mut local = [0.0; 3];
            local[incident_axis] = incident_sign * extent[incident_axis];
            local[first_side] = first_sign * extent[first_side];
            local[second_side] = second_sign * extent[second_side];
            let world = incident.position + incident.rotation.rotate_vec3(Vec3::from_array(local));
            let held = reference
                .solid
                .rotation
                .inverse()
                .rotate_vec3(world - reference.solid.position);
            (held, 1u32 << at)
        })
        .collect::<Vec<_>>();
    let mut plane = 0;
    for axis in 0..3 {
        if axis == reference.axis {
            continue;
        }
        let bound = reference.half_extents.to_array()[axis];
        polygon = clip(polygon, axis, -bound, true, 1 << (4 + plane));
        plane += 1;
        polygon = clip(polygon, axis, bound, false, 1 << (4 + plane));
        plane += 1;
        if polygon.is_empty() {
            break;
        }
    }
    let mut points = polygon
        .into_iter()
        .map(|(point, feature)| {
            let separation =
                reference.normal.dot(point) - reference.half_extents.to_array()[reference.axis];
            (
                reference.solid.position + reference.solid.rotation.rotate_vec3(point),
                separation,
                u64::from(base | feature),
            )
        })
        .filter(|(_, separation, _)| *separation <= gap)
        .collect::<Vec<_>>();
    if points.is_empty() {
        let support = support_vertex(incident, incident_half_extents, incident_normal);
        let local = reference
            .solid
            .rotation
            .inverse()
            .rotate_vec3(support - reference.solid.position);
        let separation =
            reference.normal.dot(local) - reference.half_extents.to_array()[reference.axis];
        if separation <= gap {
            points.push((support, separation, u64::from(base | SUPPORT)));
        }
    }
    points
}

fn clip(
    polygon: Vec<(Vec3, u32)>,
    axis: usize,
    bound: f32,
    greater: bool,
    plane: u32,
) -> Vec<(Vec3, u32)> {
    let mut clipped = Vec::with_capacity(polygon.len() + 1);
    for index in 0..polygon.len() {
        let (current, current_feature) = polygon[index];
        let (next, next_feature) = polygon[(index + 1) % polygon.len()];
        let current_value = current.to_array()[axis] - bound;
        let next_value = next.to_array()[axis] - bound;
        let current_inside = if greater {
            current_value >= 0.0
        } else {
            current_value <= 0.0
        };
        let next_inside = if greater {
            next_value >= 0.0
        } else {
            next_value <= 0.0
        };
        if current_inside {
            clipped.push((current, current_feature));
        }
        if current_inside != next_inside {
            let phase = current_value / (current_value - next_value);
            clipped.push((
                current.lerp(next, phase),
                current_feature | next_feature | plane,
            ));
        }
    }
    clipped
}

fn support_vertex(incident: &Solid, half_extents: Vec3, direction: Vec3) -> Vec3 {
    let mut local = Vec3::ZERO;
    for (axis, &held) in AXES.iter().enumerate() {
        let sign = if incident.rotation.rotate_vec3(held).dot(direction) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        local += held * (half_extents.to_array()[axis] * sign);
    }
    incident.position + incident.rotation.rotate_vec3(local)
}

fn support_edge(solid: &Solid, half_extents: Vec3, axis: usize, direction: Vec3) -> (Vec3, Vec3) {
    let mut center = solid.position;
    for (other, &held) in AXES.iter().enumerate() {
        if other == axis {
            continue;
        }
        let world = solid.rotation.rotate_vec3(held);
        let sign = if world.dot(direction) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        center += world * (half_extents.to_array()[other] * sign);
    }
    let direction = solid.rotation.rotate_vec3(AXES[axis]);
    let extent = half_extents.to_array()[axis];
    (center - direction * extent, center + direction * extent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Shape;
    use dome_math::Quat;

    fn solid(position: Vec3, rotation: Quat) -> Solid {
        Solid::new(position, rotation, Shape::cuboid(Vec3::splat(1.0)))
    }

    #[test]
    fn a_box_pair_reports_one_contact_point_however_its_boxes_are_ordered() {
        let gap = 0.01;
        let first = solid(Vec3::ZERO, Quat::from_rotation_z(0.7));
        let mut worst = 0.0f32;
        for pitch in [0.3f32, 0.6, 0.9, 1.2] {
            for yaw in [0.2f32, 0.5, 0.8] {
                let rotation = Quat::from_rotation_x(pitch) * Quat::from_rotation_y(yaw);
                for height in [1.8f32, 2.0, 2.2, 2.4, 2.6] {
                    for offset in [0.0f32, 0.3, 0.6] {
                        let second = solid(Vec3::new(offset, height, 0.0), rotation);
                        let Some(forward) = box_box(&first, &second, gap) else {
                            continue;
                        };
                        let Some(reverse) = box_box(&second, &first, gap) else {
                            continue;
                        };
                        let drift =
                            (forward.points[0].position - reverse.points[0].position).length();
                        worst = worst.max(drift);
                    }
                }
            }
        }
        assert!(
            worst < 0.05,
            "a box pair reports contact points {worst} apart when its boxes swap places",
        );
    }
}

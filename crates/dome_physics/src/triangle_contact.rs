use crate::contact::{Manifold, RawPoint};
use crate::segment::closest_segment_segment;
use dome_math::Vec3;

const AXES: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];
const DISTANCE_EPSILON: f32 = 1e-6;
const PARALLEL_AXIS: f32 = 1e-10;
const EDGE_DISTANCE: f32 = 1e-8;
const EDGE_BIAS: f32 = 1.05;
const SUPPORT: u32 = 1 << 8;

pub(crate) struct Contact {
    pub manifold: Manifold,
    pub edges: u8,
}

pub(crate) fn sphere(triangle: [Vec3; 3], center: Vec3, radius: f32, gap: f32) -> Option<Contact> {
    let closest = closest_point(center, triangle);
    let delta = center - closest;
    let distance = delta.length();
    let normal = if distance > DISTANCE_EPSILON {
        delta / distance
    } else {
        face(triangle)
    };
    let separation = distance - radius;
    let point = closest + normal * (separation * 0.5);
    Some(Contact {
        manifold: Manifold::new(normal, vec![(point, separation, 0)], gap)?,
        edges: edge_mask(closest, triangle),
    })
}

pub(crate) fn capsule(
    triangle: [Vec3; 3],
    start: Vec3,
    end: Vec3,
    radius: f32,
    gap: f32,
) -> Option<Contact> {
    let (segment, held) = closest_segment_triangle(start, end, triangle);
    let delta = segment - held;
    let distance = delta.length();
    let normal = if distance > DISTANCE_EPSILON {
        delta / distance
    } else {
        face(triangle)
    };
    let separation = distance - radius;
    if separation > gap {
        return None;
    }
    let axis = end - start;
    let mut points = vec![(held + normal * (separation * 0.5), separation, 0u64)];
    if axis.length_squared() > DISTANCE_EPSILON {
        for (endpoint, id) in [(start, 1u64), (end, 2u64)] {
            let closest = closest_point(endpoint, triangle);
            let held = (endpoint - closest).length() - radius;
            if held <= gap {
                points.push((endpoint - normal * (radius + held * 0.5), held, id));
            }
        }
    }
    if points.len() > 2 {
        let direction = axis.normalize();
        points.sort_by(|left, right| {
            (left.0 - start)
                .dot(direction)
                .total_cmp(&(right.0 - start).dot(direction))
        });
        points = vec![points[0], *points.last().expect("a manifold holds a point")];
    }
    Some(Contact {
        manifold: Manifold::new(normal, points, gap)?,
        edges: edge_mask(held, triangle),
    })
}

fn edge_mask(point: Vec3, triangle: [Vec3; 3]) -> u8 {
    let mut mask = 0;
    for edge in 0..3 {
        let start = triangle[edge];
        let direction = triangle[(edge + 1) % 3] - start;
        let length = direction.length_squared();
        if length <= DISTANCE_EPSILON {
            continue;
        }
        let phase = ((point - start).dot(direction) / length).clamp(0.0, 1.0);
        if (start + direction * phase - point).length_squared() <= EDGE_DISTANCE {
            mask |= 1 << edge;
        }
    }
    mask
}

pub(crate) fn cuboid(triangle: [Vec3; 3], half_extents: Vec3, gap: f32) -> Option<Contact> {
    let normal = face(triangle);
    let center = (triangle[0] + triangle[1] + triangle[2]) / 3.0;
    let mut axes = [Axis {
        normal: Vec3::ZERO,
        overlap: 0.0,
        kind: Kind::Box(0),
        score: 0.0,
    }; 13];
    let mut count = 0;
    for (axis, &direction) in AXES.iter().enumerate() {
        axes[count] = sat_axis(direction, half_extents, triangle, center, Kind::Box(axis));
        count += 1;
    }
    axes[count] = sat_axis(normal, half_extents, triangle, center, Kind::Triangle);
    count += 1;
    let edges = [(0usize, 1usize), (1, 2), (2, 0)];
    for (edge, (start, end)) in edges.iter().enumerate() {
        let direction = triangle[*end] - triangle[*start];
        for (side, &box_axis) in AXES.iter().enumerate() {
            let crossing = direction.cross(box_axis);
            if crossing.length_squared() <= PARALLEL_AXIS {
                continue;
            }
            let crossing = crossing.normalize();
            if axes[..count]
                .iter()
                .any(|axis| axis.normal.cross(crossing).length_squared() < 1e-8)
            {
                continue;
            }
            axes[count] = sat_axis(
                crossing,
                half_extents,
                triangle,
                center,
                Kind::Edge(edge, side),
            );
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
    let best = best?;
    if -best.overlap > gap {
        return None;
    }
    let edges = match best.kind {
        Kind::Edge(edge, _) => 1 << edge,
        Kind::Box(_) | Kind::Triangle => 0,
    };
    let points = match best.kind {
        Kind::Box(axis) => box_face(&best, half_extents, triangle, axis, gap),
        Kind::Triangle => triangle_face(&best, half_extents, triangle, gap),
        Kind::Edge(edge, axis) => edge_contact(&best, half_extents, triangle, edge, axis),
    };
    Some(Contact {
        manifold: Manifold::new(best.normal, points, gap)?,
        edges,
    })
}

fn closest_segment_triangle(start: Vec3, end: Vec3, triangle: [Vec3; 3]) -> (Vec3, Vec3) {
    let mut best = (start, closest_point(start, triangle));
    let mut closest = (best.0 - best.1).length_squared();
    let other = (end, closest_point(end, triangle));
    let distance = (other.0 - other.1).length_squared();
    if distance < closest {
        best = other;
        closest = distance;
    }
    for edge in 0..3 {
        let (held, other) =
            closest_segment_segment((triangle[edge], triangle[(edge + 1) % 3]), (start, end));
        let distance = (held - other).length_squared();
        if distance < closest {
            best = (other, held);
            closest = distance;
        }
    }
    best
}

pub(crate) fn closest_point(point: Vec3, triangle: [Vec3; 3]) -> Vec3 {
    let [first, second, third] = triangle;
    let first_edge = second - first;
    let second_edge = third - first;
    let offset = point - first;
    let d1 = first_edge.dot(offset);
    let d2 = second_edge.dot(offset);
    if d1 <= 0.0 && d2 <= 0.0 {
        return first;
    }
    let offset = point - second;
    let d3 = first_edge.dot(offset);
    let d4 = second_edge.dot(offset);
    if d3 >= 0.0 && d4 <= d3 {
        return second;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return first + first_edge * (d1 / (d1 - d3));
    }
    let offset = point - third;
    let d5 = first_edge.dot(offset);
    let d6 = second_edge.dot(offset);
    if d6 >= 0.0 && d5 <= d6 {
        return third;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return first + second_edge * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return second + (third - second) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let total = 1.0 / (va + vb + vc);
    first + first_edge * (vb * total) + second_edge * (vc * total)
}

fn face(triangle: [Vec3; 3]) -> Vec3 {
    (triangle[1] - triangle[0])
        .cross(triangle[2] - triangle[0])
        .normalize()
}

#[derive(Clone, Copy)]
struct Axis {
    normal: Vec3,
    overlap: f32,
    kind: Kind,
    score: f32,
}

#[derive(Clone, Copy)]
enum Kind {
    Box(usize),
    Triangle,
    Edge(usize, usize),
}

fn sat_axis(
    direction: Vec3,
    half_extents: Vec3,
    triangle: [Vec3; 3],
    center: Vec3,
    kind: Kind,
) -> Axis {
    let box_radius = half_extents.x * direction.x.abs()
        + half_extents.y * direction.y.abs()
        + half_extents.z * direction.z.abs();
    let first = triangle[0].dot(direction);
    let second = triangle[1].dot(direction);
    let third = triangle[2].dot(direction);
    let low = first.min(second).min(third);
    let high = first.max(second).max(third);
    let radius = (high - low) * 0.5;
    let offset = center.dot(direction);
    let overlap = box_radius + radius - offset.abs();
    let normal = if offset > 0.0 {
        direction * -1.0
    } else {
        direction
    };
    let score = if matches!(kind, Kind::Edge(..)) {
        overlap * EDGE_BIAS
    } else {
        overlap
    };
    Axis {
        normal,
        overlap,
        kind,
        score,
    }
}

fn box_face(
    best: &Axis,
    half_extents: Vec3,
    triangle: [Vec3; 3],
    axis: usize,
    gap: f32,
) -> Vec<RawPoint> {
    let face_normal = -best.normal;
    let extents = half_extents.to_array();
    let mut polygon = triangle
        .iter()
        .enumerate()
        .map(|(at, &vertex)| (vertex, 1u32 << at))
        .collect::<Vec<_>>();
    let mut plane = 0;
    for other in 0..3 {
        if other == axis {
            continue;
        }
        let bound = extents[other];
        polygon = clip(polygon, AXES[other], bound, 1 << (4 + plane));
        plane += 1;
        polygon = clip(polygon, -AXES[other], bound, 1 << (4 + plane));
        plane += 1;
        if polygon.is_empty() {
            break;
        }
    }
    let mut points = polygon
        .into_iter()
        .map(|(point, feature)| {
            (
                point,
                point.dot(face_normal) - extents[axis],
                u64::from(feature),
            )
        })
        .filter(|(_, separation, _)| *separation <= gap)
        .collect::<Vec<_>>();
    if points.is_empty() {
        let deepest = triangle.into_iter().min_by(|left, right| {
            (left.dot(face_normal) - extents[axis])
                .total_cmp(&(right.dot(face_normal) - extents[axis]))
        });
        if let Some(point) = deepest {
            let separation = point.dot(face_normal) - extents[axis];
            if separation <= gap {
                points.push((point, separation, u64::from(SUPPORT)));
            }
        }
    }
    points
}

fn triangle_face(best: &Axis, half_extents: Vec3, triangle: [Vec3; 3], gap: f32) -> Vec<RawPoint> {
    let normal = best.normal;
    let mut axis = 0;
    let mut largest: f32 = 0.0;
    for (index, &held) in AXES.iter().enumerate() {
        let dot = normal.dot(held).abs();
        if dot > largest {
            largest = dot;
            axis = index;
        }
    }
    let sign = if normal.to_array()[axis] >= 0.0 {
        -1.0
    } else {
        1.0
    };
    let (first, second) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let extents = half_extents.to_array();
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let mut polygon = corners
        .iter()
        .enumerate()
        .map(|(at, (first_side, second_side))| {
            let mut local = [0.0; 3];
            local[axis] = sign * extents[axis];
            local[first] = first_side * extents[first];
            local[second] = second_side * extents[second];
            (Vec3::from_array(local), 1u32 << at)
        })
        .collect::<Vec<_>>();
    let mut plane = 0;
    for edge in 0..3 {
        let start = triangle[edge];
        let end = triangle[(edge + 1) % 3];
        let other = triangle[(edge + 2) % 3];
        let mut side = (end - start).cross(normal);
        if side.length_squared() <= PARALLEL_AXIS {
            continue;
        }
        if side.dot(other - start) > 0.0 {
            side = -side;
        }
        polygon = clip(polygon, side, side.dot(start), 1 << (4 + plane));
        plane += 1;
        if polygon.is_empty() {
            break;
        }
    }
    let mut points = polygon
        .into_iter()
        .map(|(point, feature)| (point, (point - triangle[0]).dot(normal), u64::from(feature)))
        .filter(|(_, separation, _)| *separation <= gap)
        .collect::<Vec<_>>();
    if points.is_empty() {
        let support = support(half_extents, -normal);
        let separation = (support - triangle[0]).dot(normal);
        if separation <= gap {
            points.push((support, separation, u64::from(SUPPORT)));
        }
    }
    points
}

fn edge_contact(
    best: &Axis,
    half_extents: Vec3,
    triangle: [Vec3; 3],
    edge: usize,
    axis: usize,
) -> Vec<RawPoint> {
    let (start, end) = (triangle[edge], triangle[(edge + 1) % 3]);
    let normal = best.normal;
    let extents = half_extents.to_array();
    let mut corner = [0.0; 3];
    for other in 0..3 {
        if other == axis {
            continue;
        }
        corner[other] = if normal.to_array()[other] <= 0.0 {
            extents[other]
        } else {
            -extents[other]
        };
    }
    let reach = AXES[axis] * extents[axis];
    let (held, other) = closest_segment_segment(
        (start, end),
        (
            Vec3::from_array(corner) - reach,
            Vec3::from_array(corner) + reach,
        ),
    );
    vec![((held + other) * 0.5, -best.overlap, 0)]
}

fn support(half_extents: Vec3, direction: Vec3) -> Vec3 {
    let mut local = [0.0; 3];
    for (axis, &held) in AXES.iter().enumerate() {
        local[axis] = if held.dot(direction) >= 0.0 {
            half_extents.to_array()[axis]
        } else {
            -half_extents.to_array()[axis]
        };
    }
    Vec3::from_array(local)
}

fn clip(polygon: Vec<(Vec3, u32)>, normal: Vec3, offset: f32, plane: u32) -> Vec<(Vec3, u32)> {
    let mut clipped = Vec::with_capacity(polygon.len() + 1);
    for index in 0..polygon.len() {
        let (current, current_feature) = polygon[index];
        let (next, next_feature) = polygon[(index + 1) % polygon.len()];
        let current_value = current.dot(normal) - offset;
        let next_value = next.dot(normal) - offset;
        if current_value <= 0.0 {
            clipped.push((current, current_feature));
        }
        if (current_value <= 0.0) != (next_value <= 0.0) {
            let phase = current_value / (current_value - next_value);
            clipped.push((
                current.lerp(next, phase),
                current_feature | next_feature | plane,
            ));
        }
    }
    clipped
}

use dome_math::Vec3;

const EPSILON: f32 = 1e-9;

pub(crate) fn closest_segment_segment(first: (Vec3, Vec3), second: (Vec3, Vec3)) -> (Vec3, Vec3) {
    let (p1, q1) = first;
    let (p2, q2) = second;
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let mut s;
    let mut t;
    if a <= EPSILON && e <= EPSILON {
        return (p1, p2);
    }
    if a <= EPSILON {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= EPSILON {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denominator = a * e - b * b;
            s = if denominator.abs() > EPSILON {
                ((b * f - c * e) / denominator).clamp(0.0, 1.0)
            } else {
                0.0
            };
            t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
        }
    }
    (p1 + d1 * s, p2 + d2 * t)
}

pub(crate) fn closest_segment_box(
    origin: Vec3,
    direction: Vec3,
    half_extents: Vec3,
) -> (Vec3, Vec3) {
    let o = origin.to_array();
    let d = direction.to_array();
    let h = half_extents.to_array();
    let at = |t: f32| [o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t];
    let clamp = |point: [f32; 3]| {
        [
            point[0].clamp(-h[0], h[0]),
            point[1].clamp(-h[1], h[1]),
            point[2].clamp(-h[2], h[2]),
        ]
    };
    let squared = |point: [f32; 3]| {
        let held = clamp(point);
        (point[0] - held[0]).powi(2) + (point[1] - held[1]).powi(2) + (point[2] - held[2]).powi(2)
    };
    let mut limits = vec![0.0f32, 1.0];
    for axis in 0..3 {
        if d[axis].abs() > EPSILON {
            for bound in [-h[axis], h[axis]] {
                let t = (bound - o[axis]) / d[axis];
                if t > 0.0 && t < 1.0 {
                    limits.push(t);
                }
            }
        }
    }
    limits.sort_by(f32::total_cmp);
    limits.dedup_by(|left, right| (*left - *right).abs() <= EPSILON);
    let mut best = (0.0f32, squared(at(0.0)));
    if best.1 <= EPSILON {
        let point = Vec3::from_array(at(0.0));
        return (point, point);
    }
    for window in limits.windows(2) {
        let (low, high) = (window[0], window[1]);
        if high - low <= EPSILON {
            continue;
        }
        let middle = (low + high) * 0.5;
        let point = at(middle);
        let held = clamp(point);
        if point == held {
            let point = Vec3::from_array(point);
            return (point, point);
        }
        let mut quadratic = 0.0;
        let mut linear = 0.0;
        for axis in 0..3 {
            if point[axis] != held[axis] {
                quadratic += d[axis] * d[axis];
                linear += 2.0 * d[axis] * (o[axis] - held[axis]);
            }
        }
        let mut candidates = vec![low, high, middle];
        if quadratic > EPSILON {
            candidates.push((-linear / (2.0 * quadratic)).clamp(low, high));
        }
        for t in candidates {
            let distance = squared(at(t));
            if distance < best.1 {
                best = (t, distance);
            }
        }
    }
    let point = at(best.0);
    (Vec3::from_array(point), Vec3::from_array(clamp(point)))
}

pub(crate) fn closest_point_segment(point: Vec3, segment: (Vec3, Vec3)) -> Vec3 {
    let direction = segment.1 - segment.0;
    let length = direction.length_squared();
    if length <= EPSILON {
        return segment.0;
    }
    let t = ((point - segment.0).dot(direction) / length).clamp(0.0, 1.0);
    segment.0 + direction * t
}

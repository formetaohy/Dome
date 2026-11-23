use dome_math::{Transform, Vec3};

const MIN_DISTANCE: f32 = 1.0;
const MAX_DISTANCE: f32 = 50.0;
const NEAR_CUTOFF: f32 = 18_000.0;
const FAR_CUTOFF: f32 = 1_200.0;

pub(crate) struct Listener {
    position: Vec3,
    right: Vec3,
}

impl Listener {
    pub(crate) fn of(transform: &Transform) -> Self {
        Self {
            position: transform.translation,
            right: transform.rotation.rotate_vec3(Vec3::X),
        }
    }
}

pub(crate) fn place(
    listener: Option<&Listener>,
    position: Option<Vec3>,
    gain: f32,
    min_distance: f32,
    max_distance: f32,
    spatial: bool,
) -> (f32, f32, f32) {
    let (Some(listener), Some(position)) = (listener, position) else {
        return (gain, 0.0, f32::INFINITY);
    };
    if !spatial {
        return (gain, 0.0, f32::INFINITY);
    }
    let distance = listener.position.distance(position);
    let min_distance = if min_distance > 0.0 {
        min_distance
    } else {
        MIN_DISTANCE
    };
    let max_distance = if max_distance > min_distance {
        max_distance
    } else {
        min_distance + MAX_DISTANCE
    };
    let attenuation = if distance <= min_distance {
        1.0
    } else if distance >= max_distance {
        0.0
    } else {
        let falloff = (distance - min_distance) / (max_distance - min_distance);
        (min_distance / distance) * (1.0 - falloff * falloff)
    };
    let direction = position - listener.position;
    let pan = if direction.length_squared() > f32::EPSILON {
        direction.normalize().dot(listener.right).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let cutoff = if distance <= min_distance {
        f32::INFINITY
    } else {
        let falloff = ((distance - min_distance) / (max_distance - min_distance))
            .clamp(0.0, 1.0)
            .sqrt();
        NEAR_CUTOFF + (FAR_CUTOFF - NEAR_CUTOFF) * falloff
    };
    (gain * attenuation, pan, cutoff)
}

use dome_math::Vec3;

pub(crate) fn hammersley(index: u32, count: u32) -> [f32; 2] {
    [index as f32 / count as f32, radical(index)]
}

pub(crate) fn importance(sample: [f32; 2], roughness: f32, normal: Vec3) -> Vec3 {
    let alpha = roughness * roughness;
    let phi = std::f32::consts::TAU * sample[0];
    let cosine = ((1.0 - sample[1]) / (1.0 + (alpha * alpha - 1.0) * sample[1]))
        .max(0.0)
        .sqrt();
    let sine = (1.0 - cosine * cosine).max(0.0).sqrt();
    let (tangent, bitangent) = tangent_frame(normal);
    tangent * (sine * phi.cos()) + bitangent * (sine * phi.sin()) + normal * cosine
}

fn tangent_frame(normal: Vec3) -> (Vec3, Vec3) {
    let sign = if normal.z >= 0.0 { 1.0 } else { -1.0 };
    let a = -1.0 / (sign + normal.z);
    let b = normal.x * normal.y * a;
    (
        Vec3::new(
            1.0 + sign * normal.x * normal.x * a,
            sign * b,
            -sign * normal.x,
        ),
        Vec3::new(b, sign + normal.y * normal.y * a, -normal.y),
    )
}

fn radical(mut bits: u32) -> f32 {
    bits = bits.reverse_bits();
    bits as f32 * 2.328_306_4e-10
}

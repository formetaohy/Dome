use crate::environment::sample::{hammersley, importance};
use dome_math::Vec3;

pub fn brdf(size: u32) -> Vec<[f32; 4]> {
    assert!(size > 0, "a brdf table spans no texel");
    let mut texels = Vec::with_capacity(size as usize * size as usize);
    for y in 0..size {
        let roughness = (y as f32 + 0.5) / size as f32;
        for x in 0..size {
            let cosine = (x as f32 + 0.5) / size as f32;
            let (scale, bias) = integrate(cosine, roughness);
            texels.push([scale, bias, 0.0, 1.0]);
        }
    }
    texels
}

fn integrate(cosine: f32, roughness: f32) -> (f32, f32) {
    let view = Vec3::new((1.0 - cosine * cosine).max(0.0).sqrt(), 0.0, cosine);
    let mut scale = 0.0;
    let mut bias = 0.0;
    for index in 0..crate::environment::BRDF_SAMPLES {
        let half = importance(
            hammersley(index, crate::environment::BRDF_SAMPLES),
            roughness,
            Vec3::Z,
        );
        let light = half * (2.0 * view.dot(half)) - view;
        let n_dot_l = light.z.max(0.0);
        let n_dot_h = half.z.max(0.0);
        let v_dot_h = view.dot(half).max(0.0);
        if n_dot_l <= 0.0 || n_dot_h <= 0.0 || cosine <= 0.0 {
            continue;
        }
        let k = (roughness + 1.0) * (roughness + 1.0) * 0.125;
        let geometry = visibility(cosine, k) * visibility(n_dot_l, k);
        let weight = geometry * v_dot_h / (n_dot_h * cosine);
        let fresnel = (1.0 - v_dot_h).powi(5);
        scale += (1.0 - fresnel) * weight;
        bias += fresnel * weight;
    }
    (
        scale / crate::environment::BRDF_SAMPLES as f32,
        bias / crate::environment::BRDF_SAMPLES as f32,
    )
}

fn visibility(cosine: f32, k: f32) -> f32 {
    cosine / (cosine * (1.0 - k) + k)
}

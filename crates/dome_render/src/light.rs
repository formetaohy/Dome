use dome_math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalLight {
    pub color: Vec3,
    pub intensity: f32,
    pub shadow_distance: f32,
}

impl DirectionalLight {
    pub fn new(color: Vec3, intensity: f32, shadow_distance: f32) -> Self {
        assert!(
            color.x >= 0.0 && color.y >= 0.0 && color.z >= 0.0,
            "a directional light of color {color:?} emits a negative channel",
        );
        assert!(
            intensity > 0.0 && intensity.is_finite(),
            "a directional light of intensity {intensity} emits no radiance",
        );
        assert!(
            shadow_distance > 0.0 && shadow_distance.is_finite(),
            "a directional light of shadow distance {shadow_distance} shadows no depth",
        );
        Self {
            color,
            intensity,
            shadow_distance,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
}

impl PointLight {
    pub fn new(color: Vec3, intensity: f32, range: f32) -> Self {
        assert!(
            color.x >= 0.0 && color.y >= 0.0 && color.z >= 0.0,
            "a point light of color {color:?} emits a negative channel",
        );
        assert!(
            intensity > 0.0 && intensity.is_finite(),
            "a point light of intensity {intensity} emits no radiance",
        );
        assert!(
            range > 0.0 && range.is_finite(),
            "a point light of range {range} reaches no surface",
        );
        Self {
            color,
            intensity,
            range,
        }
    }
}

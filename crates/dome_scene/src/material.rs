use crate::reference::ImageId;
use dome_math::Vec3;
use dome_render::AlphaMode;

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialAsset {
    pub base_color: Vec3,
    pub alpha: f32,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub base_color_image: Option<ImageId>,
    pub metallic: f32,
    pub roughness: f32,
    pub metallic_roughness_image: Option<ImageId>,
    pub normal_image: Option<ImageId>,
    pub normal_scale: f32,
    pub occlusion_image: Option<ImageId>,
    pub occlusion_strength: f32,
    pub emissive: Vec3,
    pub emissive_image: Option<ImageId>,
}

impl MaterialAsset {
    pub fn grey() -> Self {
        Self {
            base_color: Vec3::new(0.72, 0.72, 0.75),
            alpha: 1.0,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            base_color_image: None,
            metallic: 0.0,
            roughness: 0.6,
            metallic_roughness_image: None,
            normal_image: None,
            normal_scale: 1.0,
            occlusion_image: None,
            occlusion_strength: 1.0,
            emissive: Vec3::ZERO,
            emissive_image: None,
        }
    }

    pub fn references_image(&self, id: &str) -> bool {
        [
            &self.base_color_image,
            &self.metallic_roughness_image,
            &self.normal_image,
            &self.occlusion_image,
            &self.emissive_image,
        ]
        .into_iter()
        .flatten()
        .any(|held| held.as_str() == id)
    }

    pub fn images(&self) -> [&Option<ImageId>; 5] {
        [
            &self.base_color_image,
            &self.metallic_roughness_image,
            &self.normal_image,
            &self.occlusion_image,
            &self.emissive_image,
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialEntry {
    pub id: String,
    pub asset: MaterialAsset,
}

impl MaterialEntry {
    pub fn new(id: impl Into<String>, asset: MaterialAsset) -> Self {
        Self {
            id: id.into(),
            asset,
        }
    }
}

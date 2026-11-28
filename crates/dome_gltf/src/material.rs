use crate::document::{Document, NormalTextureInfo, OcclusionTextureInfo, TextureInfo};
use dome_math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

pub struct Material {
    pub base_color: Vec3,
    pub alpha: f32,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub base_color_image: Option<usize>,
    pub metallic: f32,
    pub roughness: f32,
    pub metallic_roughness_image: Option<usize>,
    pub normal_image: Option<usize>,
    pub normal_scale: f32,
    pub occlusion_image: Option<usize>,
    pub occlusion_strength: f32,
    pub emissive: Vec3,
    pub emissive_image: Option<usize>,
}

impl Material {
    pub fn uses_image(&self) -> bool {
        self.base_color_image.is_some()
            || self.metallic_roughness_image.is_some()
            || self.normal_image.is_some()
            || self.occlusion_image.is_some()
            || self.emissive_image.is_some()
    }
}

pub(crate) fn build(document: &Document, what: &str) -> Result<Vec<Material>, String> {
    document
        .materials
        .iter()
        .enumerate()
        .map(|(index, material)| {
            let held = format!("material {index} of {what}");
            let alpha_mode = match material.alpha_mode.as_deref() {
                None | Some("OPAQUE") => AlphaMode::Opaque,
                Some("MASK") => AlphaMode::Mask,
                Some("BLEND") => AlphaMode::Blend,
                Some(mode) => return Err(format!("{held} holds the alpha mode {mode}")),
            };
            let pbr = material.pbr_metallic_roughness.as_ref();
            let base_color = pbr
                .and_then(|pbr| pbr.base_color_factor)
                .unwrap_or([1.0; 4]);
            if !base_color.iter().all(|value| (0.0..=1.0).contains(value)) {
                return Err(format!("{held} holds the base color {base_color:?}"));
            }
            let alpha_cutoff = material.alpha_cutoff.unwrap_or(0.5);
            if !alpha_cutoff.is_finite() {
                return Err(format!("{held} holds the alpha cutoff {alpha_cutoff}"));
            }
            let base_color_image = pbr
                .and_then(|pbr| pbr.base_color_texture.as_ref())
                .map(|info| image_of(info, document, &held))
                .transpose()?;
            let metallic_roughness_image = pbr
                .and_then(|pbr| pbr.metallic_roughness_texture.as_ref())
                .map(|info| image_of(info, document, &held))
                .transpose()?;
            let metallic = pbr.and_then(|pbr| pbr.metallic_factor).unwrap_or(1.0);
            let roughness = pbr.and_then(|pbr| pbr.roughness_factor).unwrap_or(1.0);
            if !(0.0..=1.0).contains(&metallic) {
                return Err(format!("{held} holds metallic {metallic}"));
            }
            if !(0.0..=1.0).contains(&roughness) {
                return Err(format!("{held} holds roughness {roughness}"));
            }
            let normal_image = material
                .normal_texture
                .as_ref()
                .map(|info| normal_image_of(info, document, &held))
                .transpose()?;
            let normal_scale = material
                .normal_texture
                .as_ref()
                .and_then(|info| info.scale)
                .unwrap_or(1.0);
            if normal_scale < 0.0 || !normal_scale.is_finite() {
                return Err(format!("{held} holds normal scale {normal_scale}"));
            }
            let occlusion_image = material
                .occlusion_texture
                .as_ref()
                .map(|info| occlusion_image_of(info, document, &held))
                .transpose()?;
            let occlusion_strength = material
                .occlusion_texture
                .as_ref()
                .and_then(|info| info.strength)
                .unwrap_or(1.0);
            if !(0.0..=1.0).contains(&occlusion_strength) {
                return Err(format!(
                    "{held} holds occlusion strength {occlusion_strength}"
                ));
            }
            let emissive = material.emissive_factor.unwrap_or([0.0; 3]);
            if !emissive.iter().all(|value| (0.0..=1.0).contains(value)) {
                return Err(format!("{held} emits the light {emissive:?}"));
            }
            let emissive_image = material
                .emissive_texture
                .as_ref()
                .map(|info| image_of(info, document, &held))
                .transpose()?;
            Ok(Material {
                base_color: Vec3::new(base_color[0], base_color[1], base_color[2]),
                alpha: base_color[3],
                alpha_mode,
                alpha_cutoff: alpha_cutoff.clamp(0.0, 1.0),
                base_color_image,
                metallic,
                roughness: roughness.max(0.0001),
                metallic_roughness_image,
                normal_image,
                normal_scale,
                occlusion_image,
                occlusion_strength,
                emissive: Vec3::new(emissive[0], emissive[1], emissive[2]),
                emissive_image,
            })
        })
        .collect()
}

fn image_of(info: &TextureInfo, document: &Document, what: &str) -> Result<usize, String> {
    texture(
        info.tex_coord,
        info.extensions.is_none(),
        info.index,
        document,
        what,
    )
}

fn normal_image_of(
    info: &NormalTextureInfo,
    document: &Document,
    what: &str,
) -> Result<usize, String> {
    texture(
        info.tex_coord,
        info.extensions.is_none(),
        info.index,
        document,
        what,
    )
}

fn occlusion_image_of(
    info: &OcclusionTextureInfo,
    document: &Document,
    what: &str,
) -> Result<usize, String> {
    texture(
        info.tex_coord,
        info.extensions.is_none(),
        info.index,
        document,
        what,
    )
}

fn texture(
    coordinate: Option<u32>,
    untransformed: bool,
    index: usize,
    document: &Document,
    what: &str,
) -> Result<usize, String> {
    if coordinate.unwrap_or(0) != 0 {
        return Err(format!(
            "{what} samples the texture coordinate set {}",
            coordinate.unwrap_or(0)
        ));
    }
    if !untransformed {
        return Err(format!("{what} holds a texture transform"));
    }
    let texture = document
        .textures
        .get(index)
        .ok_or_else(|| format!("{what} samples no texture {index}"))?;
    let image = texture
        .source
        .ok_or_else(|| format!("{what} samples the texture {index} of no image"))?;
    if image >= document.images.len() {
        return Err(format!(
            "{what} samples the image {image}, which the model does not hold"
        ));
    }
    Ok(image)
}

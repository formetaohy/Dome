use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Document {
    pub(crate) asset: Asset,
    pub(crate) scene: Option<usize>,
    #[serde(default)]
    pub(crate) scenes: Vec<Scene>,
    #[serde(default)]
    pub(crate) nodes: Vec<Node>,
    #[serde(default)]
    pub(crate) meshes: Vec<Mesh>,
    #[serde(default)]
    pub(crate) materials: Vec<Material>,
    #[serde(default)]
    pub(crate) textures: Vec<Texture>,
    #[serde(default)]
    pub(crate) images: Vec<Image>,
    #[serde(default)]
    pub(crate) samplers: Vec<Sampler>,
    #[serde(default)]
    pub(crate) skins: Vec<Skin>,
    #[serde(default)]
    pub(crate) animations: Vec<Animation>,
    #[serde(default)]
    pub(crate) buffers: Vec<Buffer>,
    #[serde(default)]
    pub(crate) buffer_views: Vec<BufferView>,
    #[serde(default)]
    pub(crate) accessors: Vec<Accessor>,
    pub(crate) extensions_required: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub(crate) struct Asset {
    pub(crate) version: String,
}

#[derive(Deserialize)]
pub(crate) struct Scene {
    pub(crate) nodes: Option<Vec<usize>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Node {
    pub(crate) name: Option<String>,
    pub(crate) children: Option<Vec<usize>>,
    pub(crate) mesh: Option<usize>,
    pub(crate) skin: Option<usize>,
    pub(crate) matrix: Option<[f32; 16]>,
    pub(crate) translation: Option<[f32; 3]>,
    pub(crate) rotation: Option<[f32; 4]>,
    pub(crate) scale: Option<[f32; 3]>,
}

#[derive(Deserialize)]
pub(crate) struct Mesh {
    pub(crate) primitives: Vec<Primitive>,
}

#[derive(Deserialize)]
pub(crate) struct Primitive {
    pub(crate) attributes: BTreeMap<String, usize>,
    pub(crate) indices: Option<usize>,
    pub(crate) material: Option<usize>,
    pub(crate) mode: Option<u32>,
    pub(crate) targets: Option<Vec<BTreeMap<String, usize>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Material {
    pub(crate) pbr_metallic_roughness: Option<Pbr>,
    pub(crate) normal_texture: Option<NormalTextureInfo>,
    pub(crate) occlusion_texture: Option<OcclusionTextureInfo>,
    pub(crate) emissive_texture: Option<TextureInfo>,
    pub(crate) emissive_factor: Option<[f32; 3]>,
    pub(crate) alpha_mode: Option<String>,
    pub(crate) alpha_cutoff: Option<f32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Pbr {
    pub(crate) base_color_factor: Option<[f32; 4]>,
    pub(crate) base_color_texture: Option<TextureInfo>,
    pub(crate) metallic_factor: Option<f32>,
    pub(crate) roughness_factor: Option<f32>,
    pub(crate) metallic_roughness_texture: Option<TextureInfo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextureInfo {
    pub(crate) index: usize,
    pub(crate) tex_coord: Option<u32>,
    pub(crate) extensions: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NormalTextureInfo {
    pub(crate) index: usize,
    pub(crate) tex_coord: Option<u32>,
    pub(crate) extensions: Option<serde_json::Value>,
    pub(crate) scale: Option<f32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OcclusionTextureInfo {
    pub(crate) index: usize,
    pub(crate) tex_coord: Option<u32>,
    pub(crate) extensions: Option<serde_json::Value>,
    pub(crate) strength: Option<f32>,
}

#[derive(Deserialize)]
pub(crate) struct Texture {
    pub(crate) source: Option<usize>,
    pub(crate) sampler: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Sampler {
    pub(crate) mag_filter: Option<u32>,
    pub(crate) min_filter: Option<u32>,
    pub(crate) wrap_s: Option<u32>,
    pub(crate) wrap_t: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Image {
    pub(crate) uri: Option<String>,
    pub(crate) buffer_view: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Skin {
    pub(crate) inverse_bind_matrices: Option<usize>,
    pub(crate) skeleton: Option<usize>,
    pub(crate) joints: Vec<usize>,
}

#[derive(Deserialize)]
pub(crate) struct Animation {
    pub(crate) name: Option<String>,
    pub(crate) channels: Vec<AnimationChannel>,
    pub(crate) samplers: Vec<AnimationSampler>,
}

#[derive(Deserialize)]
pub(crate) struct AnimationChannel {
    pub(crate) sampler: usize,
    pub(crate) target: AnimationTarget,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnimationTarget {
    pub(crate) node: Option<usize>,
    pub(crate) path: String,
}

#[derive(Deserialize)]
pub(crate) struct AnimationSampler {
    pub(crate) input: usize,
    pub(crate) output: usize,
    pub(crate) interpolation: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Buffer {
    pub(crate) uri: Option<String>,
    pub(crate) byte_length: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BufferView {
    pub(crate) buffer: usize,
    pub(crate) byte_offset: Option<usize>,
    pub(crate) byte_length: usize,
    pub(crate) byte_stride: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Accessor {
    pub(crate) buffer_view: Option<usize>,
    pub(crate) byte_offset: Option<usize>,
    pub(crate) component_type: u32,
    pub(crate) normalized: Option<bool>,
    pub(crate) count: usize,
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) sparse: Option<serde_json::Value>,
}

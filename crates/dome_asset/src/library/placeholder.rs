use super::prepare::{ImageBytes, MeshData};
use super::work::EnvironmentData;
use dome_animation::{Clip, Interpolation, Property, Track};
use dome_audio::{Channels, Sound, Source};
use dome_math::Mat4;
use dome_render::SkinVertexRecord;
use dome_scene::{AnimationEntry, MaterialAsset, MaterialEntry, Scene, Shape, SkinData, SkinEntry};
use std::sync::Arc;

const IMAGE_TEXEL: [u8; 4] = [255, 0, 255, 255];
const SAMPLE_RATE: u32 = 48_000;
const SILENT_FRAME: f32 = 0.0;

pub(super) fn image() -> ImageBytes {
    ImageBytes {
        width: 1,
        height: 1,
        rgba: IMAGE_TEXEL.to_vec(),
    }
}

pub(super) fn material(entry: &MaterialEntry) -> MaterialEntry {
    MaterialEntry::new(entry.id.as_str(), MaterialAsset::grey())
}

pub(super) fn mesh() -> MeshData {
    let vertices = Shape::Cube.vertices();
    let weights = vec![SkinVertexRecord::new([0, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]); vertices.len()];
    let positions = vertices
        .iter()
        .map(|vertex| vertex.position)
        .collect::<Vec<_>>()
        .into();
    MeshData {
        vertices,
        skin: Some(weights),
        positions,
    }
}

pub(super) fn skin(entry: &SkinEntry) -> SkinData {
    let joints = entry.asset.joints().to_vec();
    let inverse_binds = vec![Mat4::IDENTITY; joints.len()];
    SkinData::new(joints, inverse_binds.into())
}

pub(super) fn animation(scene: &Scene, entry: &AnimationEntry) -> Arc<Clip> {
    let tracks = entry
        .asset
        .targets()
        .iter()
        .map(|target| {
            let index = scene
                .node_index(target.as_str())
                .unwrap_or_else(|| panic!("the scene holds no node {target} for its animation"));
            let translation = scene.transform(index).translation;
            Track::new(
                target.as_str(),
                Property::Translation,
                Interpolation::Linear,
                vec![0.0],
                vec![translation.x, translation.y, translation.z],
            )
        })
        .collect::<Vec<_>>();
    Arc::new(Clip::new(tracks))
}

pub(super) fn sound() -> Source {
    Source::decoded(Arc::new(Sound::new(
        SAMPLE_RATE,
        Channels::Mono,
        vec![SILENT_FRAME],
    )))
}

pub(super) fn environment() -> EnvironmentData {
    EnvironmentData::Empty
}

mod animation;
mod change;
mod component;
mod deletion;
mod delta;
mod document;
mod edit;
mod environment;
pub mod field;
mod graph;
mod graph_edit;
mod image;
mod instance;
mod material;
mod mesh;
mod model;
mod node;
mod nodes;
mod parse;
mod prefab;
mod property;
mod reference;
mod scene;
mod scene_value;
mod shape;
mod skin;
mod sound;
mod target;

pub mod transform;

mod value;

pub use animation::{AnimationAsset, AnimationEntry};
pub use change::Changed;
pub use component::{
    Component, ComponentSpec, ENVIRONMENT_ATMOSPHERE_FIELDS, ENVIRONMENT_PANORAMA_FIELDS, Field,
    FieldKind, IMAGE_FIELDS, MATERIAL_FIELDS, MESH_FIELDS, MODEL_FIELDS, Reach, ReferenceKind,
    SOUND_FIELDS, Schema,
};
pub use delta::Delta;
pub use dome_render::environment::Atmosphere;
pub use edit::Edit;
pub use environment::{EnvironmentEntry, EnvironmentSource};
pub use field::Axis;
pub use graph::{
    GraphAsset, GraphCondition, GraphEntry, GraphParameter, GraphState, GraphTransition,
};
pub use image::{ImageEntry, ImageSource};
pub use material::{MaterialAsset, MaterialEntry};
pub use mesh::{Geometry, MeshAsset, MeshEntry};
pub use model::ModelEntry;
pub use node::{InstanceOverride, Node};
pub use nodes::Nodes;
pub use reference::{
    AnimationId, AudioBusId, EnvironmentId, GraphId, ImageId, MaterialId, MeshId, ModelId, NodeId,
    SkinId, SoundId,
};
pub use scene::Scene;
pub use scene_value::SceneValue;
pub use shape::Shape;
pub use skin::{SkinAsset, SkinData, SkinEntry};
pub use sound::SoundEntry;
pub use target::{Target, valid_id};
pub use transform::TRANSFORM_FIELDS;
pub use value::{Effect, Value};

#[doc(hidden)]
pub mod __private {
    pub use crate::{
        Axis, ComponentSpec, Effect, Field, FieldKind, Reach, Scene, SceneValue, Value,
    };
    pub use dome_toml::{Fields, Toml, floats};
}

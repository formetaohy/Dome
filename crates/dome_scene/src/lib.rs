mod animation;
mod animation_library;
mod body;
mod change;
mod collider;
mod deletion;
mod delta;
mod digest;
mod document;
mod edit;
mod environment;
mod field;
mod graph;
mod graph_edit;
mod graph_library;
mod image;
mod import;
mod install;
mod instance;
mod joint;
mod library;
mod material;
mod mesh;
mod model;
mod node;
mod nodes;
mod parse;
mod prefab;
mod property;
mod reference;
mod registry;
mod runtime;
mod scene;
mod scene_value;
mod schema;
mod shape;
mod skin;
mod sound;
mod source;
mod sync;
mod target;
mod value;
mod watch;

pub use animation::{AnimationAsset, AnimationEntry};
pub use animation_library::AnimationLibrary;
pub use body::BodyRef;
pub use change::Changed;
pub use collider::{ColliderRef, ShapeKind};
pub use delta::Delta;
pub use dome_render::environment::Atmosphere;
pub use dome_scene_macro::SceneComponent;
pub use edit::Edit;
pub use environment::{EnvironmentEntry, EnvironmentRef, EnvironmentSource};
pub use field::Axis;
pub use graph::{
    GraphAsset, GraphCondition, GraphEntry, GraphParameter, GraphState, GraphTransition,
};
pub use graph_library::GraphLibrary;
pub use image::{ImageEntry, ImageSource};
pub use joint::JointRef;
pub use library::{AssetError, ImageTexture, Library, Refresh};
pub use material::{MaterialAsset, MaterialEntry};
pub use mesh::{MeshAsset, MeshEntry};
pub use model::ModelEntry;
pub use node::{AnimationRef, InstanceOverride, MeshRef, Node, SceneNode, SkinRef};
pub use nodes::Nodes;
pub use reference::{
    AnimationId, AudioBusId, EnvironmentId, GraphId, ImageId, MaterialId, MeshId, ModelId, NodeId,
    SkinId,
};
pub use registry::{
    Assets, AudioBusRef, AudioCompressorRef, AudioFilterRef, AudioListenerRef, AudioReverbRef,
    AudioSourceRef, Registry, SceneComponent,
};
pub use runtime::{observe_runtime, read_runtime_property, set_runtime_property};
pub use scene::Scene;
pub use scene_value::SceneValue;
pub use schema::{
    ComponentSpec, ENVIRONMENT_ATMOSPHERE_FIELDS, ENVIRONMENT_PANORAMA_FIELDS, Field, FieldKind,
    IMAGE_FIELDS, MATERIAL_FIELDS, MESH_FIELDS, MODEL_FIELDS, NODE_FIELDS, Reach, ReferenceKind,
    SOUND_FIELDS,
};
pub use shape::Shape;
pub use skin::{SkinAsset, SkinData, SkinEntry};
pub use sound::SoundEntry;
pub use sync::SceneSync;
pub use target::{Target, valid_id};
pub use value::{Effect, Value};
pub use watch::{Change, Watch};

#[doc(hidden)]
pub mod __private {
    pub use crate::{
        Assets, Axis, ComponentSpec, Effect, Field, FieldKind, Reach, Scene, SceneComponent,
        SceneValue, Value,
    };
    pub use dome_ecs::{Entity, World};
    pub use dome_toml::{Fields, Toml, floats};
}

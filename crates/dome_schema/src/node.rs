use dome_animation::Parameter;
use dome_math::Vec3;
use dome_scene::{GraphId, MaterialId, MeshId, SkinId};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct MeshRef {
    pub mesh: MeshId,
    pub material: MaterialId,
    pub tint: Vec3,
}

impl MeshRef {
    pub fn new(mesh: impl Into<MeshId>, material: impl Into<MaterialId>, tint: Vec3) -> Self {
        Self {
            mesh: mesh.into(),
            material: material.into(),
            tint,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkinRef {
    pub skin: SkinId,
}

impl SkinRef {
    pub fn new(skin: impl Into<SkinId>) -> Self {
        Self { skin: skin.into() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnimationRef {
    pub graph: GraphId,
    pub playing: bool,
    pub fade: f32,
    state: String,
    time: f32,
    parameters: BTreeMap<String, Parameter>,
}

impl AnimationRef {
    pub const DEFAULT_FADE: f32 = 0.25;

    pub fn new(graph: impl Into<GraphId>, playing: bool, fade: f32) -> Self {
        assert!(
            fade.is_finite() && fade >= 0.0,
            "an animation fade of {fade} is no finite non-negative span",
        );
        Self {
            graph: graph.into(),
            playing,
            fade,
            state: String::new(),
            time: 0.0,
            parameters: BTreeMap::new(),
        }
    }

    pub(crate) fn with_runtime(
        mut self,
        state: impl Into<String>,
        time: f32,
        parameters: BTreeMap<String, Parameter>,
    ) -> Self {
        self.state = state.into();
        self.time = time;
        self.parameters = parameters;
        self
    }

    pub(crate) fn state(&self) -> &str {
        &self.state
    }

    pub(crate) fn time(&self) -> f32 {
        self.time
    }

    pub(crate) fn parameters(&self) -> &BTreeMap<String, Parameter> {
        &self.parameters
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneNode {
    pub id: String,
}

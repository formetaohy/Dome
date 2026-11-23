use crate::reference::{GraphId, MaterialId, MeshId, SkinId};
use crate::registry::{ErasedComponent, SceneComponent};
use crate::value::Value;
use dome_animation::Parameter;
use dome_math::{Transform, Vec3};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct InstanceOverride {
    pub node: String,
    pub path: String,
    pub value: Value,
}

pub struct Node {
    pub id: String,
    pub parent: Option<String>,
    pub(crate) prefab: Option<std::path::PathBuf>,
    pub(crate) owner: Option<String>,
    pub(crate) overrides: Vec<InstanceOverride>,
    pub(crate) additions: Vec<Node>,
    pub(crate) removals: Vec<String>,
    pub(crate) components: BTreeMap<&'static str, Box<dyn ErasedComponent>>,
}

impl Clone for Node {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            parent: self.parent.clone(),
            prefab: self.prefab.clone(),
            owner: self.owner.clone(),
            overrides: self.overrides.clone(),
            additions: self.additions.clone(),
            removals: self.removals.clone(),
            components: self
                .components
                .iter()
                .map(|(name, component)| (*name, component.clone_box()))
                .collect(),
        }
    }
}

impl Node {
    pub fn new(id: impl Into<String>) -> Self {
        let mut node = Self {
            id: id.into(),
            parent: None,
            prefab: None,
            owner: None,
            overrides: Vec::new(),
            additions: Vec::new(),
            removals: Vec::new(),
            components: BTreeMap::new(),
        };
        node.insert_component(Transform::IDENTITY);
        node
    }

    pub fn instance(id: impl Into<String>, prefab: impl Into<std::path::PathBuf>) -> Self {
        let mut node = Self::new(id);
        node.prefab = Some(prefab.into());
        node
    }

    pub fn prefab(&self) -> Option<&std::path::Path> {
        self.prefab.as_deref()
    }

    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    pub fn overrides(&self) -> &[InstanceOverride] {
        &self.overrides
    }

    pub fn additions(&self) -> &[Node] {
        &self.additions
    }

    pub fn removals(&self) -> &[String] {
        &self.removals
    }

    pub fn with<T: SceneComponent>(mut self, component: T) -> Self {
        self.insert_component(component);
        self
    }

    pub fn insert_component<T: SceneComponent>(&mut self, component: T) -> bool {
        self.insert_component_box(Box::new(component)).is_none()
    }

    pub fn remove_component(&mut self, name: &str) -> bool {
        self.components.remove(name).is_some()
    }

    pub fn has_component(&self, name: &str) -> bool {
        self.components.contains_key(name)
    }

    pub fn component<T: SceneComponent>(&self) -> Option<&T> {
        self.components
            .get(T::SPEC.name)?
            .as_any()
            .downcast_ref::<T>()
    }

    pub fn component_mut<T: SceneComponent>(&mut self) -> Option<&mut T> {
        self.components
            .get_mut(T::SPEC.name)?
            .as_any_mut()
            .downcast_mut::<T>()
    }

    pub(crate) fn components(
        &self,
    ) -> impl Iterator<Item = (&'static str, &dyn ErasedComponent)> + '_ {
        self.components
            .iter()
            .map(|(name, component)| (*name, component.as_ref()))
    }

    pub(crate) fn component_by_name(&self, name: &str) -> Option<&dyn ErasedComponent> {
        self.components
            .get(name)
            .map(|component| component.as_ref())
    }

    pub(crate) fn insert_component_box(
        &mut self,
        component: Box<dyn ErasedComponent>,
    ) -> Option<Box<dyn ErasedComponent>> {
        self.components.insert(component.name(), component)
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.parent == other.parent
            && self.prefab == other.prefab
            && self.owner == other.owner
            && self.overrides == other.overrides
            && self.additions == other.additions
            && self.removals == other.removals
            && self.components.len() == other.components.len()
            && self.components.iter().all(|(name, component)| {
                other
                    .components
                    .get(name)
                    .is_some_and(|other| component.equals(other.as_ref()))
            })
    }
}

impl fmt::Debug for Node {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Node")
            .field("id", &self.id)
            .field("parent", &self.parent)
            .field("prefab", &self.prefab)
            .field("owner", &self.owner)
            .field("overrides", &self.overrides)
            .field("additions", &self.additions)
            .field("removals", &self.removals)
            .field(
                "components",
                &self.components.keys().copied().collect::<Vec<_>>(),
            )
            .finish()
    }
}

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
pub struct SceneNode {
    pub id: String,
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

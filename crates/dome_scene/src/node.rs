use crate::component::Component;
use crate::value::Value;
use dome_math::Transform;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct InstanceOverride {
    pub node: String,
    pub path: String,
    pub value: Value,
}

pub struct Node {
    pub id: String,
    pub parent: Option<String>,
    pub(crate) transform: Transform,
    pub(crate) prefab: Option<PathBuf>,
    pub(crate) owner: Option<String>,
    pub(crate) overrides: Vec<InstanceOverride>,
    pub(crate) additions: Vec<Node>,
    pub(crate) removals: Vec<String>,
    pub(crate) components: BTreeMap<&'static str, Box<dyn Component>>,
}

impl Clone for Node {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            parent: self.parent.clone(),
            transform: self.transform,
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
        Self {
            id: id.into(),
            parent: None,
            transform: Transform::IDENTITY,
            prefab: None,
            owner: None,
            overrides: Vec::new(),
            additions: Vec::new(),
            removals: Vec::new(),
            components: BTreeMap::new(),
        }
    }

    pub fn instance(id: impl Into<String>, prefab: impl Into<PathBuf>) -> Self {
        let mut node = Self::new(id);
        node.prefab = Some(prefab.into());
        node
    }

    pub fn transform(&self) -> Transform {
        self.transform
    }

    pub fn set_transform(&mut self, transform: Transform) {
        self.transform = transform;
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

    pub fn insert_component(
        &mut self,
        component: Box<dyn Component>,
    ) -> Option<Box<dyn Component>> {
        self.components.insert(component.name(), component)
    }

    pub fn remove_component(&mut self, name: &str) -> bool {
        self.components.remove(name).is_some()
    }

    pub fn has_component(&self, name: &str) -> bool {
        self.components.contains_key(name)
    }

    pub fn components(&self) -> impl Iterator<Item = (&'static str, &dyn Component)> + '_ {
        self.components
            .iter()
            .map(|(name, component)| (*name, component.as_ref()))
    }

    pub fn components_mut(
        &mut self,
    ) -> impl Iterator<Item = (&'static str, &mut Box<dyn Component>)> {
        self.components
            .iter_mut()
            .map(|(name, component)| (*name, component))
    }

    pub fn component_by_name(&self, name: &str) -> Option<&dyn Component> {
        self.components
            .get(name)
            .map(|component| component.as_ref())
    }

    pub fn component_by_name_mut(&mut self, name: &str) -> Option<&mut (dyn Component + 'static)> {
        self.components
            .get_mut(name)
            .map(|component| component.as_mut())
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.parent == other.parent
            && self.transform == other.transform
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
            .field("transform", &self.transform)
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

use dome_ecs::Entity;
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub(crate) struct NodeMap(HashMap<String, Entity>);

impl NodeMap {
    pub(crate) fn new(nodes: HashMap<String, Entity>) -> Self {
        Self(nodes)
    }

    pub(crate) fn entity(&self, id: &str) -> Entity {
        *self
            .0
            .get(id)
            .unwrap_or_else(|| panic!("the scene holds no node {id}"))
    }
}

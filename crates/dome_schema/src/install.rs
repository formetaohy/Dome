use dome_ecs::Entity;
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct NodeMap(HashMap<String, Entity>);

impl NodeMap {
    pub(crate) fn new(nodes: HashMap<String, Entity>) -> Self {
        Self(nodes)
    }

    pub fn id(&self, id: &str) -> Option<Entity> {
        self.0.get(id).copied()
    }

    pub(crate) fn entity(&self, id: &str) -> Entity {
        *self
            .0
            .get(id)
            .unwrap_or_else(|| panic!("the scene holds no node {id}"))
    }
}

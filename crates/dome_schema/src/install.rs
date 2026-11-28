use dome_ecs::Entity;
use dome_scene::Scene;

#[derive(Clone, Debug)]
pub struct NodeMap {
    scene: Scene,
    entities: Vec<Entity>,
}

impl NodeMap {
    pub(crate) fn new(scene: Scene, entities: Vec<Entity>) -> Self {
        assert_eq!(
            scene.nodes().len(),
            entities.len(),
            "a node map answers one entity for every node of its scene",
        );
        Self { scene, entities }
    }

    pub fn id(&self, id: &str) -> Option<Entity> {
        self.entities.get(self.scene.node_index(id)?).copied()
    }

    pub(crate) fn entity(&self, id: &str) -> Entity {
        self.id(id)
            .unwrap_or_else(|| panic!("the scene holds no node {id}"))
    }
}

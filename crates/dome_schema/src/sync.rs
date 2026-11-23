use crate::SceneNode;
use crate::animation_library::AnimationLibrary;
use crate::assets::Assets;
use crate::graph_library::GraphLibrary;
use crate::install::NodeMap;
use crate::registry::Registry;
use dome_ecs::{Entity, World, clear_parent, propagate, set_parent};
use dome_render::SceneDamage;
use dome_scene::Changed;
use dome_scene::Node;
use dome_scene::Scene;
use dome_scene::{Reach, ReferenceKind};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct SceneSync {
    registry: Arc<Registry>,
    applied: Option<Scene>,
    entities: Vec<Entity>,
    ids: HashMap<String, Entity>,
}

impl SceneSync {
    pub fn new(registry: Arc<Registry>) -> Self {
        Self {
            registry,
            applied: None,
            entities: Vec::new(),
            ids: HashMap::new(),
        }
    }

    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    pub fn applied(&self) -> Option<&Scene> {
        self.applied.as_ref()
    }

    pub fn entity(&self, index: usize) -> Option<Entity> {
        self.entities.get(index).copied()
    }

    pub fn id(&self, id: &str) -> Option<Entity> {
        self.ids.get(id).copied()
    }

    pub fn sync(
        &mut self,
        scene: &Scene,
        world: &mut World,
        assets: &dyn Assets,
        changed: &Changed,
    ) {
        let registry = Arc::clone(&self.registry);
        registry.check(scene);
        if !changed.any() && self.applied.as_ref() == Some(scene) {
            return;
        }
        let scene_changed = self.applied.as_ref() != Some(scene);
        let applied = self.applied.as_ref();
        let mut ids = HashMap::with_capacity(scene.nodes().len());
        let mut entities = Vec::with_capacity(scene.nodes().len());
        for node in scene.nodes() {
            let held = self.ids.get(&node.id).copied();
            let reused = held.is_some_and(|entity| world.is_alive(entity));
            let entity = match held {
                Some(entity) if reused => entity,
                _ => world.spawn(),
            };
            assert!(
                ids.insert(node.id.clone(), entity).is_none(),
                "the scene holds the node {} twice",
                node.id,
            );
            entities.push((entity, reused));
        }
        let retained = ids.values().copied().collect::<HashSet<_>>();
        let mut damaged = Vec::new();
        for (index, node) in scene.nodes().iter().enumerate() {
            let (entity, _) = entities[index];
            match node.parent.as_deref() {
                Some(parent) => {
                    let parent = *ids.get(parent).unwrap_or_else(|| {
                        panic!(
                            "the scene parents {} to {parent}, which it does not hold",
                            node.id,
                        )
                    });
                    set_parent(world, entity, parent);
                }
                None => {
                    clear_parent(world, entity);
                }
            }
        }
        for entity in self.ids.values() {
            if !retained.contains(entity) {
                world.despawn(*entity);
                damaged.push(*entity);
            }
        }
        let nodes = NodeMap::new(ids.clone());
        if scene_changed
            || world.resource::<GraphLibrary>().is_none()
            || changed.holds(ReferenceKind::Animation)
            || changed.holds(ReferenceKind::Graph)
        {
            let mut animations = world
                .remove_resource::<AnimationLibrary>()
                .unwrap_or_default();
            let mut graphs = world.remove_resource::<GraphLibrary>().unwrap_or_default();
            if scene_changed {
                animations.rebuild(scene, assets, &nodes);
                graphs.rebuild(scene, &animations);
            } else {
                let rebuilt = animations.revise(scene, assets, &nodes, changed);
                graphs.revise(scene, &animations, &rebuilt, changed);
            }
            world.insert_resource(animations);
            world.insert_resource(graphs);
        }
        world.insert_resource(nodes);
        let visual = Visual::of(scene, changed);
        let mut sky = visual.sky;
        for (index, node) in scene.nodes().iter().enumerate() {
            let (entity, reused) = entities[index];
            let previous = previous(applied, index, node);
            let shared = previous.is_some_and(|held| std::ptr::eq(held, node));
            let mut touched = !reused;
            if let Some(previous) = previous {
                touched |= previous.parent != node.parent;
                touched |= previous.transform() != node.transform();
            }
            world.insert(
                entity,
                SceneNode {
                    id: node.id.clone(),
                },
            );
            crate::runtime::transform::install(world, entity, node.transform());
            for registration in registry.registrations() {
                let spec = registration.spec;
                let authored = node.component_by_name(spec.name);
                let refreshed = authored
                    .is_some_and(|component| (registration.runtime)(component).stale(changed));
                let replaced = !shared
                    && (!reused
                        || previous.is_none_or(|previous| {
                            match (previous.component_by_name(spec.name), authored) {
                                (None, None) => false,
                                (Some(held), Some(next)) => !held.equals(next),
                                _ => true,
                            }
                        }));
                if !refreshed && !replaced {
                    continue;
                }
                touched |= spec.reach == Reach::Node;
                sky |= spec.reach == Reach::World;
                match authored {
                    Some(component) if replaced => (registration.runtime)(component)
                        .replace(scene, &node.id, world, entity, assets),
                    Some(component) => (registration.runtime)(component)
                        .refresh(scene, &node.id, world, entity, assets),
                    None => registry.remove(spec.name, world, entity),
                }
            }
            touched |= visual.holds(node);
            if touched {
                damaged.push(entity);
            }
        }
        self.applied = Some(scene.clone());
        self.entities = entities.into_iter().map(|(entity, _)| entity).collect();
        self.ids = ids;
        publish(world, damaged, sky);
        dome_animation::settle(world);
        propagate(world);
    }

    pub fn apply(&mut self, scene: &Scene, world: &mut World, assets: &dyn Assets, index: usize) {
        let registry = Arc::clone(&self.registry);
        registry.check(scene);
        let Some(node) = scene.nodes().get(index) else {
            return;
        };
        let Some(entity) = self.ids.get(&node.id).copied() else {
            return;
        };
        if !world.is_alive(entity) {
            return;
        }
        let previous = self.applied.as_ref().and_then(|applied| {
            applied
                .node_index(&node.id)
                .and_then(|at| applied.nodes().get(at))
        });
        let shared = previous.is_some_and(|held| std::ptr::eq(held, node));
        let mut touched = previous.is_none_or(|held| held.transform() != node.transform())
            || previous.is_some_and(|held| held.parent != node.parent);
        let mut sky = false;
        match node.parent.as_deref() {
            Some(parent) => {
                let Some(parent) = self.ids.get(parent).copied() else {
                    return;
                };
                set_parent(world, entity, parent);
            }
            None => {
                clear_parent(world, entity);
            }
        }
        crate::runtime::transform::install(world, entity, node.transform());
        for registration in registry.registrations() {
            let spec = registration.spec;
            let authored = node.component_by_name(spec.name);
            let replaced = !shared
                && previous.is_none_or(|previous| {
                    match (previous.component_by_name(spec.name), authored) {
                        (None, None) => false,
                        (Some(authored), Some(next)) => !authored.equals(next),
                        _ => true,
                    }
                });
            if !replaced {
                continue;
            }
            touched |= spec.reach == Reach::Node;
            sky |= spec.reach == Reach::World;
            match authored {
                Some(component) => (registration.runtime)(component)
                    .replace(scene, &node.id, world, entity, assets),
                None => registry.remove(spec.name, world, entity),
            }
        }
        if let Some(applied) = &mut self.applied
            && let Some(handle) = scene.node_handle(index).cloned()
            && let Some(at) = applied.node_index(&node.id)
        {
            applied.remember_node(at, handle);
        }
        publish(world, touched.then_some(entity).into_iter().collect(), sky);
        dome_animation::settle(world);
        propagate(world);
    }
}

fn previous<'a>(applied: Option<&'a Scene>, index: usize, node: &Node) -> Option<&'a Node> {
    let applied = applied?;
    let at = applied
        .nodes()
        .get(index)
        .filter(|held| held.id == node.id)
        .map_or_else(|| applied.node_index(&node.id), |_| Some(index))?;
    applied.nodes().get(at)
}

fn publish(world: &mut World, nodes: Vec<Entity>, sky: bool) {
    if nodes.is_empty() && !sky {
        return;
    }
    match world.resource_mut::<SceneDamage>() {
        Some(held) => held.absorb(nodes, sky),
        None => {
            world.insert_resource(SceneDamage::new(1, nodes, sky));
        }
    }
}

struct Visual<'a> {
    meshes: HashSet<&'a str>,
    skins: HashSet<&'a str>,
    materials: HashSet<&'a str>,
    sky: bool,
}

impl<'a> Visual<'a> {
    fn of(scene: &'a Scene, changed: &'a Changed) -> Self {
        let materials = scene
            .materials()
            .iter()
            .filter(|entry| {
                changed.touches(ReferenceKind::Material, &entry.id)
                    || changed
                        .ids(ReferenceKind::Image)
                        .any(|id| entry.asset.references_image(id))
            })
            .map(|entry| entry.id.as_str())
            .collect();
        Self {
            meshes: changed.ids(ReferenceKind::Mesh).collect(),
            skins: changed.ids(ReferenceKind::Skin).collect(),
            materials,
            sky: changed.holds(ReferenceKind::Environment),
        }
    }

    fn holds(&self, node: &Node) -> bool {
        if self.meshes.is_empty() && self.skins.is_empty() && self.materials.is_empty() {
            return false;
        }
        node.components().any(|(_, component)| {
            self.meshes
                .iter()
                .any(|id| component.references(ReferenceKind::Mesh, id))
                || self
                    .skins
                    .iter()
                    .any(|id| component.references(ReferenceKind::Skin, id))
                || self
                    .materials
                    .iter()
                    .any(|id| component.references(ReferenceKind::Material, id))
        })
    }
}

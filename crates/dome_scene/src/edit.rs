use crate::animation::AnimationEntry;
use crate::component::ReferenceKind;
use crate::deletion;
use crate::delta::{Asset, Delta, Splice, Step};
use crate::environment::EnvironmentEntry;
use crate::graph::GraphEntry;
use crate::image::ImageEntry;
use crate::material::{MaterialAsset, MaterialEntry};
use crate::mesh::{MeshAsset, MeshEntry};
use crate::model::ModelEntry;
use crate::node::Node;
use crate::scene::{Owned, Scene};
use crate::shape::Shape;
use crate::skin::SkinEntry;
use crate::sound::SoundEntry;
use crate::target::{Target, valid_id};
use crate::value::Value;
use dome_math::Transform;
use dome_render::environment::Atmosphere;
use std::collections::BTreeSet;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Edit<'a> {
    pub(crate) scene: &'a mut Scene,
    delta: Delta,
    settled: bool,
}

impl Scene {
    pub fn edit<T>(
        &mut self,
        edit: impl FnOnce(&mut Edit<'_>) -> Result<T, String>,
    ) -> Result<(T, Delta), String> {
        let mut held = Edit::begin(self);
        let value = edit(&mut held)?;
        Ok((value, held.settle()))
    }
}

impl<'a> Edit<'a> {
    fn begin(scene: &'a mut Scene) -> Self {
        Self {
            scene,
            delta: Delta::default(),
            settled: false,
        }
    }

    fn settle(mut self) -> Delta {
        self.settled = true;
        let delta = std::mem::take(&mut self.delta);
        if !delta.is_empty() {
            self.scene.touch();
        }
        delta
    }

    pub(crate) fn insert_nodes(&mut self, at: usize, nodes: Vec<Node>) {
        assert!(!nodes.is_empty(), "an edit inserts at least one node");
        let inserted = nodes.into_iter().map(Arc::new).collect::<Vec<_>>();
        self.scene.nodes.splice(at, 0, inserted.clone());
        self.delta.push(Step::Nodes(Splice::insert(at, inserted)));
    }

    pub fn append_nodes(&mut self, nodes: Vec<Node>) {
        let at = self.scene.nodes.len();
        self.insert_nodes(at, nodes);
    }

    pub(crate) fn remove_nodes(&mut self, indices: &[usize]) {
        let mut sorted = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for at in sorted {
            match runs.last_mut() {
                Some((start, held)) if *start + *held == at => *held += 1,
                _ => runs.push((at, 1)),
            }
        }
        for (start, held) in runs.into_iter().rev() {
            let removed = self.scene.nodes.splice(start, held, Vec::new());
            self.delta.push(Step::Nodes(Splice::remove(start, removed)));
        }
    }

    pub(crate) fn place_node(&mut self, at: usize, node: Node) {
        let inserted = Arc::new(node);
        if self.scene.nodes[at] == *inserted {
            return;
        }
        let removed = self.scene.nodes.replace(at, inserted.clone());
        self.delta.push(Step::Nodes(Splice {
            at,
            removed: vec![removed],
            inserted: vec![inserted],
        }));
    }

    pub(crate) fn change_node<R>(
        &mut self,
        at: usize,
        change: impl FnOnce(&mut Node, &Scene) -> Result<R, String>,
    ) -> Result<R, String> {
        let mut node = self.scene.nodes[at].clone();
        let result = change(&mut node, self.scene)?;
        self.place_node(at, node);
        Ok(result)
    }

    pub(crate) fn add_asset<A: Asset>(&mut self, entry: A, owner: Owned) {
        let entries = A::entries(self.scene);
        let at = entries.len();
        entries.push(entry.clone());
        self.delta.push(A::step(Splice::insert(at, vec![entry])));
        self.add_owner(owner);
    }

    pub(crate) fn set_asset<A: Asset>(&mut self, at: usize, entry: A) {
        if A::held(self.scene)[at] == entry {
            return;
        }
        let removed = std::mem::replace(&mut A::entries(self.scene)[at], entry.clone());
        self.delta.push(A::step(Splice {
            at,
            removed: vec![removed],
            inserted: vec![entry],
        }));
    }

    pub(crate) fn change_asset<A: Asset, R>(
        &mut self,
        at: usize,
        change: impl FnOnce(&mut A, &Scene) -> Result<R, String>,
    ) -> Result<R, String> {
        let mut entry = A::held(self.scene)[at].clone();
        let result = change(&mut entry, self.scene)?;
        self.set_asset(at, entry);
        Ok(result)
    }

    pub(crate) fn remove_asset<A: Asset>(&mut self, at: usize) {
        let removed = A::entries(self.scene).remove(at);
        self.delta.push(A::step(Splice::remove(at, vec![removed])));
    }

    pub(crate) fn add_owner(&mut self, owner: Owned) {
        let owners = Arc::make_mut(&mut self.scene.owners);
        let at = owners.len();
        owners.push(owner.clone());
        self.delta
            .push(Step::Owners(Splice::insert(at, vec![owner])));
    }

    pub(crate) fn set_owners(&mut self, owners: Vec<Owned>) {
        let held = Arc::make_mut(&mut self.scene.owners);
        if *held == owners {
            return;
        }
        let before = std::mem::replace(held, owners.clone());
        self.delta.push(Step::Owners(Splice {
            at: 0,
            removed: before,
            inserted: owners,
        }));
    }

    pub(crate) fn change_owner(
        &mut self,
        kind: ReferenceKind,
        id: &str,
        change: impl FnOnce(&mut Owned),
    ) {
        let at = self
            .scene
            .owners
            .iter()
            .position(|held| held.kind == kind && held.id == id)
            .expect("the scene owns the asset it changes");
        let owners = Arc::make_mut(&mut self.scene.owners);
        let mut owner = owners[at].clone();
        change(&mut owner);
        if owners[at] == owner {
            return;
        }
        let removed = std::mem::replace(&mut owners[at], owner.clone());
        self.delta.push(Step::Owners(Splice {
            at,
            removed: vec![removed],
            inserted: vec![owner],
        }));
    }

    pub(crate) fn set_instance(&mut self, key: &str, keys: Option<BTreeSet<PathBuf>>) {
        let before = self.scene.instances.get(key).cloned();
        if before == keys {
            return;
        }
        let instances = Arc::make_mut(&mut self.scene.instances);
        match &keys {
            Some(held) => {
                instances.insert(String::from(key), held.clone());
            }
            None => {
                instances.remove(key);
            }
        }
        self.delta.push(Step::Instances {
            key: String::from(key),
            before,
            after: keys,
        });
    }

    pub(crate) fn mark(&self) -> usize {
        self.delta.len()
    }

    pub(crate) fn rollback(&mut self, mark: usize) {
        while self.delta.len() > mark {
            self.revert_step();
        }
    }

    pub(crate) fn revert_step(&mut self) {
        let step = self
            .delta
            .pop()
            .expect("an edit returns to the step it records");
        step.revert(self.scene);
    }

    pub(crate) fn check_audio_buses(&self) -> Result<(), String> {
        self.scene.check_audio_buses()
    }

    pub fn add_node(&mut self, id: &str) -> Result<Target, String> {
        self.insert_node(Node::new(id))
    }

    pub fn insert_node(&mut self, node: Node) -> Result<Target, String> {
        assert_id(&node.id, "node")?;
        if self.node_index(&node.id).is_some() {
            return Err(format!("the scene holds the node {}", node.id));
        }
        let id = node.id.clone();
        self.append_nodes(vec![node]);
        Ok(Target::Node(id))
    }

    pub fn add_instance(&mut self, id: &str, path: &str) -> Result<Target, String> {
        assert_id(id, "prefab instance")?;
        if self.node_index(id).is_some() {
            return Err(format!("the scene holds the node {id}"));
        }
        self.attach_instance(Node::instance(id, crate::folder::relative_path(path)))
    }

    pub fn add_mesh(&mut self, id: &str, shape: Shape) -> Result<Target, String> {
        assert_id(id, "mesh")?;
        if self.mesh_index(id).is_some() {
            return Err(format!("the scene holds the mesh {id}"));
        }
        self.adopt_asset(MeshEntry::new(id, MeshAsset::shape(shape)));
        Ok(Target::Mesh(String::from(id)))
    }

    pub fn add_material(&mut self, id: &str) -> Result<Target, String> {
        assert_id(id, "material")?;
        if self.material_index(id).is_some() {
            return Err(format!("the scene holds the material {id}"));
        }
        self.adopt_asset(MaterialEntry::new(id, MaterialAsset::grey()));
        Ok(Target::Material(String::from(id)))
    }

    pub fn add_sound(&mut self, id: &str, path: &str) -> Result<Target, String> {
        assert_id(id, "sound")?;
        if self.sound_index(id).is_some() {
            return Err(format!("the scene holds the sound {id}"));
        }
        let path = self.relative(id, "sound", path)?;
        self.adopt_asset(SoundEntry::new(id, path));
        Ok(Target::Sound(String::from(id)))
    }

    pub fn add_environment(&mut self, id: &str, path: &str) -> Result<Target, String> {
        assert_id(id, "environment")?;
        if self.environment_index(id).is_some() {
            return Err(format!("the scene holds the environment {id}"));
        }
        let path = self.relative(id, "environment", path)?;
        self.adopt_asset(crate::environment::EnvironmentEntry::new(id, path));
        Ok(Target::Environment(String::from(id)))
    }

    pub fn add_atmosphere(&mut self, id: &str, atmosphere: Atmosphere) -> Result<Target, String> {
        assert_id(id, "environment")?;
        if self.environment_index(id).is_some() {
            return Err(format!("the scene holds the environment {id}"));
        }
        self.adopt_asset(crate::environment::EnvironmentEntry::with_atmosphere(
            id, atmosphere,
        ));
        Ok(Target::Environment(String::from(id)))
    }

    pub fn add_capture_environment(&mut self, id: &str) -> Result<Target, String> {
        assert_id(id, "environment")?;
        if self.environment_index(id).is_some() {
            return Err(format!("the scene holds the environment {id}"));
        }
        self.adopt_asset(crate::environment::EnvironmentEntry::captured(id));
        Ok(Target::Environment(String::from(id)))
    }

    pub fn add_image(&mut self, id: &str, path: &str) -> Result<Target, String> {
        assert_id(id, "image")?;
        if self.image_index(id).is_some() {
            return Err(format!("the scene holds the image {id}"));
        }
        let path = self.relative(id, "image", path)?;
        self.adopt_asset(ImageEntry::new(id, path));
        Ok(Target::Image(String::from(id)))
    }

    pub fn add_model(&mut self, id: &str, path: &str) -> Result<Target, String> {
        assert_id(id, "model")?;
        if self.model_index(id).is_some() {
            return Err(format!("the scene holds the model {id}"));
        }
        let path = self.relative(id, "model", path)?;
        self.adopt_asset(ModelEntry::new(id, path));
        Ok(Target::Model(String::from(id)))
    }

    pub fn add_graph(&mut self, id: &str) -> Result<Target, String> {
        assert_id(id, "graph")?;
        if self.graph_index(id).is_some() {
            return Err(format!("the scene holds the graph {id}"));
        }
        let clip = self
            .animations
            .first()
            .ok_or_else(|| String::from("the scene holds no animation to graph"))?;
        let state = crate::graph::GraphState::new(
            "default",
            crate::reference::AnimationId::new(clip.id.as_str()),
            1.0,
            true,
        )?;
        self.adopt_asset(GraphEntry::new(
            id,
            crate::graph::GraphAsset::new("default", Vec::new(), vec![state], Vec::new()),
        ));
        Ok(Target::Graph(String::from(id)))
    }

    fn adopt_asset<A: Asset>(&mut self, entry: A) {
        let entries = A::entries(self.scene);
        let at = entries.len();
        entries.push(entry.clone());
        self.delta.push(A::step(Splice::insert(at, vec![entry])));
    }

    pub fn adopt(&mut self, entry: impl Into<AssetEntry>) {
        match entry.into() {
            AssetEntry::Model(entry) => self.adopt_asset(entry),
            AssetEntry::Image(entry) => self.adopt_asset(entry),
            AssetEntry::Sound(entry) => self.adopt_asset(entry),
            AssetEntry::Environment(entry) => self.adopt_asset(entry),
            AssetEntry::Mesh(entry) => self.adopt_asset(entry),
            AssetEntry::Material(entry) => self.adopt_asset(entry),
            AssetEntry::Skin(entry) => self.adopt_asset(entry),
            AssetEntry::Animation(entry) => self.adopt_asset(entry),
            AssetEntry::Graph(entry) => self.adopt_asset(entry),
        }
    }

    fn relative(&self, id: &str, kind: &str, path: &str) -> Result<PathBuf, String> {
        let path = crate::folder::relative_path(path);
        if !crate::folder::Folder::Asset.holds(&path) {
            return Err(format!(
                "the {kind} {id} holds the path {} outside the assets folder",
                path.display(),
            ));
        }
        if !self.scene.asset(&path).is_file() {
            return Err(format!(
                "the {kind} {id} names {}, which the project {} does not hold",
                path.display(),
                self.root.display(),
            ));
        }
        Ok(path)
    }

    pub fn remove(&mut self, target: &Target) -> Result<(), String> {
        let deletion = deletion::plan(self.scene, target)?;
        deletion::apply(self, deletion);
        Ok(())
    }

    pub fn reparent(&mut self, index: usize, parent: Option<usize>) {
        assert!(index < self.nodes.len(), "a reparented node exists");
        assert!(
            self.nodes[index].owner.is_none(),
            "the node {} belongs to a prefab instance",
            self.nodes[index].id,
        );
        if let Some(parent) = parent {
            assert!(parent < self.nodes.len(), "a named parent exists");
            assert!(
                !self.is_ancestor(index, parent),
                "{} parents itself through {}",
                self.nodes[index].id,
                self.nodes[parent].id,
            );
        }
        let id = parent.map(|at| self.nodes[at].id.clone());
        self.change_node(index, |node, _| {
            node.parent = id;
            Ok(())
        })
        .expect("a reparented node writes its parent");
        self.scene.nodes.reindex();
    }

    pub fn set_transform(&mut self, index: usize, transform: Transform) -> Result<(), String> {
        assert!(index < self.nodes.len(), "a transformed node exists");
        if self.nodes[index].owner.is_none() {
            return self.change_node(index, |node, _| {
                node.set_transform(transform);
                Ok(())
            });
        }
        let current = self.transform(index);
        if current.translation != transform.translation {
            self.write_override(
                index,
                "transform.translation",
                &Value::Vector(transform.translation),
            )?;
        }
        if current.rotation != transform.rotation {
            self.write_override(
                index,
                "transform.rotation",
                &Value::Vector(crate::field::euler(transform.rotation)),
            )?;
        }
        if current.scale != transform.scale {
            self.write_override(index, "transform.scale", &Value::Vector(transform.scale))?;
        }
        Ok(())
    }
}

impl Deref for Edit<'_> {
    type Target = Scene;

    fn deref(&self) -> &Scene {
        self.scene
    }
}

impl Drop for Edit<'_> {
    fn drop(&mut self) {
        if !self.settled {
            let delta = std::mem::take(&mut self.delta);
            delta.revert(self.scene);
        }
    }
}

fn assert_id(id: &str, what: &str) -> Result<(), String> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(format!("the {what} id {id} holds no letter, digit, _ or -"))
    }
}

pub enum AssetEntry {
    Model(ModelEntry),
    Image(ImageEntry),
    Sound(SoundEntry),
    Environment(EnvironmentEntry),
    Mesh(MeshEntry),
    Material(MaterialEntry),
    Skin(SkinEntry),
    Animation(AnimationEntry),
    Graph(GraphEntry),
}

macro_rules! asset_entry {
    ($($kind:ident $entry:ident),+ $(,)?) => {
        $(
            impl From<$entry> for AssetEntry {
                fn from(entry: $entry) -> Self {
                    Self::$kind(entry)
                }
            }
        )+
    };
}

asset_entry!(
    Model ModelEntry,
    Image ImageEntry,
    Sound SoundEntry,
    Environment EnvironmentEntry,
    Mesh MeshEntry,
    Material MaterialEntry,
    Skin SkinEntry,
    Animation AnimationEntry,
    Graph GraphEntry,
);

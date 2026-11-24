use crate::animation::AnimationEntry;
use crate::component::{ReferenceKind, Schema};
use crate::environment::EnvironmentEntry;
use crate::folder::Folder;
use crate::graph::GraphEntry;
use crate::image::ImageEntry;
use crate::material::MaterialEntry;
use crate::mesh::MeshEntry;
use crate::model::ModelEntry;
use crate::node::Node;
use crate::nodes::Nodes;
use crate::skin::SkinEntry;
use crate::sound::SoundEntry;
use crate::target::{Target, valid_id};
use dome_math::Transform;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) const FORMAT: &str = "dome_scene";

#[derive(Clone)]
pub struct Scene {
    pub(crate) root: PathBuf,
    pub(crate) schema: Arc<dyn Schema>,
    pub(crate) prefab: bool,
    pub(crate) owners: Arc<Vec<Owned>>,
    pub(crate) instances: Arc<BTreeMap<String, BTreeSet<PathBuf>>>,
    pub(crate) images: Arc<Vec<ImageEntry>>,
    pub(crate) sounds: Arc<Vec<SoundEntry>>,
    pub(crate) environments: Arc<Vec<EnvironmentEntry>>,
    pub(crate) meshes: Arc<Vec<MeshEntry>>,
    pub(crate) materials: Arc<Vec<MaterialEntry>>,
    pub(crate) models: Arc<Vec<ModelEntry>>,
    pub(crate) skins: Arc<Vec<SkinEntry>>,
    pub(crate) animations: Arc<Vec<AnimationEntry>>,
    pub(crate) graphs: Arc<Vec<GraphEntry>>,
    pub(crate) nodes: Nodes,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Owned {
    pub(crate) kind: ReferenceKind,
    pub(crate) id: String,
    pub(crate) prefab: PathBuf,
    pub(crate) scope: Option<String>,
}

impl fmt::Debug for Scene {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Scene")
            .field("root", &self.root)
            .field("prefab", &self.prefab)
            .field("owners", &self.owners)
            .field("instances", &self.instances)
            .field("images", &self.images)
            .field("sounds", &self.sounds)
            .field("environments", &self.environments)
            .field("meshes", &self.meshes)
            .field("materials", &self.materials)
            .field("models", &self.models)
            .field("skins", &self.skins)
            .field("animations", &self.animations)
            .field("graphs", &self.graphs)
            .field("nodes", &self.nodes)
            .finish()
    }
}

impl PartialEq for Scene {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
            && self.prefab == other.prefab
            && shared(&self.owners, &other.owners)
            && shared(&self.instances, &other.instances)
            && shared(&self.images, &other.images)
            && shared(&self.sounds, &other.sounds)
            && shared(&self.meshes, &other.meshes)
            && shared(&self.environments, &other.environments)
            && shared(&self.materials, &other.materials)
            && shared(&self.models, &other.models)
            && shared(&self.skins, &other.skins)
            && shared(&self.animations, &other.animations)
            && shared(&self.graphs, &other.graphs)
            && self.nodes == other.nodes
    }
}

impl Scene {
    pub fn new(root: impl Into<PathBuf>, schema: Arc<dyn Schema>) -> Self {
        Self::with_schema(root, schema)
    }

    pub fn with_schema(root: impl Into<PathBuf>, schema: Arc<dyn Schema>) -> Self {
        Self {
            root: root.into(),
            schema,
            prefab: false,
            owners: Arc::new(Vec::new()),
            instances: Arc::new(BTreeMap::new()),
            images: Arc::new(Vec::new()),
            sounds: Arc::new(Vec::new()),
            environments: Arc::new(Vec::new()),
            meshes: Arc::new(Vec::new()),
            materials: Arc::new(Vec::new()),
            models: Arc::new(Vec::new()),
            skins: Arc::new(Vec::new()),
            animations: Arc::new(Vec::new()),
            graphs: Arc::new(Vec::new()),
            nodes: Nodes::new(),
        }
    }

    pub fn load(path: impl AsRef<Path>, root: impl Into<PathBuf>, schema: Arc<dyn Schema>) -> Self {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("the scene {} reads no text: {error}", path.display()));
        Self::parse(
            &text,
            root,
            &format!("the scene {}", path.display()),
            schema,
        )
    }

    pub fn parse(
        text: &str,
        root: impl Into<PathBuf>,
        what: &str,
        schema: Arc<dyn Schema>,
    ) -> Self {
        let root = root.into();
        crate::prefab::Loader::new(root.clone(), schema).document(text, what)
    }

    pub fn write(&self) -> String {
        self.validate("a written scene");
        crate::document::write(self)
    }

    pub fn save(&self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        let text = self.write();
        let mut staging = path.as_os_str().to_owned();
        staging.push(format!(".{}.tmp", std::process::id()));
        let staging = PathBuf::from(staging);
        let written = fs::File::create(&staging)
            .and_then(|mut file| {
                std::io::Write::write_all(&mut file, text.as_bytes())?;
                file.sync_all()
            })
            .and_then(|()| fs::rename(&staging, path));
        written.unwrap_or_else(|error| {
            let _ = fs::remove_file(&staging);
            panic!("the scene {} writes no text: {error}", path.display())
        });
    }

    pub fn schema(&self) -> &dyn Schema {
        self.schema.as_ref()
    }

    pub fn schema_arc(&self) -> Arc<dyn Schema> {
        Arc::clone(&self.schema)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn asset(&self, path: &Path) -> PathBuf {
        Folder::Asset.resolve(&self.root, path)
    }

    pub fn prefab(&self, path: &Path) -> PathBuf {
        Folder::Prefab.resolve(&self.root, path)
    }

    pub fn instances(&self) -> &BTreeMap<String, BTreeSet<PathBuf>> {
        &self.instances
    }

    pub fn is_prefab(&self) -> bool {
        self.prefab
    }

    pub fn node_owner(&self, index: usize) -> Option<&str> {
        self.nodes.get(index)?.owner.as_deref()
    }

    pub fn instance_prefab(&self, marker: &str) -> Option<&Path> {
        self.node_index(marker)
            .and_then(|index| self.nodes[index].prefab())
    }

    pub fn asset_owner(&self, kind: ReferenceKind, id: &str) -> Option<&Path> {
        self.owners
            .iter()
            .find(|held| held.kind == kind && held.id == id)
            .map(|held| held.prefab.as_path())
    }

    pub(crate) fn scoped_away(
        &self,
        kind: ReferenceKind,
        id: &str,
        removed: &HashSet<String>,
    ) -> bool {
        self.owners.iter().any(|held| {
            held.kind == kind
                && held.id == id
                && held
                    .scope
                    .as_deref()
                    .is_some_and(|scope| removed.contains(scope))
        })
    }

    pub fn prefab_owner(&self, target: &Target) -> Option<&Path> {
        match target {
            Target::Node(id) => {
                let index = self.node_index(id)?;
                self.nodes[index].owner.as_deref()?;
                let marker = self.instance_marker(id)?;
                self.instance_prefab(marker)
            }
            Target::Mesh(id) => self.asset_owner(ReferenceKind::Mesh, id),
            Target::Material(id) => self.asset_owner(ReferenceKind::Material, id),
            Target::Image(id) => self.asset_owner(ReferenceKind::Image, id),
            Target::Model(id) => self.asset_owner(ReferenceKind::Model, id),
            Target::Sound(id) => self.asset_owner(ReferenceKind::Sound, id),
            Target::Environment(id) => self.asset_owner(ReferenceKind::Environment, id),
            Target::Graph(id) => self.asset_owner(ReferenceKind::Graph, id),
        }
    }

    pub fn images(&self) -> &[ImageEntry] {
        &self.images
    }

    pub fn meshes(&self) -> &[MeshEntry] {
        &self.meshes
    }

    pub fn materials(&self) -> &[MaterialEntry] {
        &self.materials
    }

    pub fn models(&self) -> &[ModelEntry] {
        &self.models
    }

    pub fn skins(&self) -> &[SkinEntry] {
        &self.skins
    }

    pub fn animations(&self) -> &[AnimationEntry] {
        &self.animations
    }

    pub fn graphs(&self) -> &[GraphEntry] {
        &self.graphs
    }

    pub fn nodes(&self) -> &Nodes {
        &self.nodes
    }

    pub fn sounds(&self) -> &[SoundEntry] {
        &self.sounds
    }

    pub fn environments(&self) -> &[EnvironmentEntry] {
        &self.environments
    }

    pub fn remember_node(&mut self, at: usize, node: Arc<Node>) {
        self.nodes.replace(at, node);
    }

    pub fn node_handle(&self, index: usize) -> Option<&Arc<Node>> {
        self.nodes.handles().get(index)
    }

    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.nodes.index_of(id)
    }

    pub fn mesh_index(&self, id: &str) -> Option<usize> {
        self.meshes.iter().position(|mesh| mesh.id == id)
    }

    pub fn material_index(&self, id: &str) -> Option<usize> {
        self.materials.iter().position(|material| material.id == id)
    }

    pub fn image_index(&self, id: &str) -> Option<usize> {
        self.images.iter().position(|image| image.id == id)
    }

    pub fn sound_index(&self, id: &str) -> Option<usize> {
        self.sounds.iter().position(|sound| sound.id == id)
    }

    pub fn environment_index(&self, id: &str) -> Option<usize> {
        self.environments
            .iter()
            .position(|environment| environment.id == id)
    }

    pub fn model_index(&self, id: &str) -> Option<usize> {
        self.models.iter().position(|model| model.id == id)
    }

    pub fn skin_index(&self, id: &str) -> Option<usize> {
        self.skins.iter().position(|skin| skin.id == id)
    }

    pub fn animation_index(&self, id: &str) -> Option<usize> {
        self.animations
            .iter()
            .position(|animation| animation.id == id)
    }

    pub fn graph_index(&self, id: &str) -> Option<usize> {
        self.graphs.iter().position(|graph| graph.id == id)
    }

    pub fn image(&self, id: &str) -> Option<&ImageEntry> {
        self.images.iter().find(|image| image.id == id)
    }

    pub fn sound(&self, id: &str) -> Option<&SoundEntry> {
        self.sounds.iter().find(|sound| sound.id == id)
    }

    pub fn environment(&self, id: &str) -> Option<&EnvironmentEntry> {
        self.environments
            .iter()
            .find(|environment| environment.id == id)
    }

    pub fn mesh(&self, id: &str) -> Option<&MeshEntry> {
        self.meshes.iter().find(|mesh| mesh.id == id)
    }

    pub fn material(&self, id: &str) -> Option<&MaterialEntry> {
        self.materials.iter().find(|material| material.id == id)
    }

    pub fn model(&self, id: &str) -> Option<&ModelEntry> {
        self.models.iter().find(|model| model.id == id)
    }

    pub fn skin(&self, id: &str) -> Option<&SkinEntry> {
        self.skins.iter().find(|skin| skin.id == id)
    }

    pub fn animation(&self, id: &str) -> Option<&AnimationEntry> {
        self.animations.iter().find(|animation| animation.id == id)
    }

    pub fn graph(&self, id: &str) -> Option<&GraphEntry> {
        self.graphs.iter().find(|graph| graph.id == id)
    }

    pub fn holds(&self, target: &Target) -> bool {
        match target {
            Target::Node(id) => self.node_index(id).is_some(),
            Target::Mesh(id) => self.mesh_index(id).is_some(),
            Target::Material(id) => self.material_index(id).is_some(),
            Target::Image(id) => self.image_index(id).is_some(),
            Target::Model(id) => self.model_index(id).is_some(),
            Target::Sound(id) => self.sound_index(id).is_some(),
            Target::Environment(id) => self.environment_index(id).is_some(),
            Target::Graph(id) => self.graph_index(id).is_some(),
        }
    }

    pub fn holds_reference(&self, kind: ReferenceKind, id: &str) -> bool {
        match kind {
            ReferenceKind::Node => self.node_index(id).is_some(),
            ReferenceKind::Mesh => self.mesh_index(id).is_some(),
            ReferenceKind::Material => self.material_index(id).is_some(),
            ReferenceKind::Image => self.image_index(id).is_some(),
            ReferenceKind::Model => self.model_index(id).is_some(),
            ReferenceKind::Skin => self.skin_index(id).is_some(),
            ReferenceKind::Animation => self.animation_index(id).is_some(),
            ReferenceKind::Graph => self.graph_index(id).is_some(),
            ReferenceKind::Sound => self.sound_index(id).is_some(),
            ReferenceKind::Environment => self.environment_index(id).is_some(),
            ReferenceKind::AudioBus => self
                .node_index(id)
                .is_some_and(|index| self.nodes[index].has_component("audio_bus")),
        }
    }

    pub fn asset_ids(&self, kind: ReferenceKind) -> Vec<&str> {
        match kind {
            ReferenceKind::Node => self.nodes.iter().map(|node| node.id.as_str()).collect(),
            ReferenceKind::Mesh => self.meshes.iter().map(|mesh| mesh.id.as_str()).collect(),
            ReferenceKind::Material => self
                .materials
                .iter()
                .map(|material| material.id.as_str())
                .collect(),
            ReferenceKind::Image => self.images.iter().map(|image| image.id.as_str()).collect(),
            ReferenceKind::Model => self.models.iter().map(|model| model.id.as_str()).collect(),
            ReferenceKind::Skin => self.skins.iter().map(|skin| skin.id.as_str()).collect(),
            ReferenceKind::Animation => self
                .animations
                .iter()
                .map(|animation| animation.id.as_str())
                .collect(),
            ReferenceKind::Graph => self.graphs.iter().map(|graph| graph.id.as_str()).collect(),
            ReferenceKind::Sound => self.sounds.iter().map(|sound| sound.id.as_str()).collect(),
            ReferenceKind::Environment => self
                .environments
                .iter()
                .map(|environment| environment.id.as_str())
                .collect(),
            ReferenceKind::AudioBus => self
                .nodes
                .iter()
                .filter(|node| node.has_component("audio_bus"))
                .map(|node| node.id.as_str())
                .collect(),
        }
    }

    pub(crate) fn check_audio_buses(&self) -> Result<(), String> {
        self.schema.validate(self)
    }

    pub(crate) fn referenced_by(&self, kind: ReferenceKind, id: &str) -> bool {
        self.nodes.iter().any(|node| {
            node.components()
                .any(|(_, component)| component.references(kind, id))
        })
    }

    pub fn parent_index(&self, index: usize) -> Option<usize> {
        let parent = self.nodes.get(index)?.parent.as_deref()?;
        self.node_index(parent)
    }

    pub fn ancestors_hold(&self, index: usize, component: &str) -> bool {
        let mut at = self.parent_index(index);
        while let Some(current) = at {
            if self.nodes[current].has_component(component) {
                return true;
            }
            at = self.parent_index(current);
        }
        false
    }

    pub fn subtree_holds(&self, index: usize, component: &str) -> bool {
        self.subtree(index)
            .into_iter()
            .any(|at| self.nodes[at].has_component(component))
    }

    pub fn is_ancestor(&self, ancestor: usize, node: usize) -> bool {
        let mut at = Some(node);
        let mut steps = 0;
        while let Some(current) = at {
            if current == ancestor {
                return true;
            }
            steps += 1;
            assert!(
                steps <= self.nodes.len(),
                "the scene closes the parent chain of {} into a cycle",
                self.nodes[node].id,
            );
            at = self.parent_index(current);
        }
        false
    }

    pub fn children(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        self.nodes.children_of(index).iter().copied()
    }

    pub fn roots(&self) -> impl Iterator<Item = usize> + '_ {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.parent.is_none())
            .map(|(at, _)| at)
    }

    pub fn subtree(&self, index: usize) -> Vec<usize> {
        let mut held = Vec::new();
        let mut stack = vec![index];
        while let Some(at) = stack.pop() {
            held.push(at);
            stack.extend(self.children(at));
        }
        held.sort_unstable();
        held
    }

    pub fn transform(&self, index: usize) -> Transform {
        self.nodes
            .get(index)
            .unwrap_or_else(|| panic!("a scene node holds its transform"))
            .transform()
    }

    pub fn world_transform(&self, index: usize) -> Transform {
        let mut chain = Vec::new();
        let mut at = Some(index);
        while let Some(current) = at {
            chain.push(current);
            assert!(
                chain.len() <= self.nodes.len(),
                "the scene closes the parent chain of {} into a cycle",
                self.nodes[index].id,
            );
            at = self.parent_index(current);
        }
        let mut world = Transform::IDENTITY;
        for at in chain.into_iter().rev() {
            world = world.compose(self.transform(at));
        }
        world
    }

    pub(crate) fn validate(&self, what: &str) {
        unique(self.images.iter(), what, "image");
        unique(self.sounds.iter(), what, "sound");
        unique(self.meshes.iter(), what, "mesh");
        unique(self.materials.iter(), what, "material");
        unique(self.models.iter(), what, "model");
        unique(self.skins.iter(), what, "skin");
        unique(self.animations.iter(), what, "animation");
        unique(self.graphs.iter(), what, "graph");
        for graph in self.graphs.iter() {
            graph
                .asset
                .validate(|clip| self.animation_index(clip).is_some());
        }
        unique(self.nodes.iter(), what, "node");
        for node in &self.nodes {
            if node.prefab.is_some() {
                assert!(
                    node.components().next().is_none(),
                    "the prefab instance {} holds its transform alone",
                    node.id,
                );
                let path = node
                    .prefab
                    .as_deref()
                    .expect("a prefab instance names its prefab");
                assert!(
                    Folder::Prefab.holds(path),
                    "the prefab instance {} holds the path {} outside the prefabs folder",
                    node.id,
                    path.display(),
                );
            }
            if let Some(owner) = &node.owner {
                assert!(
                    self.node_index(owner)
                        .is_some_and(|index| self.nodes[index].prefab.is_some()),
                    "{what} holds the node {} of the prefab instance {owner}, which it does not hold",
                    node.id,
                );
            }
            assert!(
                node.overrides().is_empty() || node.prefab.is_some(),
                "{what} holds the node {} of overrides and no prefab",
                node.id,
            );
            for entry in node.overrides() {
                assert!(
                    !entry.node.is_empty() && valid_id(&entry.node),
                    "{what} holds the override node {} of {}",
                    entry.node,
                    node.id,
                );
                assert!(
                    !entry.path.is_empty() && entry.path != "parent",
                    "{what} holds the override {}\\.{} of {}",
                    entry.node,
                    entry.path,
                    node.id,
                );
            }
            assert!(
                node.additions().is_empty() || node.prefab.is_some(),
                "{what} holds the node {} of additions and no prefab",
                node.id,
            );
            assert!(
                node.removals().is_empty() || node.prefab.is_some(),
                "{what} holds the node {} of removals and no prefab",
                node.id,
            );
            let mut additions = HashSet::new();
            for addition in node.additions() {
                assert!(
                    !addition.id.is_empty() && valid_id(&addition.id),
                    "{what} holds the addition {} of {}",
                    addition.id,
                    node.id,
                );
                assert!(
                    additions.insert(addition.id.as_str()),
                    "{what} holds the addition {} of {} twice",
                    addition.id,
                    node.id,
                );
                assert!(
                    addition.owner.is_none()
                        && addition.additions.is_empty()
                        && addition.removals.is_empty(),
                    "{what} holds the addition {} of {} with its own structure",
                    addition.id,
                    node.id,
                );
                if let Some(parent) = &addition.parent {
                    assert!(
                        !parent.is_empty() && valid_id(parent) && parent != &addition.id,
                        "{what} parents the addition {} of {} to {parent}",
                        addition.id,
                        node.id,
                    );
                }
                for component in self.schema.specs() {
                    if component.required {
                        assert!(
                            addition.has_component(component.name),
                            "{what} holds no {} for the addition {} of {}",
                            component.name,
                            addition.id,
                            node.id,
                        );
                    }
                }
                for (name, _) in addition.components() {
                    assert!(
                        self.schema.spec(name).is_some(),
                        "{what} holds the component {name} on the addition {} of {}, which its schema does not hold",
                        addition.id,
                        node.id,
                    );
                }
                let id = format!("{}_{}", node.id, addition.id);
                let index = self.node_index(&id).unwrap_or_else(|| {
                    panic!(
                        "{what} holds the addition {} of {}, which it does not materialize",
                        addition.id, node.id,
                    )
                });
                assert_eq!(
                    self.nodes[index].owner.as_deref(),
                    Some(node.id.as_str()),
                    "{what} holds the addition {id} of another node",
                );
                let parent = addition
                    .parent
                    .as_ref()
                    .map(|parent| format!("{}_{parent}", node.id))
                    .unwrap_or_else(|| node.id.clone());
                assert!(
                    self.node_index(&parent).is_some(),
                    "{what} parents the addition {id} to {parent}, which it does not hold",
                );
                assert_eq!(
                    self.nodes[index].parent.as_deref(),
                    Some(parent.as_str()),
                    "{what} materializes the addition {id} under another parent",
                );
            }
            let mut removals = HashSet::new();
            for removal in node.removals() {
                assert!(
                    !removal.is_empty() && valid_id(removal),
                    "{what} holds the removal {removal} of {}",
                    node.id,
                );
                assert!(
                    removals.insert(removal.as_str()),
                    "{what} holds the removal {removal} of {} twice",
                    node.id,
                );
                assert!(
                    !additions.contains(removal.as_str()),
                    "{what} removes the addition {removal} of {}",
                    node.id
                );
            }
        }
        for model in self.models.iter() {
            assert!(
                Folder::Asset.holds(&model.path),
                "{what} holds the model {} of path {} outside the assets folder",
                model.id,
                model.path.display(),
            );
        }
        for image in self.images.iter() {
            match &image.source {
                crate::image::ImageSource::Path(path) => assert!(
                    Folder::Asset.holds(path),
                    "{what} holds the image {} of path {} outside the assets folder",
                    image.id,
                    path.display(),
                ),
                crate::image::ImageSource::Model { model, .. } => assert!(
                    self.model_index(model.as_str()).is_some(),
                    "{what} holds the image {} from the model {model}, which it does not hold",
                    image.id,
                ),
            }
        }
        for sound in self.sounds.iter() {
            assert!(
                Folder::Asset.holds(&sound.path),
                "{what} holds the sound {} of path {} outside the assets folder",
                sound.id,
                sound.path.display(),
            );
        }
        for environment in self.environments.iter() {
            if let Some(path) = environment.panorama() {
                assert!(
                    Folder::Asset.holds(path),
                    "{what} holds the environment {} of path {} outside the assets folder",
                    environment.id,
                    path.display(),
                );
            }
        }
        for skin in self.skins.iter() {
            assert!(
                self.model_index(skin.asset.model_of().as_str()).is_some(),
                "{what} holds the skin {} from the model {}, which it does not hold",
                skin.id,
                skin.asset.model_of(),
            );
            let mut joints = HashSet::new();
            for joint in skin.asset.joints() {
                assert!(
                    self.node_index(joint.as_str()).is_some(),
                    "{what} holds the skin {} of joint {joint}, which it does not hold",
                    skin.id,
                );
                assert!(
                    joints.insert(joint.as_str()),
                    "{what} holds the skin {} of joint {joint} twice",
                    skin.id,
                );
            }
        }
        for animation in self.animations.iter() {
            assert!(
                self.model_index(animation.asset.model_of().as_str())
                    .is_some(),
                "{what} holds the animation {} from the model {}, which it does not hold",
                animation.id,
                animation.asset.model_of(),
            );
            for target in animation.asset.targets() {
                assert!(
                    self.node_index(target.as_str()).is_some(),
                    "{what} holds the animation {} of target {target}, which it does not hold",
                    animation.id,
                );
            }
        }
        for mesh in self.meshes.iter() {
            assert!(
                !mesh.asset.is_empty(),
                "{what} holds the mesh {} of no vertex",
                mesh.id,
            );
            if let Some((model, ..)) = mesh.asset.model_of() {
                assert!(
                    self.model_index(model.as_str()).is_some(),
                    "{what} holds the mesh {} from the model {model}, which it does not hold",
                    mesh.id,
                );
            }
        }
        for material in self.materials.iter() {
            assert!(
                (0.0..=1.0).contains(&material.asset.metallic),
                "{what} holds the material {} of metallic {}",
                material.id,
                material.asset.metallic,
            );
            assert!(
                material.asset.roughness > 0.0 && material.asset.roughness <= 1.0,
                "{what} holds the material {} of roughness {}",
                material.id,
                material.asset.roughness,
            );
            assert!(
                material.asset.normal_scale.is_finite() && material.asset.normal_scale >= 0.0,
                "{what} holds the material {} of normal scale {}",
                material.id,
                material.asset.normal_scale,
            );
            assert!(
                (0.0..=1.0).contains(&material.asset.occlusion_strength),
                "{what} holds the material {} of occlusion strength {}",
                material.id,
                material.asset.occlusion_strength,
            );
            for (name, color) in [
                ("base color", material.asset.base_color),
                ("emissive", material.asset.emissive),
            ] {
                assert!(
                    (0.0..=1.0).contains(&color.x)
                        && (0.0..=1.0).contains(&color.y)
                        && (0.0..=1.0).contains(&color.z),
                    "{what} holds the material {} of {name} {color:?} outside the unit cube",
                    material.id,
                );
            }
            for image in material.asset.images().into_iter().flatten() {
                assert!(
                    self.image_index(image.as_str()).is_some(),
                    "{what} samples the image {image}, which it does not hold",
                );
            }
        }
        for node in &self.nodes {
            for component in self.schema.specs() {
                if component.required {
                    assert!(
                        node.has_component(component.name),
                        "{what} holds no {} for node {}",
                        component.name,
                        node.id,
                    );
                }
            }
            for (name, component) in node.components() {
                assert!(
                    self.schema.spec(name).is_some(),
                    "{what} holds the component {name} on node {}, which its schema does not hold",
                    node.id,
                );
                component
                    .validate(self, &node.id)
                    .unwrap_or_else(|error| panic!("{what}: {error}"));
            }
        }
        self.check_audio_buses()
            .unwrap_or_else(|error| panic!("{what}: {error}"));
        for (at, node) in self.nodes.iter().enumerate() {
            if let Some(parent) = node.parent.as_deref() {
                let parent = self.node_index(parent).unwrap_or_else(|| {
                    panic!(
                        "{what} parents {} to {parent}, which it does not hold",
                        node.id,
                    )
                });
                assert!(
                    !self.is_ancestor(at, parent),
                    "{what} closes the parent chain of {} into a cycle",
                    node.id,
                );
            }
        }
    }
}

trait Identified {
    fn id(&self) -> &str;
}

impl Identified for ImageEntry {
    fn id(&self) -> &str {
        &self.id
    }
}

impl Identified for MeshEntry {
    fn id(&self) -> &str {
        &self.id
    }
}

impl Identified for SkinEntry {
    fn id(&self) -> &str {
        &self.id
    }
}

impl Identified for AnimationEntry {
    fn id(&self) -> &str {
        &self.id
    }
}

impl Identified for GraphEntry {
    fn id(&self) -> &str {
        &self.id
    }
}

impl Identified for MaterialEntry {
    fn id(&self) -> &str {
        &self.id
    }
}

impl Identified for SoundEntry {
    fn id(&self) -> &str {
        &self.id
    }
}

impl Identified for ModelEntry {
    fn id(&self) -> &str {
        &self.id
    }
}

impl Identified for Node {
    fn id(&self) -> &str {
        &self.id
    }
}

fn unique<'a, T: Identified + 'a>(entries: impl Iterator<Item = &'a T>, what: &str, kind: &str) {
    let mut ids = HashMap::new();
    for (at, entry) in entries.enumerate() {
        assert!(
            valid_id(entry.id()),
            "{what} holds the {kind} id {}, which holds no letter, digit, _ or -",
            entry.id(),
        );
        assert!(
            ids.insert(entry.id(), at).is_none(),
            "{what} holds two {kind} entries named {}",
            entry.id(),
        );
    }
}

fn shared<T: PartialEq>(left: &Arc<T>, right: &Arc<T>) -> bool {
    Arc::ptr_eq(left, right) || left == right
}

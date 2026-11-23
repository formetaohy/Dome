use crate::animation::AnimationEntry;
use crate::environment::EnvironmentEntry;
use crate::graph::GraphEntry;
use crate::image::ImageEntry;
use crate::material::MaterialEntry;
use crate::mesh::MeshEntry;
use crate::model::ModelEntry;
use crate::node::Node;
use crate::scene::{Owned, Scene};
use crate::skin::SkinEntry;
use crate::sound::SoundEntry;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Delta {
    steps: Vec<Step>,
}

impl Delta {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn apply(&self, scene: &mut Scene) {
        for step in &self.steps {
            step.apply(scene);
        }
    }

    pub fn revert(&self, scene: &mut Scene) {
        for step in self.steps.iter().rev() {
            step.revert(scene);
        }
    }

    pub(crate) fn push(&mut self, step: Step) {
        self.steps.push(step);
    }

    pub(crate) fn pop(&mut self) -> Option<Step> {
        self.steps.pop()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    Nodes(Splice<Arc<Node>>),
    Images(Splice<ImageEntry>),
    Sounds(Splice<SoundEntry>),
    Environments(Splice<EnvironmentEntry>),
    Meshes(Splice<MeshEntry>),
    Materials(Splice<MaterialEntry>),
    Models(Splice<ModelEntry>),
    Skins(Splice<SkinEntry>),
    Animations(Splice<AnimationEntry>),
    Graphs(Splice<GraphEntry>),
    Owners(Splice<Owned>),
    Instances {
        key: String,
        before: Option<BTreeSet<PathBuf>>,
        after: Option<BTreeSet<PathBuf>>,
    },
}

impl Step {
    fn apply(&self, scene: &mut Scene) {
        match self {
            Self::Nodes(splice) => {
                scene
                    .nodes
                    .splice(splice.at, splice.removed.len(), splice.inserted.clone());
            }
            Self::Images(splice) => splice.apply(&mut scene.images),
            Self::Sounds(splice) => splice.apply(&mut scene.sounds),
            Self::Environments(splice) => splice.apply(&mut scene.environments),
            Self::Meshes(splice) => splice.apply(&mut scene.meshes),
            Self::Materials(splice) => splice.apply(&mut scene.materials),
            Self::Models(splice) => splice.apply(&mut scene.models),
            Self::Skins(splice) => splice.apply(&mut scene.skins),
            Self::Animations(splice) => splice.apply(&mut scene.animations),
            Self::Graphs(splice) => splice.apply(&mut scene.graphs),
            Self::Owners(splice) => splice.apply(&mut scene.owners),
            Self::Instances { key, after, .. } => hold(scene, key, after),
        }
    }

    pub(crate) fn revert(&self, scene: &mut Scene) {
        match self {
            Self::Nodes(splice) => {
                scene
                    .nodes
                    .splice(splice.at, splice.inserted.len(), splice.removed.clone());
            }
            Self::Images(splice) => splice.revert(&mut scene.images),
            Self::Sounds(splice) => splice.revert(&mut scene.sounds),
            Self::Environments(splice) => splice.revert(&mut scene.environments),
            Self::Meshes(splice) => splice.revert(&mut scene.meshes),
            Self::Materials(splice) => splice.revert(&mut scene.materials),
            Self::Models(splice) => splice.revert(&mut scene.models),
            Self::Skins(splice) => splice.revert(&mut scene.skins),
            Self::Animations(splice) => splice.revert(&mut scene.animations),
            Self::Graphs(splice) => splice.revert(&mut scene.graphs),
            Self::Owners(splice) => splice.revert(&mut scene.owners),
            Self::Instances { key, before, .. } => hold(scene, key, before),
        }
    }
}

fn hold(scene: &mut Scene, key: &str, held: &Option<BTreeSet<PathBuf>>) {
    let instances = Arc::make_mut(&mut scene.instances);
    match held {
        Some(keys) => {
            instances.insert(String::from(key), keys.clone());
        }
        None => {
            instances.remove(key);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Splice<T> {
    pub(crate) at: usize,
    pub(crate) removed: Vec<T>,
    pub(crate) inserted: Vec<T>,
}

impl<T> Splice<T> {
    pub(crate) fn insert(at: usize, inserted: Vec<T>) -> Self {
        Self {
            at,
            removed: Vec::new(),
            inserted,
        }
    }

    pub(crate) fn remove(at: usize, removed: Vec<T>) -> Self {
        Self {
            at,
            removed,
            inserted: Vec::new(),
        }
    }
}

impl<T: Clone> Splice<T> {
    fn apply(&self, entries: &mut Arc<Vec<T>>) {
        Arc::make_mut(entries).splice(
            self.at..self.at + self.removed.len(),
            self.inserted.iter().cloned(),
        );
    }

    fn revert(&self, entries: &mut Arc<Vec<T>>) {
        Arc::make_mut(entries).splice(
            self.at..self.at + self.inserted.len(),
            self.removed.iter().cloned(),
        );
    }
}

pub(crate) trait Asset: Clone + PartialEq {
    fn id(&self) -> &str;

    fn entries(scene: &mut Scene) -> &mut Vec<Self>;
    fn held(scene: &Scene) -> &[Self];
    fn step(splice: Splice<Self>) -> Step;
}

macro_rules! assets {
    ($($entry:ty => $field:ident, $step:ident;)+) => {
        $(
            impl Asset for $entry {
                fn id(&self) -> &str {
                    &self.id
                }

                fn entries(scene: &mut Scene) -> &mut Vec<Self> {
                    Arc::make_mut(&mut scene.$field)
                }

                fn held(scene: &Scene) -> &[Self] {
                    &scene.$field
                }

                fn step(splice: Splice<Self>) -> Step {
                    Step::$step(splice)
                }
            }
        )+
    };
}

assets! {
    ImageEntry => images, Images;
    SoundEntry => sounds, Sounds;
    EnvironmentEntry => environments, Environments;
    MeshEntry => meshes, Meshes;
    MaterialEntry => materials, Materials;
    ModelEntry => models, Models;
    SkinEntry => skins, Skins;
    AnimationEntry => animations, Animations;
    GraphEntry => graphs, Graphs;
}

pub(crate) fn owned(
    kind: crate::schema::ReferenceKind,
    id: impl Into<String>,
    prefab: &Path,
    scope: Option<&str>,
) -> Owned {
    Owned {
        kind,
        id: id.into(),
        prefab: prefab.to_path_buf(),
        scope: scope.map(String::from),
    }
}

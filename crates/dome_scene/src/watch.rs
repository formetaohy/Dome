use crate::digest::Digest;
use crate::scene::Scene;
use crate::source::Sources;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SETTLE_POLLS: u32 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub scene: bool,
    pub model: bool,
    pub asset: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Scene,
    Model,
    Asset,
}

pub struct Watch {
    path: PathBuf,
    sources: Sources,
    files: Vec<(PathBuf, Kind, Option<Digest>)>,
    errors: Vec<String>,
    pending: Option<Change>,
    held: u32,
}

impl Watch {
    pub fn new(scene: &Path, root: &Scene) -> Self {
        let mut watch = Self {
            path: scene.to_path_buf(),
            sources: Sources::new(),
            files: Vec::new(),
            errors: Vec::new(),
            pending: None,
            held: 0,
        };
        watch.renew(root);
        watch
    }

    pub fn renew(&mut self, root: &Scene) {
        self.errors.clear();
        let mut desired = BTreeMap::new();
        desired.insert(self.path.clone(), Kind::Scene);
        for owners in root.instances.values() {
            for prefab in owners {
                desired.insert(root.root().join(prefab), Kind::Scene);
            }
        }
        for image in root.images() {
            if let Some(path) = image.source.path() {
                desired.insert(root.root().join(path), Kind::Asset);
            }
        }
        for sound in root.sounds() {
            desired.insert(root.root().join(&sound.path), Kind::Asset);
        }
        for environment in root.environments() {
            if let Some(path) = environment.panorama() {
                desired.insert(root.root().join(path), Kind::Asset);
            }
        }
        for model in root.models() {
            let path = root.root().join(&model.path);
            desired.insert(path.clone(), Kind::Model);
            let held = self.sources.dependencies(&path);
            if let Some(error) = held.error() {
                self.errors.push(String::from(error));
            }
            for dependency in held.sources().buffers.iter().chain(&held.sources().images) {
                desired.insert(dependency.clone(), Kind::Asset);
            }
        }
        let mut files = Vec::with_capacity(desired.len());
        for (path, kind) in desired {
            files.push((path.clone(), kind, self.sources.digest(&path).ok()));
        }
        self.files = files;
        self.pending = None;
        self.held = 0;
    }

    pub fn changed(&mut self) -> Option<Change> {
        let changed = self.poll();
        if changed.scene || changed.model || changed.asset {
            let pending = self.pending.get_or_insert(Change::default());
            pending.scene |= changed.scene;
            pending.model |= changed.model;
            pending.asset |= changed.asset;
            self.held = 0;
            return None;
        }
        self.pending.as_ref()?;
        self.held = self.held.saturating_add(1);
        if self.held < SETTLE_POLLS {
            return None;
        }
        self.held = 0;
        self.pending.take()
    }

    fn poll(&mut self) -> Change {
        let mut changed = Change::default();
        for (path, kind, digest) in &mut self.files {
            let current = self.sources.digest(path).ok();
            if *digest != current {
                *digest = current;
                match kind {
                    Kind::Scene => changed.scene = true,
                    Kind::Model => changed.model = true,
                    Kind::Asset => changed.asset = true,
                }
            }
        }
        changed
    }

    pub fn errors(&self) -> impl Iterator<Item = &str> + '_ {
        self.errors.iter().map(String::as_str)
    }
}

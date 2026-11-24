use crate::digest::Digest;
use crate::source::Sources;
use dome_scene::Scene;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SWEEP_FILES: usize = 32;
const SETTLE_SWEEPS: u32 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub scene: bool,
    pub model: bool,
    pub asset: bool,
}

impl Change {
    fn any(self) -> bool {
        self.scene || self.model || self.asset
    }

    fn absorb(&mut self, other: Self) {
        self.scene |= other.scene;
        self.model |= other.model;
        self.asset |= other.asset;
    }
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
    cursor: usize,
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
            cursor: 0,
            held: 0,
        };
        watch.renew(root);
        watch
    }

    pub fn renew(&mut self, root: &Scene) {
        self.errors.clear();
        let mut desired = BTreeMap::new();
        desired.insert(self.path.clone(), Kind::Scene);
        for owners in root.instances().values() {
            for prefab in owners {
                desired.insert(root.prefab(prefab), Kind::Scene);
            }
        }
        for image in root.images() {
            if let Some(path) = image.source.path() {
                desired.insert(root.asset(path), Kind::Asset);
            }
        }
        for sound in root.sounds() {
            desired.insert(root.asset(&sound.path), Kind::Asset);
        }
        for environment in root.environments() {
            if let Some(path) = environment.panorama() {
                desired.insert(root.asset(path), Kind::Asset);
            }
        }
        for model in root.models() {
            let path = root.asset(&model.path);
            desired.insert(path.clone(), Kind::Model);
            let held = self.sources.dependencies(&path);
            if let Some(error) = held.error() {
                self.errors.push(String::from(error));
            }
            for dependency in held.sources().buffers.iter().chain(&held.sources().images) {
                let inside = dependency
                    .strip_prefix(root.root())
                    .is_ok_and(|relative| dome_scene::Folder::Asset.holds(relative));
                if inside {
                    desired.insert(dependency.clone(), Kind::Asset);
                } else {
                    self.errors.push(format!(
                        "the model {} reads {} outside the assets folder",
                        model.id,
                        dependency.display(),
                    ));
                }
            }
        }
        let mut files = Vec::with_capacity(desired.len());
        for (path, kind) in desired {
            files.push((path.clone(), kind, self.sources.digest(&path).ok()));
        }
        self.files = files;
        self.pending = None;
        self.cursor = 0;
        self.held = 0;
    }

    pub fn changed(&mut self) -> Option<Change> {
        let (polled, swept) = self.poll();
        if polled.any() {
            self.pending.get_or_insert(Change::default()).absorb(polled);
            self.held = 0;
            return None;
        }
        self.pending.as_ref()?;
        if !swept {
            return None;
        }
        self.held += 1;
        if self.held < SETTLE_SWEEPS {
            return None;
        }
        self.held = 0;
        self.pending.take()
    }

    fn poll(&mut self) -> (Change, bool) {
        let mut changed = Change::default();
        let count = self.files.len();
        if count == 0 {
            return (changed, true);
        }
        let take = count.min(SWEEP_FILES);
        let cursor = self.cursor;
        let (files, sources) = (&mut self.files, &mut self.sources);
        for offset in 0..take {
            let (path, kind, digest) = &mut files[(cursor + offset) % count];
            let current = sources.digest(path).ok();
            if *digest == current {
                continue;
            }
            *digest = current;
            match kind {
                Kind::Scene => changed.scene = true,
                Kind::Model => changed.model = true,
                Kind::Asset => changed.asset = true,
            }
        }
        self.cursor = (cursor + take) % count;
        (changed, self.cursor == 0)
    }

    pub fn errors(&self) -> impl Iterator<Item = &str> + '_ {
        self.errors.iter().map(String::as_str)
    }
}

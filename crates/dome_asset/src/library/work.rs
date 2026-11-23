use super::prepare::panic_message;
use super::{AssetError, TextureKey};
use crate::digest::Digest;
use crate::source::Sources;
use dome_animation::Clip;
use dome_audio::Source;
use dome_image::ColorSpace;
use dome_render::TextureFormat;
use dome_render::environment::{Atmosphere, Panorama};
use dome_scene::Changed;
use dome_scene::EnvironmentEntry;
use dome_scene::ImageEntry;
use dome_scene::MaterialEntry;
use dome_scene::MeshEntry;
use dome_scene::ReferenceKind;
use dome_scene::Scene;
use dome_scene::SoundEntry;
use dome_scene::{SkinData, SkinEntry};
use std::any::Any;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

pub(super) struct Job {
    pub(super) scene: Scene,
    pub(super) fingerprints: Fingerprints,
    pub(super) textures: HashSet<TextureKey>,
    pub(super) sources: Sources,
}

#[derive(Clone, Default)]
pub(super) struct Fingerprints {
    pub(super) images: HashMap<String, Digest>,
    pub(super) materials: HashMap<String, Digest>,
    pub(super) meshes: HashMap<String, Digest>,
    pub(super) skins: HashMap<String, Digest>,
    pub(super) animations: HashMap<String, Digest>,
    pub(super) sounds: HashMap<String, Digest>,
    pub(super) environments: HashMap<String, Digest>,
}

pub(super) enum Outcome {
    Prepared(Box<Prepared>),
    Failed(String),
}

pub(super) struct Prepared {
    pub(super) applied: bool,
    pub(super) errors: Vec<AssetError>,
    pub(super) images: Vec<PreparedImage>,
    pub(super) materials: Vec<PreparedMaterial>,
    pub(super) meshes: Vec<PreparedMesh>,
    pub(super) skins: Vec<PreparedSkin>,
    pub(super) animations: Vec<PreparedAnimation>,
    pub(super) sounds: Vec<PreparedSound>,
    pub(super) environments: Vec<PreparedEnvironment>,
    pub(super) textures: HashMap<String, Arc<ImageMips>>,
    pub(super) changed_images: HashSet<String>,
    pub(super) removed: Removed,
    pub(super) sources: Sources,
    pub(super) files: BTreeSet<PathBuf>,
}

pub(super) struct PreparedImage {
    pub(super) entry: ImageEntry,
    pub(super) fingerprint: Digest,
    pub(super) mips: Arc<ImageMips>,
}

pub(super) struct PreparedMaterial {
    pub(super) entry: MaterialEntry,
    pub(super) fingerprint: Digest,
}

pub(super) struct PreparedMesh {
    pub(super) entry: MeshEntry,
    pub(super) fingerprint: Digest,
    pub(super) data: super::prepare::MeshData,
}

pub(super) struct PreparedSkin {
    pub(super) entry: SkinEntry,
    pub(super) fingerprint: Digest,
    pub(super) data: SkinData,
}

pub(super) struct PreparedAnimation {
    pub(super) entry: dome_scene::AnimationEntry,
    pub(super) fingerprint: Digest,
    pub(super) clip: Arc<Clip>,
}

pub(super) struct PreparedSound {
    pub(super) entry: SoundEntry,
    pub(super) fingerprint: Digest,
    pub(super) source: Source,
}

pub(super) struct PreparedEnvironment {
    pub(super) entry: EnvironmentEntry,
    pub(super) fingerprint: Digest,
    pub(super) data: EnvironmentData,
}

pub(super) enum EnvironmentData {
    Panorama(Panorama),
    Atmosphere(Atmosphere),
}

#[derive(Default)]
pub(super) struct Removed {
    pub(super) images: Vec<String>,
    pub(super) materials: Vec<String>,
    pub(super) meshes: Vec<String>,
    pub(super) skins: Vec<String>,
    pub(super) animations: Vec<String>,
    pub(super) sounds: Vec<String>,
    pub(super) environments: Vec<String>,
}

impl Removed {
    pub(super) fn gpu(&self) -> bool {
        !self.images.is_empty()
            || !self.materials.is_empty()
            || !self.meshes.is_empty()
            || !self.environments.is_empty()
    }
}

pub(super) struct ImageMips {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) srgb: Vec<Vec<u8>>,
    pub(super) linear: Option<Vec<Vec<u8>>>,
}

impl ImageMips {
    pub(super) fn levels(&self, color: ColorSpace) -> Option<&[Vec<u8>]> {
        match color {
            ColorSpace::Srgb => Some(&self.srgb),
            ColorSpace::Linear => self.linear.as_deref(),
        }
    }

    pub(super) fn bytes(&self, color: ColorSpace) -> u64 {
        self.levels(color).map_or(0, |levels| {
            levels.iter().map(|level| level.len() as u64).sum()
        })
    }
}

impl Prepared {
    pub(super) fn gpu(&self) -> bool {
        !self.images.is_empty()
            || !self.materials.is_empty()
            || !self.meshes.is_empty()
            || !self.environments.is_empty()
    }

    pub(super) fn changed(&self) -> Changed {
        let mut changed = Changed::none();
        for item in &self.images {
            changed.insert(ReferenceKind::Image, &item.entry.id);
        }
        for item in &self.materials {
            changed.insert(ReferenceKind::Material, &item.entry.id);
        }
        for item in &self.meshes {
            changed.insert(ReferenceKind::Mesh, &item.entry.id);
        }
        for item in &self.skins {
            changed.insert(ReferenceKind::Skin, &item.entry.id);
        }
        for item in &self.animations {
            changed.insert(ReferenceKind::Animation, &item.entry.id);
        }
        for item in &self.sounds {
            changed.insert(ReferenceKind::Sound, &item.entry.id);
        }
        for item in &self.environments {
            changed.insert(ReferenceKind::Environment, &item.entry.id);
        }
        for id in &self.removed.images {
            changed.insert(ReferenceKind::Image, id);
        }
        for id in &self.removed.materials {
            changed.insert(ReferenceKind::Material, id);
        }
        for id in &self.removed.meshes {
            changed.insert(ReferenceKind::Mesh, id);
        }
        for id in &self.removed.skins {
            changed.insert(ReferenceKind::Skin, id);
        }
        for id in &self.removed.animations {
            changed.insert(ReferenceKind::Animation, id);
        }
        for id in &self.removed.sounds {
            changed.insert(ReferenceKind::Sound, id);
        }
        for id in &self.removed.environments {
            changed.insert(ReferenceKind::Environment, id);
        }
        changed
    }
}

pub(super) fn format_of(color: ColorSpace) -> TextureFormat {
    match color {
        ColorSpace::Srgb => TextureFormat::Rgba8UnormSrgb,
        ColorSpace::Linear => TextureFormat::Rgba8Unorm,
    }
}

pub(super) fn image_modes(entry: &MaterialEntry) -> [(Option<&str>, ColorSpace); 5] {
    let asset = &entry.asset;
    [
        (
            asset.base_color_image.as_ref().map(|id| id.as_str()),
            ColorSpace::Srgb,
        ),
        (
            asset
                .metallic_roughness_image
                .as_ref()
                .map(|id| id.as_str()),
            ColorSpace::Linear,
        ),
        (
            asset.normal_image.as_ref().map(|id| id.as_str()),
            ColorSpace::Linear,
        ),
        (
            asset.occlusion_image.as_ref().map(|id| id.as_str()),
            ColorSpace::Linear,
        ),
        (
            asset.emissive_image.as_ref().map(|id| id.as_str()),
            ColorSpace::Srgb,
        ),
    ]
}

pub(super) struct Worker {
    jobs: Sender<Job>,
    results: Receiver<Outcome>,
}

impl Worker {
    pub(super) fn new() -> Self {
        let (jobs, held) = mpsc::channel::<Job>();
        let (outcome, results) = mpsc::channel::<Outcome>();
        thread::Builder::new()
            .name(String::from("dome assets"))
            .spawn(move || {
                while let Ok(job) = held.recv() {
                    let result = catch_unwind(AssertUnwindSafe(|| super::plan::plan(job)))
                        .map(|prepared| Outcome::Prepared(Box::new(prepared)))
                        .unwrap_or_else(|payload| Outcome::Failed(describe(payload.as_ref())));
                    if outcome.send(result).is_err() {
                        break;
                    }
                }
            })
            .expect("the asset worker starts");
        Self { jobs, results }
    }

    pub(super) fn start(&self, job: Job) {
        self.jobs
            .send(job)
            .expect("the asset worker accepts the scene it loads");
    }

    pub(super) fn outcome(&self) -> Option<Outcome> {
        match self.results.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Outcome::Failed(String::from(
                "the asset worker stops before it loads the scene",
            ))),
        }
    }
}

fn describe(payload: &(dyn Any + Send)) -> String {
    panic_message(payload)
}

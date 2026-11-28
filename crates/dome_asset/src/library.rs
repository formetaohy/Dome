mod commit;
mod placeholder;
mod plan;
mod prepare;
mod work;

use crate::digest::Digest;
use crate::source::Sources;
use dome_animation::{Clip, Interpolation, Property, Track};
use dome_audio::Source;
use dome_image::ColorSpace;
use dome_math::{Mat4, Vec3};
use dome_physics::TriangleMesh;
use dome_render::{
    Environment, Material, MaterialDesc, MaterialImage, Mesh, Renderer, Sampler, SamplerDesc,
    Texture, TextureFormat, TextureUsages,
};
use dome_scene::Changed;
use dome_scene::MaterialEntry;
use dome_scene::NodeId;
use dome_scene::Scene;
use dome_scene::SkinData;
use dome_schema::Assets;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use work::Fingerprints;
use work::ImageMips;
use work::Job;
use work::Outcome;
use work::Worker;

const WAIT_SLICE: Duration = Duration::from_millis(50);

pub struct Library {
    meshes: HashMap<String, MeshRecord>,
    materials: HashMap<String, MaterialRecord>,
    images: HashMap<String, ImageRecord>,
    sounds: HashMap<String, SoundRecord>,
    environments: HashMap<String, EnvironmentRecord>,
    skins: HashMap<String, SkinRecord>,
    animations: HashMap<String, AnimationRecord>,
    textures: HashMap<TextureKey, Texture>,
    samplers: HashMap<SamplerDesc, Sampler>,
    sources: Sources,
    worker: Worker,
    loading: bool,
    commit: Option<commit::Commit>,
    fallback: Option<Fallback>,
}

#[derive(Clone)]
pub struct ImageTexture {
    texture: Texture,
    sampler: SamplerDesc,
}

impl ImageTexture {
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    pub fn sampler(&self) -> SamplerDesc {
        self.sampler
    }
}

pub struct AssetError {
    pub kind: &'static str,
    pub id: String,
    pub message: String,
}

impl fmt::Display for AssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}: {}", self.kind, self.id, self.message)
    }
}

pub struct Refresh {
    pub applied: bool,
    pub changed: Changed,
    pub errors: Vec<AssetError>,
}

pub enum WaitOutcome {
    Ready(Refresh),
    Timeout,
    Cancelled,
}

impl Refresh {
    pub fn message(&self) -> String {
        self.errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    }
}

type TextureKey = (String, TextureFormat, ColorSpace);

struct MeshRecord {
    fingerprint: Digest,
    mesh: Mesh,
    positions: Arc<[[f32; 3]]>,
    shape: OnceLock<Arc<TriangleMesh>>,
}

struct MaterialRecord {
    fingerprint: Digest,
    material: Material,
}

struct ImageRecord {
    fingerprint: Digest,
    image: ImageTexture,
}

struct SoundRecord {
    fingerprint: Digest,
    source: Source,
}

struct EnvironmentRecord {
    fingerprint: Digest,
    capture: bool,
    environment: Environment,
}

struct SkinRecord {
    fingerprint: Digest,
    data: SkinData,
}

struct AnimationRecord {
    fingerprint: Digest,
    clip: Arc<Clip>,
}

struct Fallback {
    mesh: MeshRecord,
    material: Material,
    sound: Source,
    environment: Environment,
}

impl Default for Library {
    fn default() -> Self {
        Self::new()
    }
}

impl Library {
    pub fn new() -> Self {
        Self {
            meshes: HashMap::new(),
            materials: HashMap::new(),
            images: HashMap::new(),
            sounds: HashMap::new(),
            environments: HashMap::new(),
            skins: HashMap::new(),
            animations: HashMap::new(),
            textures: HashMap::new(),
            samplers: HashMap::new(),
            sources: Sources::new(),
            worker: Worker::new(),
            loading: false,
            commit: None,
            fallback: None,
        }
    }

    pub fn request(&mut self, scene: &Scene) -> bool {
        if self.loading || self.commit.is_some() {
            return false;
        }
        self.worker.start(Job {
            scene: scene.clone(),
            fingerprints: self.fingerprints(),
            textures: self.textures.keys().cloned().collect(),
            sources: self.sources.clone(),
        });
        self.loading = true;
        true
    }

    pub fn poll(&mut self, renderer: &mut Renderer, budget: u64) -> Option<Refresh> {
        self.poll_ready(renderer, budget, None)
    }

    fn poll_ready(
        &mut self,
        renderer: &mut Renderer,
        budget: u64,
        ready: Option<Outcome>,
    ) -> Option<Refresh> {
        if let Some(mut held) = self.commit.take() {
            match held.run(self, renderer, budget) {
                Some(refresh) => return Some(refresh),
                None => {
                    self.commit = Some(held);
                    return None;
                }
            }
        }
        if !self.loading {
            return None;
        }
        let outcome = match ready {
            Some(outcome) => outcome,
            None => self.worker.outcome()?,
        };
        self.loading = false;
        match outcome {
            Outcome::Prepared(prepared) => {
                let mut held = commit::Commit::new(*prepared);
                match held.run(self, renderer, budget) {
                    Some(refresh) => Some(refresh),
                    None => {
                        self.commit = Some(held);
                        None
                    }
                }
            }
            Outcome::Failed(message) => Some(Refresh {
                applied: false,
                changed: Changed::none(),
                errors: vec![AssetError {
                    kind: "asset",
                    id: String::from("worker"),
                    message,
                }],
            }),
        }
    }

    pub fn refresh(
        &mut self,
        scene: &Scene,
        renderer: &mut Renderer,
        budget: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> WaitOutcome {
        assert!(
            self.request(scene),
            "a library refreshes one scene at a time",
        );
        self.wait(renderer, budget, deadline, cancelled)
    }

    pub fn wait(
        &mut self,
        renderer: &mut Renderer,
        budget: u64,
        deadline: Instant,
        mut cancelled: impl FnMut() -> bool,
    ) -> WaitOutcome {
        assert!(self.loading(), "a library waits for the scene it loads");
        loop {
            if let Some(refresh) = self.poll(renderer, budget) {
                return WaitOutcome::Ready(refresh);
            }
            if self.commit.is_some() {
                continue;
            }
            if cancelled() {
                return WaitOutcome::Cancelled;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return WaitOutcome::Timeout;
            }
            if let Some(outcome) = self.worker.wait(remaining.min(WAIT_SLICE))
                && let Some(refresh) = self.poll_ready(renderer, budget, Some(outcome))
            {
                return WaitOutcome::Ready(refresh);
            }
        }
    }

    pub fn loading(&self) -> bool {
        self.loading || self.commit.is_some()
    }

    pub fn images(&self) -> impl Iterator<Item = (&str, &ImageTexture)> + '_ {
        self.images
            .iter()
            .map(|(id, image)| (id.as_str(), &image.image))
    }

    pub fn environments(&self) -> impl Iterator<Item = (&str, &Environment)> + '_ {
        self.environments
            .iter()
            .map(|(id, held)| (id.as_str(), &held.environment))
    }

    pub fn image_preview(&self, id: &str) -> Option<ImageTexture> {
        self.images.get(id).map(|held| held.image.clone())
    }

    pub fn environment_preview(&self, id: &str) -> Option<Environment> {
        self.environments
            .get(id)
            .map(|held| held.environment.clone())
    }

    fn ensure_fallback(&mut self, renderer: &mut Renderer) {
        if self.fallback.is_some() {
            return;
        }
        let sampler = self.sampler(renderer, environment_sampler_desc());
        let radiance = empty_texture(renderer, "missing.environment");
        let panorama = empty_texture(renderer, "missing.environment.panorama");
        let mut data = placeholder::mesh();
        let skin = data
            .skin
            .take()
            .expect("a placeholder mesh holds its skin weights");
        let mesh = renderer.skinned_mesh(&data.vertices, &skin);
        let mesh = MeshRecord {
            fingerprint: Digest::default(),
            mesh,
            positions: data.positions,
            shape: OnceLock::new(),
        };
        let material = renderer.material(MaterialDesc::new(Vec3::new(0.5, 0.5, 0.5)));
        self.fallback = Some(Fallback {
            mesh,
            material,
            sound: placeholder::sound(),
            environment: Environment::new(radiance, panorama, sampler, 1, [Vec3::ZERO; 9]),
        });
    }

    fn fallback(&self) -> &Fallback {
        self.fallback
            .as_ref()
            .expect("a committed library holds its fallback assets")
    }

    fn fingerprints(&self) -> Fingerprints {
        Fingerprints {
            images: self
                .images
                .iter()
                .map(|(id, held)| (id.clone(), held.fingerprint))
                .collect(),
            materials: self
                .materials
                .iter()
                .map(|(id, held)| (id.clone(), held.fingerprint))
                .collect(),
            meshes: self
                .meshes
                .iter()
                .map(|(id, held)| (id.clone(), held.fingerprint))
                .collect(),
            skins: self
                .skins
                .iter()
                .map(|(id, held)| (id.clone(), held.fingerprint))
                .collect(),
            animations: self
                .animations
                .iter()
                .map(|(id, held)| (id.clone(), held.fingerprint))
                .collect(),
            sounds: self
                .sounds
                .iter()
                .map(|(id, held)| (id.clone(), held.fingerprint))
                .collect(),
            environments: self
                .environments
                .iter()
                .map(|(id, held)| (id.clone(), held.fingerprint))
                .collect(),
        }
    }

    fn sampler(&mut self, renderer: &Renderer, desc: SamplerDesc) -> Sampler {
        if let Some(sampler) = self.samplers.get(&desc) {
            return sampler.clone();
        }
        let sampler = renderer.sampler(desc);
        self.samplers.insert(desc, sampler.clone());
        sampler
    }

    fn texture(
        &mut self,
        renderer: &Renderer,
        id: &str,
        format: TextureFormat,
        color: ColorSpace,
        textures: &HashMap<String, Arc<ImageMips>>,
    ) -> Texture {
        let key = (String::from(id), format, color);
        if let Some(texture) = self.textures.get(&key) {
            return texture.clone();
        }
        let mips = textures.get(id).unwrap_or_else(|| {
            panic!("the material samples the image {id}, which the library does not hold")
        });
        let levels = mips
            .levels(color)
            .unwrap_or_else(|| panic!("the image {id} holds no {color:?} levels for its material"));
        let held = levels.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let texture = renderer.texture(
            id,
            mips.width,
            mips.height,
            levels.len() as u32,
            format,
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        );
        renderer.write_texture(&texture, &held);
        self.textures.insert(key, texture.clone());
        texture
    }

    fn material_desc(
        &mut self,
        renderer: &Renderer,
        entry: &MaterialEntry,
        textures: &HashMap<String, Arc<ImageMips>>,
    ) -> MaterialDesc {
        let asset = &entry.asset;
        let mut desc = MaterialDesc::new(asset.base_color)
            .alpha(asset.alpha)
            .alpha_mode(asset.alpha_mode)
            .alpha_cutoff(asset.alpha_cutoff)
            .metallic(asset.metallic)
            .roughness(asset.roughness)
            .normal_scale(asset.normal_scale)
            .occlusion_strength(asset.occlusion_strength)
            .emissive(asset.emissive);
        if let Some(image) = &asset.base_color_image {
            desc = desc.base_color_image(self.material_image(
                renderer,
                image,
                TextureFormat::Rgba8UnormSrgb,
                ColorSpace::Srgb,
                textures,
            ));
        }
        if let Some(image) = &asset.metallic_roughness_image {
            desc = desc.metallic_roughness_image(self.material_image(
                renderer,
                image,
                TextureFormat::Rgba8Unorm,
                ColorSpace::Linear,
                textures,
            ));
        }
        if let Some(image) = &asset.normal_image {
            desc = desc.normal_image(self.material_image(
                renderer,
                image,
                TextureFormat::Rgba8Unorm,
                ColorSpace::Linear,
                textures,
            ));
        }
        if let Some(image) = &asset.occlusion_image {
            desc = desc.occlusion_image(self.material_image(
                renderer,
                image,
                TextureFormat::Rgba8Unorm,
                ColorSpace::Linear,
                textures,
            ));
        }
        if let Some(image) = &asset.emissive_image {
            desc = desc.emissive_image(self.material_image(
                renderer,
                image,
                TextureFormat::Rgba8UnormSrgb,
                ColorSpace::Srgb,
                textures,
            ));
        }
        desc
    }

    fn material_image(
        &mut self,
        renderer: &Renderer,
        id: &str,
        format: TextureFormat,
        color: ColorSpace,
        textures: &HashMap<String, Arc<ImageMips>>,
    ) -> MaterialImage {
        let sampler = self
            .images
            .get(id)
            .map(|held| held.image.sampler)
            .unwrap_or_else(|| panic!("the scene samples the image {id}, which it does not hold"));
        let sampler = self.sampler(renderer, sampler);
        let texture = self.texture(renderer, id, format, color, textures);
        MaterialImage::new(texture, sampler)
    }
}

fn environment_sampler_desc() -> SamplerDesc {
    SamplerDesc {
        min_filter: dome_render::FilterMode::Linear,
        mag_filter: dome_render::FilterMode::Linear,
        mipmap_filter: dome_render::FilterMode::Linear,
        address_mode_u: dome_render::AddressMode::Repeat,
        address_mode_v: dome_render::AddressMode::ClampToEdge,
        compare: None,
    }
}

fn empty_texture(renderer: &Renderer, label: &str) -> Texture {
    const EMPTY_TEXEL: &[u8] = &[0, 0, 0, 0, 0, 0, 0x00, 0x3C];
    let texture = renderer.texture(
        label,
        1,
        1,
        1,
        TextureFormat::Rgba16Float,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    renderer.write_texture(&texture, &[EMPTY_TEXEL]);
    texture
}

impl Assets for Library {
    fn mesh(&self, id: &str) -> Mesh {
        self.meshes
            .get(id)
            .map_or(self.fallback().mesh.mesh, |held| held.mesh)
    }

    fn material(&self, id: &str) -> Material {
        self.materials
            .get(id)
            .map_or(self.fallback().material, |held| held.material)
    }

    fn skin(&self, id: &str, node: &str) -> SkinData {
        if let Some(held) = self.skins.get(id) {
            return held.data.clone();
        }
        SkinData::new(vec![NodeId::new(node)], Arc::from([Mat4::IDENTITY]))
    }

    fn mesh_shape(&self, id: &str) -> Arc<TriangleMesh> {
        let fallback = &self.fallback().mesh;
        let held = self.meshes.get(id).unwrap_or(fallback);
        Arc::clone(
            held.shape
                .get_or_init(|| Arc::new(TriangleMesh::from_positions(&held.positions))),
        )
    }

    fn animation(&self, id: &str, targets: &[NodeId]) -> Arc<Clip> {
        if let Some(held) = self.animations.get(id) {
            return Arc::clone(&held.clip);
        }
        let tracks = targets
            .iter()
            .map(|target| {
                Track::new(
                    target.as_str(),
                    Property::Translation,
                    Interpolation::Linear,
                    vec![0.0],
                    vec![0.0, 0.0, 0.0],
                )
                .expect("a missing animation holds a valid placeholder track")
            })
            .collect();
        Arc::new(Clip::new(tracks))
    }

    fn environment(&self, id: &str) -> Environment {
        self.environments.get(id).map_or_else(
            || self.fallback().environment.clone(),
            |held| held.environment.clone(),
        )
    }

    fn capture(&self, id: &str) -> Option<dome_render::CaptureProbe> {
        self.environments
            .get(id)
            .filter(|held| held.capture)
            .map(|_| dome_render::CaptureProbe::default())
    }

    fn sound(&self, id: &str) -> Source {
        self.sounds
            .get(id)
            .map_or_else(|| self.fallback().sound.clone(), |held| held.source.clone())
    }
}

impl Library {
    pub fn build(scene: &Scene, renderer: &mut Renderer) -> Library {
        let mut library = Library::new();
        let outcome = library.refresh(
            scene,
            renderer,
            u64::MAX,
            Instant::now() + Duration::from_secs(30),
            || false,
        );
        let refresh = match outcome {
            WaitOutcome::Ready(refresh) => refresh,
            WaitOutcome::Timeout => panic!("the library loads its scene within its deadline"),
            WaitOutcome::Cancelled => unreachable!("an uncancelled library load is not cancelled"),
        };
        assert!(
            refresh.applied,
            "the library adopts the assets of its scene: {}",
            refresh.message(),
        );
        assert!(
            refresh.errors.is_empty(),
            "the scene builds its assets: {}",
            refresh.message(),
        );
        library
    }
}

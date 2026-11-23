mod commit;
mod plan;
mod prepare;
mod work;

use crate::digest::Digest;
use crate::source::Sources;
use dome_animation::Clip;
use dome_audio::Source;
use dome_image::ColorSpace;
use dome_physics::TriangleMesh;
use dome_render::{
    Environment, Material, MaterialDesc, MaterialImage, Mesh, Renderer, Sampler, SamplerDesc,
    Texture, TextureFormat, TextureUsages,
};
use dome_scene::Changed;
use dome_scene::MaterialEntry;
use dome_scene::Scene;
use dome_scene::SkinData;
use dome_schema::Assets;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::OnceLock;
use work::Fingerprints;
use work::ImageMips;
use work::Job;
use work::Outcome;
use work::Worker;

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
    retired_meshes: Vec<Mesh>,
    retired_materials: Vec<Material>,
    retired_images: Vec<ImageTexture>,
    synchronous: bool,
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
    pub fatal: bool,
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
            retired_meshes: Vec::new(),
            retired_materials: Vec::new(),
            retired_images: Vec::new(),
            synchronous: false,
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
        let outcome = self.worker.outcome()?;
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
                    fatal: true,
                }],
            }),
        }
    }

    pub fn refresh(&mut self, scene: &Scene, renderer: &mut Renderer) -> Refresh {
        assert!(
            self.request(scene),
            "a library refreshes one scene at a time",
        );
        self.synchronous = true;
        loop {
            if let Some(refresh) = self.poll(renderer, u64::MAX) {
                self.synchronous = false;
                return refresh;
            }
            std::thread::sleep(std::time::Duration::from_micros(100));
        }
    }

    pub fn release(&mut self, renderer: &mut Renderer) {
        if self.retired_meshes.is_empty()
            && self.retired_materials.is_empty()
            && self.retired_images.is_empty()
        {
            return;
        }
        renderer.idle();
        for mesh in self.retired_meshes.drain(..) {
            renderer.remove_mesh(mesh);
        }
        for material in self.retired_materials.drain(..) {
            renderer.remove_material(material);
        }
        self.retired_images.clear();
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
    ) -> Result<Texture, String> {
        let key = (String::from(id), format, color);
        if let Some(texture) = self.textures.get(&key) {
            return Ok(texture.clone());
        }
        let mips = textures.get(id).ok_or_else(|| {
            format!("the material samples the image {id}, whose pixels the library holds none")
        })?;
        let levels = mips
            .levels(color)
            .ok_or_else(|| format!("the image {id} holds no {color:?} levels for its material"))?;
        let held = levels.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let texture = renderer.texture(
            id,
            mips.width,
            mips.height,
            levels.len() as u32,
            format,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        renderer.write_texture(&texture, &held);
        self.textures.insert(key, texture.clone());
        Ok(texture)
    }

    fn material_desc(
        &mut self,
        renderer: &Renderer,
        entry: &MaterialEntry,
        textures: &HashMap<String, Arc<ImageMips>>,
    ) -> Result<MaterialDesc, String> {
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
            )?);
        }
        if let Some(image) = &asset.metallic_roughness_image {
            desc = desc.metallic_roughness_image(self.material_image(
                renderer,
                image,
                TextureFormat::Rgba8Unorm,
                ColorSpace::Linear,
                textures,
            )?);
        }
        if let Some(image) = &asset.normal_image {
            desc = desc.normal_image(self.material_image(
                renderer,
                image,
                TextureFormat::Rgba8Unorm,
                ColorSpace::Linear,
                textures,
            )?);
        }
        if let Some(image) = &asset.occlusion_image {
            desc = desc.occlusion_image(self.material_image(
                renderer,
                image,
                TextureFormat::Rgba8Unorm,
                ColorSpace::Linear,
                textures,
            )?);
        }
        if let Some(image) = &asset.emissive_image {
            desc = desc.emissive_image(self.material_image(
                renderer,
                image,
                TextureFormat::Rgba8UnormSrgb,
                ColorSpace::Srgb,
                textures,
            )?);
        }
        Ok(desc)
    }

    fn material_image(
        &mut self,
        renderer: &Renderer,
        id: &str,
        format: TextureFormat,
        color: ColorSpace,
        textures: &HashMap<String, Arc<ImageMips>>,
    ) -> Result<MaterialImage, String> {
        let sampler = self
            .images
            .get(id)
            .map(|held| held.image.sampler)
            .ok_or_else(|| format!("the scene samples the image {id}, which it does not hold"))?;
        let sampler = self.sampler(renderer, sampler);
        let texture = self.texture(renderer, id, format, color, textures)?;
        Ok(MaterialImage::new(texture, sampler))
    }
}

impl Assets for Library {
    fn mesh(&self, id: &str) -> Mesh {
        self.meshes
            .get(id)
            .unwrap_or_else(|| panic!("the scene renders the mesh {id}, which it does not hold"))
            .mesh
    }

    fn material(&self, id: &str) -> Material {
        self.materials
            .get(id)
            .unwrap_or_else(|| {
                panic!("the scene renders the material {id}, which it does not hold")
            })
            .material
    }

    fn skin(&self, id: &str) -> SkinData {
        self.skins
            .get(id)
            .unwrap_or_else(|| panic!("the scene deforms the skin {id}, which it does not hold"))
            .data
            .clone()
    }

    fn mesh_shape(&self, id: &str) -> Arc<TriangleMesh> {
        let held = self
            .meshes
            .get(id)
            .unwrap_or_else(|| panic!("the scene collides the mesh {id}, which it does not hold"));
        Arc::clone(
            held.shape
                .get_or_init(|| Arc::new(TriangleMesh::from_positions(&held.positions))),
        )
    }

    fn animation(&self, id: &str) -> Arc<Clip> {
        Arc::clone(
            &self
                .animations
                .get(id)
                .unwrap_or_else(|| {
                    panic!("the scene plays the animation {id}, which it does not hold")
                })
                .clip,
        )
    }

    fn environment(&self, id: &str) -> Environment {
        self.environments
            .get(id)
            .unwrap_or_else(|| {
                panic!("the scene lights with the environment {id}, which it does not hold")
            })
            .environment
            .clone()
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
            .unwrap_or_else(|| panic!("the scene plays the sound {id}, which it does not hold"))
            .source
            .clone()
    }
}

impl Library {
    pub fn build(scene: &Scene, renderer: &mut Renderer) -> Library {
        let mut library = Library::new();
        let refresh = library.refresh(scene, renderer);
        assert!(
            refresh.applied,
            "the scene builds its assets: {}",
            refresh.message(),
        );
        library
    }
}

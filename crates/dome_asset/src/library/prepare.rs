use super::work::EnvironmentData;
use crate::digest::{Digest, DigestBuilder};
use crate::source::Sources;
use dome_animation::{Clip, Track};
use dome_audio::Sound;
use dome_gltf::Model;
use dome_image::Image;
use dome_image::Radiance;
use dome_math::Mat4;
use dome_render::environment::Panorama;
use dome_render::{AddressMode, FilterMode, SamplerDesc, SkinVertexRecord, Vertex};
use dome_scene::AnimationEntry;
use dome_scene::MaterialEntry;
use dome_scene::Scene;
use dome_scene::SoundEntry;
use dome_scene::{EnvironmentEntry, EnvironmentSource};
use dome_scene::{Geometry, MeshEntry};
use dome_scene::{ImageEntry, ImageSource};
use dome_scene::{SkinData, SkinEntry};
use std::any::Any;
use std::collections::{BTreeSet, HashMap};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;

pub(super) struct ImageBytes {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) rgba: Vec<u8>,
}

pub(super) struct MeshData {
    pub(super) vertices: Vec<Vertex>,
    pub(super) skin: Option<Vec<SkinVertexRecord>>,
    pub(super) positions: Arc<[[f32; 3]]>,
}

pub(super) struct Models<'a> {
    scene: &'a Scene,
    loaded: HashMap<String, Result<Model, String>>,
    digests: HashMap<(String, Scope), Result<Digest, String>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Scope {
    Geometry,
    Image,
}

pub(super) struct Prepare<'scene> {
    pub(super) images: ImageCache,
    pub(super) models: Models<'scene>,
    pub(super) sources: Sources,
}

pub(super) type ImageCache = HashMap<String, Arc<ImageBytes>>;

impl<'scene> Prepare<'scene> {
    pub(super) fn new(scene: &'scene Scene, sources: Sources) -> Self {
        Self {
            images: ImageCache::new(),
            models: Models::new(scene),
            sources,
        }
    }
}

impl Models<'_> {
    fn new(scene: &Scene) -> Models<'_> {
        Models {
            scene,
            loaded: HashMap::new(),
            digests: HashMap::new(),
        }
    }

    pub(super) fn digest(
        &mut self,
        id: &str,
        scope: Scope,
        sources: &mut Sources,
        files: &mut BTreeSet<PathBuf>,
    ) -> Result<Digest, String> {
        let key = (String::from(id), scope);
        if let Some(held) = self.digests.get(&key) {
            return held.clone();
        }
        let held = self.digest_uncached(id, scope, sources, files);
        self.digests.insert(key, held.clone());
        held
    }

    fn digest_uncached(
        &mut self,
        id: &str,
        scope: Scope,
        sources: &mut Sources,
        files: &mut BTreeSet<PathBuf>,
    ) -> Result<Digest, String> {
        let entry = self
            .scene
            .model(id)
            .ok_or_else(|| format!("the scene holds no model {id}"))?;
        let path = self.scene.asset(&entry.path);
        files.insert(path.clone());
        let file = sources.digest(&path)?;
        let external = sources.dependencies(&path);
        let mut digest = Digest::builder();
        digest
            .text("model")
            .text(match scope {
                Scope::Geometry => "geometry",
                Scope::Image => "image",
            })
            .text(id)
            .text(&path.to_string_lossy())
            .digest(file);
        let dependencies = external.sources();
        for source in &dependencies.buffers {
            asset_dependency(self.scene, id, "buffer", source)?;
            files.insert(source.clone());
            digest.digest(sources.digest(source)?);
        }
        if scope == Scope::Image {
            for source in &dependencies.images {
                asset_dependency(self.scene, id, "image", source)?;
                files.insert(source.clone());
                digest.digest(sources.digest(source)?);
            }
        }
        Ok(digest.finish())
    }

    pub(super) fn load(&mut self, id: &str, sources: &mut Sources) -> Result<&Model, String> {
        if !self.loaded.contains_key(id) {
            let held = self.load_uncached(id, sources);
            self.loaded.insert(String::from(id), held);
        }
        match self
            .loaded
            .get(id)
            .expect("a loaded model keeps the slot it was asked for")
        {
            Ok(model) => Ok(model),
            Err(error) => Err(error.clone()),
        }
    }

    fn load_uncached(&self, id: &str, sources: &mut Sources) -> Result<Model, String> {
        let entry = self
            .scene
            .model(id)
            .ok_or_else(|| format!("the scene holds no model {id}"))?;
        let path = self.scene.asset(&entry.path);
        let (_, bytes) = sources.read(&path)?;
        catch_unwind(AssertUnwindSafe(|| Model::parse(&bytes, &path)))
            .map_err(|payload| panic_message(payload.as_ref()))
    }
}

pub(super) fn asset_dependency(
    scene: &Scene,
    id: &str,
    kind: &str,
    path: &std::path::Path,
) -> Result<(), String> {
    let inside = path
        .strip_prefix(scene.root())
        .is_ok_and(|relative| dome_scene::Folder::Asset.holds(relative));
    if inside {
        Ok(())
    } else {
        Err(format!(
            "the model {id} reads its {kind} {} outside the assets folder",
            path.display(),
        ))
    }
}

pub(super) fn validate_material(entry: &MaterialEntry) -> Result<(), String> {
    let asset = &entry.asset;
    let unit = |value: f32| (0.0..=1.0).contains(&value);
    if !unit(asset.base_color.x) || !unit(asset.base_color.y) || !unit(asset.base_color.z) {
        return Err(format!(
            "the material {} holds the base color {:?} outside the unit cube",
            entry.id, asset.base_color,
        ));
    }
    if !unit(asset.alpha) {
        return Err(format!(
            "the material {} holds the alpha {} outside the unit range",
            entry.id, asset.alpha,
        ));
    }
    if !unit(asset.alpha_cutoff) {
        return Err(format!(
            "the material {} holds the alpha cutoff {} outside the unit range",
            entry.id, asset.alpha_cutoff,
        ));
    }
    if !unit(asset.metallic) {
        return Err(format!(
            "the material {} holds the metallic {} outside the unit range",
            entry.id, asset.metallic,
        ));
    }
    if !(asset.roughness > 0.0 && asset.roughness <= 1.0) {
        return Err(format!(
            "the material {} holds the roughness {} outside the scatter range",
            entry.id, asset.roughness,
        ));
    }
    if !(asset.normal_scale.is_finite() && asset.normal_scale >= 0.0) {
        return Err(format!(
            "the material {} holds the normal scale {} outside the scale range",
            entry.id, asset.normal_scale,
        ));
    }
    if !unit(asset.occlusion_strength) {
        return Err(format!(
            "the material {} holds the occlusion strength {} outside the unit range",
            entry.id, asset.occlusion_strength,
        ));
    }
    if !asset.emissive.x.is_finite()
        || !asset.emissive.y.is_finite()
        || !asset.emissive.z.is_finite()
        || asset.emissive.x < 0.0
        || asset.emissive.y < 0.0
        || asset.emissive.z < 0.0
    {
        return Err(format!(
            "the material {} emits the light {:?}",
            entry.id, asset.emissive,
        ));
    }
    Ok(())
}

pub(super) fn image_source(
    entry: &ImageEntry,
    scene: &Scene,
    sources: &mut Sources,
    files: &mut BTreeSet<PathBuf>,
    models: &mut Models<'_>,
) -> Result<Digest, String> {
    match &entry.source {
        ImageSource::Path(path) => {
            let file = scene.asset(path);
            files.insert(file.clone());
            sources.digest(&file)
        }
        ImageSource::Model { model, .. } => {
            models.digest(model.as_str(), Scope::Image, sources, files)
        }
    }
}

pub(super) fn image_data_of(
    scene: &Scene,
    id: &str,
    images: &mut ImageCache,
    models: &mut Models<'_>,
    sources: &mut Sources,
) -> Result<Arc<ImageBytes>, String> {
    if let Some(held) = images.get(id) {
        return Ok(Arc::clone(held));
    }
    let entry = scene
        .image(id)
        .ok_or_else(|| format!("the scene holds no image {id}"))?;
    let bytes = match &entry.source {
        ImageSource::Path(path) => {
            let file = scene.asset(path);
            let (_, bytes) = sources.read(&file)?;
            let decoded = catch_unwind(AssertUnwindSafe(|| {
                Image::decode(&bytes, &format!("the image {id}"))
            }))
            .map_err(|payload| panic_message(payload.as_ref()))?;
            ImageBytes {
                width: decoded.width(),
                height: decoded.height(),
                rgba: decoded.rgba().to_vec(),
            }
        }
        ImageSource::Model { model, image } => {
            let held = models.load(model.as_str(), sources)?;
            let image = held.images.get(*image as usize).ok_or_else(|| {
                format!("the image {id} reads the image {image} of the model {model}")
            })?;
            ImageBytes {
                width: image.width(),
                height: image.height(),
                rgba: image.rgba().to_vec(),
            }
        }
    };
    if bytes.width == 0 || bytes.height == 0 {
        return Err(format!("the image {id} spans no texel"));
    }
    let bytes = Arc::new(bytes);
    images.insert(String::from(id), Arc::clone(&bytes));
    Ok(bytes)
}

pub(super) fn mesh_data(
    entry: &MeshEntry,
    models: &mut Models<'_>,
    sources: &mut Sources,
) -> Result<MeshData, String> {
    let (vertices, skin) = match entry.asset.geometry() {
        Geometry::Shape(shape) => (shape.vertices(), None),
        Geometry::Vertices(vertices) => (vertices.to_vec(), None),
        Geometry::Model {
            model,
            mesh,
            primitive,
        } => {
            let held = models.load(model.as_str(), sources)?;
            let mesh = held.meshes.get(*mesh as usize).ok_or_else(|| {
                format!(
                    "the mesh {} reads the mesh {mesh} of the model {model}",
                    entry.id
                )
            })?;
            let primitive = mesh.primitives.get(*primitive as usize).ok_or_else(|| {
                format!(
                    "the mesh {} reads the primitive {primitive} of the model {model}",
                    entry.id,
                )
            })?;
            (
                primitive
                    .vertices
                    .iter()
                    .map(|vertex| crate::convert::vertex(*vertex))
                    .collect::<Vec<_>>(),
                primitive.skin.as_ref().map(|skin| {
                    skin.iter()
                        .map(|vertex| crate::convert::skin_vertex(*vertex))
                        .collect::<Vec<_>>()
                }),
            )
        }
    };
    if vertices.is_empty() {
        return Err(format!("the mesh {} holds no vertex", entry.id));
    }
    Ok(MeshData {
        positions: positions(&vertices).into(),
        vertices,
        skin,
    })
}

pub(super) fn skin_data(entry: &SkinEntry, model: &Model) -> Result<SkinData, String> {
    let held = model
        .skins
        .get(entry.asset.index() as usize)
        .ok_or_else(|| {
            format!(
                "the skin {} reads the skin {} of the model {}",
                entry.id,
                entry.asset.index(),
                entry.asset.model_of(),
            )
        })?;
    if held.inverse_binds.len() != entry.asset.joints().len() {
        return Err(format!(
            "the skin {} maps {} joints onto a model skin of {}",
            entry.id,
            entry.asset.joints().len(),
            held.inverse_binds.len(),
        ));
    }
    let inverse_binds: Arc<[Mat4]> = Arc::from(held.inverse_binds.clone());
    Ok(SkinData::new(entry.asset.joints().to_vec(), inverse_binds))
}

pub(super) fn animation_data(entry: &AnimationEntry, model: &Model) -> Result<Arc<Clip>, String> {
    let held = model
        .animations
        .get(entry.asset.index() as usize)
        .ok_or_else(|| {
            format!(
                "the animation {} reads the animation {} of the model {}",
                entry.id,
                entry.asset.index(),
                entry.asset.model_of(),
            )
        })?;
    if held.channels.len() != entry.asset.targets().len() {
        return Err(format!(
            "the animation {} maps {} channels to {} targets",
            entry.id,
            held.channels.len(),
            entry.asset.targets().len(),
        ));
    }
    let tracks = catch_unwind(AssertUnwindSafe(|| {
        held.channels
            .iter()
            .zip(entry.asset.targets())
            .map(|(channel, target)| {
                Track::new(
                    target.as_str(),
                    channel.property,
                    channel.interpolation,
                    channel.times.clone(),
                    channel.values.clone(),
                )
            })
            .collect::<Vec<_>>()
    }))
    .map_err(|payload| panic_message(payload.as_ref()))?;
    if tracks.is_empty() {
        return Err(format!("the animation {} holds no track", entry.id));
    }
    Ok(Arc::new(Clip::new(tracks)))
}

pub(super) fn sound_data(
    scene: &Scene,
    entry: &SoundEntry,
    sources: &mut Sources,
) -> Result<dome_audio::Source, String> {
    let file = scene.asset(&entry.path);
    if entry.stream {
        return catch_unwind(AssertUnwindSafe(|| dome_audio::Source::stream(file)))
            .map_err(|payload| panic_message(payload.as_ref()))?;
    }
    let (_, bytes) = sources.read(&file)?;
    let sound = catch_unwind(AssertUnwindSafe(|| {
        Sound::decode(&bytes, &format!("the sound {}", file.display()))
    }))
    .map_err(|payload| panic_message(payload.as_ref()))?;
    Ok(dome_audio::Source::decoded(Arc::new(sound)))
}

pub(super) fn environment_data(
    scene: &Scene,
    entry: &EnvironmentEntry,
    sources: &mut Sources,
) -> Result<EnvironmentData, String> {
    match &entry.source {
        EnvironmentSource::Panorama(path) => {
            let file = scene.asset(path);
            let (_, bytes) = sources.read(&file)?;
            let radiance = catch_unwind(AssertUnwindSafe(|| {
                Radiance::decode(&bytes, &format!("the environment {}", file.display()))
            }))
            .map_err(|payload| panic_message(payload.as_ref()))?;
            Ok(EnvironmentData::Panorama(Panorama::new(
                radiance.width(),
                radiance.height(),
                radiance.rgb(),
            )))
        }
        EnvironmentSource::Atmosphere(atmosphere) => Ok(EnvironmentData::Atmosphere(*atmosphere)),
        EnvironmentSource::Capture => Ok(EnvironmentData::Empty),
    }
}

pub(super) fn atmosphere_fingerprint(atmosphere: &dome_render::environment::Atmosphere) -> Digest {
    let mut fingerprint = Digest::builder();
    fingerprint
        .text("atmosphere")
        .number(u64::from(dome_render::environment::MODEL_VERSION))
        .number(atmosphere.sun_elevation.to_bits() as u64)
        .number(atmosphere.sun_azimuth.to_bits() as u64)
        .number(atmosphere.sun_intensity.to_bits() as u64)
        .number(atmosphere.rayleigh.to_bits() as u64)
        .number(atmosphere.mie.to_bits() as u64)
        .number(atmosphere.mie_anisotropy.to_bits() as u64)
        .number(atmosphere.ozone.to_bits() as u64)
        .number(atmosphere.ground_albedo.x.to_bits() as u64)
        .number(atmosphere.ground_albedo.y.to_bits() as u64)
        .number(atmosphere.ground_albedo.z.to_bits() as u64)
        .number(atmosphere.altitude.to_bits() as u64);
    fingerprint.finish()
}

pub(super) fn environment_fingerprint(entry: &EnvironmentEntry, digest: Option<Digest>) -> Digest {
    let mut fingerprint = Digest::builder();
    fingerprint.text("environment").text(&entry.id);
    if let Some(path) = entry.panorama() {
        fingerprint.text(&path.to_string_lossy());
    }
    match digest {
        Some(digest) => fingerprint.digest(digest),
        None => fingerprint.number(0),
    };
    fingerprint.finish()
}

fn positions(vertices: &[Vertex]) -> Vec<[f32; 3]> {
    vertices.iter().map(|vertex| vertex.position).collect()
}

pub(super) fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    if let Some(text) = payload.downcast_ref::<&str>() {
        return String::from(*text);
    }
    String::from("the asset fails")
}

pub(super) fn image_fingerprint(entry: &ImageEntry, source: Option<Digest>) -> Digest {
    let mut digest = Digest::builder();
    digest.text("image").text(&entry.id);
    source_digest(&mut digest, &entry.source);
    match source {
        Some(source) => digest.digest(source),
        None => digest.number(0),
    };
    sampler_digest(&mut digest, entry.sampler);
    digest.finish()
}

fn source_digest(digest: &mut DigestBuilder, source: &ImageSource) {
    match source {
        ImageSource::Path(path) => {
            digest.text("path").text(&path.to_string_lossy());
        }
        ImageSource::Model { model, image } => {
            digest
                .text("model")
                .text(model.as_str())
                .number(u64::from(*image));
        }
    }
}

pub(super) fn material_fingerprint(entry: &MaterialEntry, images: &[Digest]) -> Digest {
    let asset = &entry.asset;
    let mut digest = Digest::builder();
    digest.text("material").text(&entry.id);
    digest
        .number(asset.base_color.x.to_bits() as u64)
        .number(asset.base_color.y.to_bits() as u64)
        .number(asset.base_color.z.to_bits() as u64)
        .number(asset.alpha.to_bits() as u64)
        .text(asset.alpha_mode.name())
        .number(asset.alpha_cutoff.to_bits() as u64)
        .number(asset.metallic.to_bits() as u64)
        .number(asset.roughness.to_bits() as u64)
        .number(asset.normal_scale.to_bits() as u64)
        .number(asset.occlusion_strength.to_bits() as u64)
        .number(asset.emissive.x.to_bits() as u64)
        .number(asset.emissive.y.to_bits() as u64)
        .number(asset.emissive.z.to_bits() as u64);
    for image in asset.images() {
        match image {
            Some(image) => digest.text(image.as_str()),
            None => digest.number(0),
        };
    }
    for image in images {
        digest.digest(*image);
    }
    digest.finish()
}

pub(super) fn mesh_fingerprint(entry: &MeshEntry, model: Option<Digest>) -> Digest {
    let mut digest = Digest::builder();
    digest.text("mesh").text(&entry.id);
    match entry.asset.geometry() {
        Geometry::Shape(shape) => {
            digest.text("shape").text(shape.name());
        }
        Geometry::Vertices(vertices) => {
            digest.text("vertices").number(vertices.len() as u64);
            for vertex in vertices.iter() {
                vertex_digest(&mut digest, vertex);
            }
        }
        Geometry::Model {
            model,
            mesh,
            primitive,
        } => {
            digest
                .text("model")
                .text(model.as_str())
                .number(*mesh as u64)
                .number(*primitive as u64);
        }
    }
    match model {
        Some(model) => digest.digest(model),
        None => digest.number(0),
    };
    digest.finish()
}

pub(super) fn skin_fingerprint(entry: &SkinEntry, model: Option<Digest>) -> Digest {
    let mut digest = Digest::builder();
    digest
        .text("skin")
        .text(&entry.id)
        .text(entry.asset.model_of().as_str())
        .number(entry.asset.index() as u64);
    for joint in entry.asset.joints() {
        digest.text(joint.as_str());
    }
    match model {
        Some(model) => digest.digest(model),
        None => digest.number(0),
    };
    digest.finish()
}

pub(super) fn animation_fingerprint(entry: &AnimationEntry, model: Option<Digest>) -> Digest {
    let mut digest = Digest::builder();
    digest
        .text("animation")
        .text(&entry.id)
        .text(entry.asset.model_of().as_str())
        .number(entry.asset.index() as u64);
    for target in entry.asset.targets() {
        digest.text(target.as_str());
    }
    match model {
        Some(model) => digest.digest(model),
        None => digest.number(0),
    };
    digest.finish()
}

pub(super) fn sound_fingerprint(entry: &SoundEntry, digest: Option<Digest>) -> Digest {
    let mut fingerprint = Digest::builder();
    fingerprint
        .text("sound")
        .text(&entry.id)
        .text(&entry.path.to_string_lossy())
        .number(u64::from(entry.stream));
    match digest {
        Some(digest) => fingerprint.digest(digest),
        None => fingerprint.number(0),
    };
    fingerprint.finish()
}

fn sampler_digest(digest: &mut DigestBuilder, sampler: SamplerDesc) {
    digest
        .number(filter_code(sampler.min_filter))
        .number(filter_code(sampler.mag_filter))
        .number(filter_code(sampler.mipmap_filter))
        .number(address_code(sampler.address_mode_u))
        .number(address_code(sampler.address_mode_v));
}

fn filter_code(filter: FilterMode) -> u64 {
    match filter {
        FilterMode::Nearest => 0,
        FilterMode::Linear => 1,
    }
}

fn address_code(mode: AddressMode) -> u64 {
    match mode {
        AddressMode::ClampToEdge => 0,
        AddressMode::Repeat => 1,
        AddressMode::MirrorRepeat => 2,
        AddressMode::ClampToBorder => panic!("a material sampler holds no border clamp"),
    }
}

fn vertex_digest(digest: &mut DigestBuilder, vertex: &Vertex) {
    for value in vertex.position {
        digest.number(value.to_bits() as u64);
    }
    for value in vertex.normal {
        digest.number(value.to_bits() as u64);
    }
    for value in vertex.uv {
        digest.number(value.to_bits() as u64);
    }
}

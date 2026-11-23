use super::work::{
    EnvironmentData, ImageMips, Prepared, PreparedAnimation, PreparedEnvironment, PreparedImage,
    PreparedMaterial, PreparedMesh, PreparedSkin, PreparedSound, format_of, image_modes,
};
use super::{AssetError, ImageRecord, ImageTexture, Library, MaterialRecord, MeshRecord, Refresh};
use dome_image::ColorSpace;
use dome_render::environment::BakeRequest;
use dome_render::{
    Environment, Renderer, Sampler, SamplerDesc, SkinVertexRecord, Texture, TextureFormat,
    TextureUsages, Vertex,
};
use std::collections::{HashMap, HashSet};
use std::mem::size_of;
use std::sync::Arc;
use std::sync::OnceLock;

const ITEM_LIMIT: usize = 64;
const EMPTY_TEXEL: &[u8] = &[0, 0, 0, 0, 0, 0, 0x00, 0x3C];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Remove,
    Images,
    Materials,
    Meshes,
    Skins,
    Animations,
    Sounds,
    Environments,
    Finish,
    Done,
}

struct PendingEnvironment {
    id: String,
    fingerprint: crate::digest::Digest,
    request: BakeRequest,
    panorama: Texture,
    sampler: Sampler,
    atmosphere: Option<dome_render::environment::Atmosphere>,
}

pub(super) struct Commit {
    prepared: Prepared,
    refresh: Refresh,
    stage: Stage,
    index: usize,
    spent: u64,
    items: usize,
    pending: Vec<PendingEnvironment>,
}

impl Commit {
    pub(super) fn new(mut prepared: Prepared) -> Self {
        let refresh = Refresh {
            applied: prepared.applied,
            changed: prepared.changed(),
            errors: std::mem::take(&mut prepared.errors),
        };
        Self {
            prepared,
            refresh,
            stage: Stage::Remove,
            index: 0,
            spent: 0,
            items: 0,
            pending: Vec::new(),
        }
    }

    pub(super) fn run(
        &mut self,
        library: &mut Library,
        renderer: &mut Renderer,
        budget: u64,
    ) -> Option<Refresh> {
        self.spent = 0;
        self.items = 0;
        if !self.prepared.applied {
            library.adopt(&mut self.prepared);
            return Some(self.finish());
        }
        loop {
            if self.items >= ITEM_LIMIT {
                return None;
            }
            match self.stage {
                Stage::Remove => self.remove(library, renderer),
                Stage::Images => {
                    let Some(item) = self.prepared.images.get(self.index) else {
                        self.next(Stage::Materials);
                        continue;
                    };
                    let cost = item.mips.bytes(ColorSpace::Srgb);
                    if !self.allows(cost, budget) {
                        return None;
                    }
                    library.commit_image(renderer, item);
                    self.charge(cost);
                }
                Stage::Materials => {
                    let Some(item) = self.prepared.materials.get(self.index) else {
                        self.next(Stage::Meshes);
                        continue;
                    };
                    let cost = library.material_cost(&item.entry, &self.prepared.textures);
                    if !self.allows(cost, budget) {
                        return None;
                    }
                    if let Err(message) =
                        library.commit_material(renderer, item, &self.prepared.textures)
                    {
                        self.refresh.errors.push(AssetError {
                            kind: "material",
                            id: item.entry.id.clone(),
                            message,
                            fatal: false,
                        });
                    }
                    self.charge(cost);
                }
                Stage::Meshes => {
                    let Some(item) = self.prepared.meshes.get(self.index) else {
                        self.next(Stage::Skins);
                        continue;
                    };
                    let cost = mesh_bytes(item);
                    if !self.allows(cost, budget) {
                        return None;
                    }
                    library.commit_mesh(renderer, item);
                    self.charge(cost);
                }
                Stage::Skins => {
                    let Some(item) = self.prepared.skins.get(self.index) else {
                        self.next(Stage::Animations);
                        continue;
                    };
                    if !self.allows(0, budget) {
                        return None;
                    }
                    library.commit_skin(item);
                    self.charge(0);
                }
                Stage::Animations => {
                    let Some(item) = self.prepared.animations.get(self.index) else {
                        self.next(Stage::Sounds);
                        continue;
                    };
                    if !self.allows(0, budget) {
                        return None;
                    }
                    library.commit_animation(item);
                    self.charge(0);
                }
                Stage::Sounds => {
                    let Some(item) = self.prepared.sounds.get(self.index) else {
                        self.next(Stage::Environments);
                        continue;
                    };
                    let cost = if item.entry.stream {
                        0
                    } else {
                        item.source.frame_count().unwrap_or(0) * 4
                    };
                    if !self.allows(cost, budget) {
                        return None;
                    }
                    library.commit_sound(item);
                    self.charge(cost);
                }
                Stage::Environments => {
                    if self.index >= self.prepared.environments.len() {
                        if !self.settle_environments(library, renderer) {
                            return None;
                        }
                        self.next(Stage::Finish);
                        continue;
                    }
                    let cost = environment_bytes(&self.prepared.environments[self.index]);
                    if !self.allows(cost, budget) {
                        return None;
                    }
                    self.submit_environment(library, renderer);
                    self.charge(cost);
                }
                Stage::Finish => {
                    library.adopt(&mut self.prepared);
                    self.stage = Stage::Done;
                }
                Stage::Done => return Some(self.finish()),
            }
        }
    }

    fn remove(&mut self, library: &mut Library, renderer: &mut Renderer) {
        if self.prepared.gpu() || (library.synchronous && self.prepared.removed.gpu()) {
            renderer.idle();
        }
        for id in &self.prepared.removed.materials {
            if let Some(record) = library.materials.remove(id) {
                if library.synchronous {
                    renderer.remove_material(record.material);
                } else {
                    library.retired_materials.push(record.material);
                }
            }
        }
        for id in &self.prepared.removed.meshes {
            if let Some(record) = library.meshes.remove(id) {
                if library.synchronous {
                    renderer.remove_mesh(record.mesh);
                } else {
                    library.retired_meshes.push(record.mesh);
                }
            }
        }
        for id in &self.prepared.removed.images {
            library.images.remove(id);
            library.textures.retain(|(held, ..), _| held != id);
        }
        for id in &self.prepared.changed_images {
            library.textures.retain(|(held, ..), _| held != id);
        }
        for id in &self.prepared.removed.skins {
            library.skins.remove(id);
        }
        for id in &self.prepared.removed.animations {
            library.animations.remove(id);
        }
        for id in &self.prepared.removed.sounds {
            library.sounds.remove(id);
        }
        for id in &self.prepared.removed.environments {
            library.environments.remove(id);
        }
        self.stage = Stage::Images;
        self.index = 0;
    }

    fn finish(&mut self) -> Refresh {
        Refresh {
            applied: self.refresh.applied,
            changed: std::mem::take(&mut self.refresh.changed),
            errors: std::mem::take(&mut self.refresh.errors),
        }
    }

    fn allows(&self, cost: u64, budget: u64) -> bool {
        if self.items >= ITEM_LIMIT {
            return false;
        }
        self.items == 0 || self.spent.saturating_add(cost) <= budget
    }

    fn charge(&mut self, cost: u64) {
        self.spent = self.spent.saturating_add(cost);
        self.items += 1;
        self.index += 1;
    }

    fn next(&mut self, stage: Stage) {
        self.stage = stage;
        self.index = 0;
    }

    fn submit_environment(&mut self, library: &mut Library, renderer: &mut Renderer) {
        let item = &self.prepared.environments[self.index];
        let sampler = library.sampler(
            renderer,
            SamplerDesc {
                min_filter: dome_render::Filter::Linear,
                mag_filter: dome_render::Filter::Linear,
                mip_filter: dome_render::Filter::Linear,
                address_u: dome_render::AddressMode::Repeat,
                address_v: dome_render::AddressMode::ClampToEdge,
            },
        );
        if item.entry.capture() {
            let empty = |label: &str| {
                let texture = renderer.texture(
                    label,
                    1,
                    1,
                    1,
                    TextureFormat::Rgba16Float,
                    TextureUsages::SAMPLED | TextureUsages::COPY_DST,
                );
                renderer.write_texture(&texture, &[EMPTY_TEXEL]);
                texture
            };
            let radiance = empty(&item.entry.id);
            let panorama = empty(&format!("{}.panorama", item.entry.id));
            library.environments.insert(
                item.entry.id.clone(),
                super::EnvironmentRecord {
                    fingerprint: item.fingerprint,
                    capture: true,
                    environment: Environment::new(
                        radiance,
                        panorama,
                        sampler,
                        1,
                        [dome_math::Vec3::ZERO; 9],
                    ),
                },
            );
            return;
        }
        let (panorama, background, atmosphere) = match &item.data {
            EnvironmentData::Panorama(panorama) => {
                let texture = panorama.texture(renderer, &item.entry.id);
                (texture.clone(), texture, None)
            }
            EnvironmentData::Atmosphere(held) => {
                let panoramas = renderer.atmosphere(held);
                (panoramas.radiance, panoramas.background, Some(*held))
            }
        };
        let request = renderer.bake(&panorama, &sampler);
        self.pending.push(PendingEnvironment {
            id: item.entry.id.clone(),
            fingerprint: item.fingerprint,
            request,
            panorama: background,
            sampler,
            atmosphere,
        });
    }

    fn settle_environments(&mut self, library: &mut Library, renderer: &mut Renderer) -> bool {
        let mut held = Vec::with_capacity(self.pending.len());
        for pending in std::mem::take(&mut self.pending) {
            let Some(baked) = renderer.take_baked(pending.request) else {
                held.push(pending);
                continue;
            };
            let mut environment = Environment::new(
                baked.radiance,
                pending.panorama,
                pending.sampler,
                baked.levels,
                baked.irradiance,
            );
            if let Some(atmosphere) = pending.atmosphere {
                environment = environment
                    .lit(atmosphere.sun_intensity, true)
                    .air(atmosphere);
            }
            library.environments.insert(
                pending.id,
                super::EnvironmentRecord {
                    fingerprint: pending.fingerprint,
                    capture: false,
                    environment,
                },
            );
        }
        self.pending = held;
        self.pending.is_empty()
    }
}

impl Library {
    fn adopt(&mut self, prepared: &mut Prepared) {
        self.sources = std::mem::take(&mut prepared.sources);
        self.sources.retain(&prepared.files);
    }

    fn commit_image(&mut self, renderer: &Renderer, item: &PreparedImage) {
        let levels = item
            .mips
            .levels(ColorSpace::Srgb)
            .expect("a prepared image holds its colour levels")
            .iter()
            .map(Vec::as_slice)
            .collect::<Vec<_>>();
        let texture = renderer.texture(
            &item.entry.id,
            item.mips.width,
            item.mips.height,
            levels.len() as u32,
            TextureFormat::Rgba8UnormSrgb,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        renderer.write_texture(&texture, &levels);
        self.textures.insert(
            (
                item.entry.id.clone(),
                TextureFormat::Rgba8UnormSrgb,
                ColorSpace::Srgb,
            ),
            texture.clone(),
        );
        self.images.insert(
            item.entry.id.clone(),
            ImageRecord {
                fingerprint: item.fingerprint,
                image: ImageTexture {
                    texture,
                    sampler: item.entry.sampler,
                },
            },
        );
    }

    fn material_cost(
        &self,
        entry: &dome_scene::MaterialEntry,
        textures: &HashMap<String, Arc<ImageMips>>,
    ) -> u64 {
        let mut seen = HashSet::new();
        let mut cost = 0;
        for (id, color) in image_modes(entry) {
            let Some(id) = id else {
                continue;
            };
            let key = (String::from(id), format_of(color), color);
            if self.textures.contains_key(&key) || !seen.insert(key) {
                continue;
            }
            if let Some(mips) = textures.get(id) {
                cost += mips.bytes(color);
            }
        }
        cost
    }

    fn commit_material(
        &mut self,
        renderer: &mut Renderer,
        item: &PreparedMaterial,
        textures: &HashMap<String, Arc<ImageMips>>,
    ) -> Result<(), String> {
        let desc = self.material_desc(renderer, &item.entry, textures)?;
        let material = match self.materials.get(&item.entry.id) {
            Some(previous) => {
                renderer.update_material(previous.material, desc);
                previous.material
            }
            None => renderer.material(desc),
        };
        self.materials.insert(
            item.entry.id.clone(),
            MaterialRecord {
                fingerprint: item.fingerprint,
                material,
            },
        );
        Ok(())
    }

    fn commit_mesh(&mut self, renderer: &mut Renderer, item: &PreparedMesh) {
        let previous = self.meshes.get(&item.entry.id).map(|held| held.mesh);
        let mesh = match (previous, item.data.skin.as_deref()) {
            (Some(mesh), Some(skin)) => {
                renderer.update_skinned_mesh(mesh, &item.data.vertices, skin);
                mesh
            }
            (Some(mesh), None) => {
                renderer.update_mesh(mesh, &item.data.vertices);
                mesh
            }
            (None, Some(skin)) => renderer.skinned_mesh(&item.data.vertices, skin),
            (None, None) => renderer.mesh(&item.data.vertices),
        };
        self.meshes.insert(
            item.entry.id.clone(),
            MeshRecord {
                fingerprint: item.fingerprint,
                mesh,
                positions: Arc::clone(&item.data.positions),
                shape: OnceLock::new(),
            },
        );
    }

    fn commit_skin(&mut self, item: &PreparedSkin) {
        self.skins.insert(
            item.entry.id.clone(),
            super::SkinRecord {
                fingerprint: item.fingerprint,
                data: item.data.clone(),
            },
        );
    }

    fn commit_animation(&mut self, item: &PreparedAnimation) {
        self.animations.insert(
            item.entry.id.clone(),
            super::AnimationRecord {
                fingerprint: item.fingerprint,
                clip: Arc::clone(&item.clip),
            },
        );
    }

    fn commit_sound(&mut self, item: &PreparedSound) {
        self.sounds.insert(
            item.entry.id.clone(),
            super::SoundRecord {
                fingerprint: item.fingerprint,
                source: item.source.clone(),
            },
        );
    }
}

fn environment_bytes(item: &PreparedEnvironment) -> u64 {
    match &item.data {
        EnvironmentData::Panorama(panorama) => {
            u64::from(panorama.width()) * u64::from(panorama.height()) * 8
        }
        EnvironmentData::Atmosphere(_) => 0,
    }
}

fn mesh_bytes(item: &PreparedMesh) -> u64 {
    let vertices = item.data.vertices.len() as u64 * size_of::<Vertex>() as u64;
    let skin = item.data.skin.as_ref().map_or(0, |skin| {
        skin.len() as u64 * size_of::<SkinVertexRecord>() as u64
    });
    vertices + skin
}

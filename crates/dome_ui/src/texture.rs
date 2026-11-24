use dome_gpu::{
    AddressMode, Device, FilterMode, Queue, ResourceId, Sampler, SamplerDesc, SamplerTable,
    Submission, Texture, TextureFormat, TextureRegion, TextureTable, TextureUsages,
};
use dome_gpu::{SAMPLER_TABLE_CAPACITY, TEXTURE_TABLE_CAPACITY};
use dome_text::Text;
use std::collections::{HashMap, HashSet};

const WHITE: [u8; 4] = [255, 255, 255, 255];

pub(crate) const LINEAR_SAMPLER: u32 = 0;

pub(crate) const LINEAR: SamplerDesc = SamplerDesc {
    min_filter: FilterMode::Linear,
    mag_filter: FilterMode::Linear,
    mipmap_filter: FilterMode::Linear,
    address_mode_u: AddressMode::ClampToEdge,
    address_mode_v: AddressMode::ClampToEdge,
    compare: None,
};

pub(crate) const NEAREST: SamplerDesc = SamplerDesc {
    min_filter: FilterMode::Nearest,
    mag_filter: FilterMode::Nearest,
    mipmap_filter: FilterMode::Nearest,
    address_mode_u: AddressMode::ClampToEdge,
    address_mode_v: AddressMode::ClampToEdge,
    compare: None,
};

struct Named {
    index: u32,
    sampler: u32,
    identity: ResourceId,
}

pub(crate) struct Textures {
    device: Device,
    table: TextureTable,
    samplers: SamplerTable,
    white: Texture,
    atlas: Option<Texture>,
    atlas_revision: Option<u64>,
    atlas_index: u32,
    scene_index: u32,
    scene_identity: ResourceId,
    named: HashMap<String, Named>,
    generation: u64,
}

impl Textures {
    pub(crate) fn new(device: &Device, queue: &Queue) -> Self {
        let white = Texture::new(
            device,
            "dome ui white",
            1,
            1,
            1,
            TextureFormat::Rgba8Unorm,
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        );
        white.write(queue, &WHITE);
        let mut table = TextureTable::new(device, TEXTURE_TABLE_CAPACITY, &white);
        let scene_index = table.add(&white);
        let atlas_index = table.add(&white);
        let mut samplers = SamplerTable::new(
            device,
            SAMPLER_TABLE_CAPACITY,
            &Sampler::new(device, LINEAR),
        );
        samplers.add(&Sampler::new(device, LINEAR));
        samplers.add(&Sampler::new(device, NEAREST));
        Self {
            device: device.clone(),
            table,
            samplers,
            white: white.clone(),
            atlas: None,
            atlas_revision: None,
            atlas_index,
            scene_index,
            scene_identity: white.identity(),
            named: HashMap::new(),
            generation: 0,
        }
    }

    pub(crate) fn refresh(&mut self, text: &mut Text, submission: &mut Submission) {
        let revision = text.atlas().revision();
        if self.atlas_revision == Some(revision) {
            return;
        }
        let size = text.atlas().size();
        if self.atlas.as_ref().is_some_and(|held| held.width() == size) {
            let regions = text.atlas_mut().drain();
            let atlas = text.atlas();
            let pixels = atlas.pixels();
            let texture = self.atlas.as_ref().expect("a held atlas names its texture");
            for region in regions {
                let offset = (region.y * size + region.x) as usize;
                submission.write_texture_region(
                    texture,
                    0,
                    TextureRegion::new(region.x, region.y, region.width, region.height),
                    u64::from(size),
                    &pixels[offset..],
                );
            }
        } else {
            let atlas = text.atlas();
            let texture = Texture::new(
                &self.device,
                "dome ui atlas",
                size,
                size,
                1,
                TextureFormat::R8Unorm,
                TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            );
            submission.write_texture_region(
                &texture,
                0,
                TextureRegion::new(0, 0, size, size),
                u64::from(size),
                atlas.pixels(),
            );
            let _ = text.atlas_mut().drain();
            self.table.set(self.atlas_index, &texture);
            self.atlas = Some(texture);
            self.generation = self.generation.wrapping_add(1);
        }
        self.atlas_revision = Some(revision);
    }

    pub(crate) fn sync<'a>(
        &mut self,
        entries: impl IntoIterator<Item = (&'a str, &'a Texture, SamplerDesc)>,
    ) -> bool {
        let entries = entries.into_iter().collect::<Vec<_>>();
        let wanted = entries
            .iter()
            .map(|(name, ..)| *name)
            .collect::<HashSet<_>>();
        let mut changed = false;
        let stale = self
            .named
            .iter()
            .filter(|(name, _)| !wanted.contains(name.as_str()))
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        for name in stale {
            let named = self
                .named
                .remove(&name)
                .expect("a released interface texture names its entry");
            self.table.release(named.index, &self.white);
            changed = true;
        }
        for (name, texture, sampler) in entries {
            let sampler = self.samplers.add_described(sampler);
            let identity = texture.identity();
            if let Some(named) = self.named.get_mut(name) {
                if named.identity != identity {
                    self.table.set(named.index, texture);
                    named.identity = identity;
                    changed = true;
                }
                if named.sampler != sampler {
                    named.sampler = sampler;
                    changed = true;
                }
                continue;
            }
            let index = self.table.add(texture);
            self.named.insert(
                String::from(name),
                Named {
                    index,
                    sampler,
                    identity,
                },
            );
            changed = true;
        }
        if changed {
            self.generation = self.generation.wrapping_add(1);
        }
        changed
    }

    pub(crate) fn index(&self, name: &str) -> u32 {
        self.named
            .get(name)
            .unwrap_or_else(|| panic!("a user interface paints the unregistered texture {name}"))
            .index
    }

    pub(crate) fn sampler(&self, name: &str) -> u32 {
        self.named
            .get(name)
            .unwrap_or_else(|| panic!("a user interface paints the unregistered texture {name}"))
            .sampler
    }

    pub(crate) fn set_scene(&mut self, texture: &Texture) {
        if self.scene_identity == texture.identity() {
            return;
        }
        self.table.set(self.scene_index, texture);
        self.scene_identity = texture.identity();
        self.generation = self.generation.wrapping_add(1);
    }

    pub(crate) fn scene(&self) -> (u32, u32) {
        (self.scene_index, LINEAR_SAMPLER)
    }

    pub(crate) fn atlas(&self) -> (u32, u32) {
        (self.atlas_index, LINEAR_SAMPLER)
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn table(&self) -> &TextureTable {
        &self.table
    }

    pub(crate) fn samplers(&self) -> &SamplerTable {
        &self.samplers
    }
}

use crate::context::Device;
use crate::identity::ResourceId;
use crate::sampler::{Sampler, SamplerDesc};
use crate::texture::{Texture, TextureUsages};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub const TEXTURE_TABLE_CAPACITY: u32 = 1024;
pub const SAMPLER_TABLE_CAPACITY: u32 = 64;

struct TextureTableState {
    generation: AtomicU64,
    identity: ResourceId,
}

pub struct TextureTable {
    device: Device,
    capacity: u32,
    entries: Vec<Texture>,
    fallback: Texture,
    occupied: Vec<bool>,
    free: Vec<u32>,
    grown: u32,
    filled: u32,
    state: Arc<TextureTableState>,
}

impl Clone for TextureTable {
    fn clone(&self) -> Self {
        Self {
            device: self.device.clone(),
            capacity: self.capacity,
            entries: self.entries.clone(),
            fallback: self.fallback.clone(),
            occupied: self.occupied.clone(),
            free: self.free.clone(),
            grown: self.grown,
            filled: self.filled,
            state: Arc::clone(&self.state),
        }
    }
}

impl TextureTable {
    pub fn new(device: &Device, capacity: u32, fallback: &Texture) -> Self {
        assert!(capacity > 0, "a texture table holds no image");
        assert!(
            fallback.device().same(device),
            "a texture table fills from another device"
        );
        assert!(
            fallback.usages().contains(TextureUsages::TEXTURE_BINDING),
            "a texture table fills from an unsampled texture"
        );
        Self {
            device: device.clone(),
            capacity,
            entries: vec![fallback.clone(); capacity as usize],
            fallback: fallback.clone(),
            occupied: vec![false; capacity as usize],
            free: Vec::new(),
            grown: 0,
            filled: 0,
            state: Arc::new(TextureTableState {
                generation: AtomicU64::new(1),
                identity: ResourceId::next(),
            }),
        }
    }

    pub fn reset(&mut self, fallback: &Texture) {
        for index in self.indices().collect::<Vec<_>>() {
            self.fill(index, fallback);
        }
        self.occupied.fill(false);
        self.free.clear();
        self.grown = 0;
        self.filled = 0;
        self.fallback = fallback.clone();
    }

    pub fn add(&mut self, texture: &Texture) -> u32 {
        let index = match self.free.pop() {
            Some(index) => index,
            None => {
                assert!(
                    self.grown < self.capacity,
                    "a texture table of {} images is full",
                    self.capacity
                );
                self.grown += 1;
                self.grown - 1
            }
        };
        self.occupied[index as usize] = true;
        self.filled += 1;
        self.set(index, texture);
        index
    }

    pub fn release(&mut self, index: u32, fallback: &Texture) {
        assert!(
            self.occupied(index),
            "a texture table of {} images releases the slot {index} it holds no image in",
            self.capacity,
        );
        self.fill(index, fallback);
        self.occupied[index as usize] = false;
        self.filled -= 1;
        self.free.push(index);
    }

    pub fn set(&mut self, index: u32, texture: &Texture) {
        assert!(
            index < self.capacity,
            "a texture table of {} images holds no slot {index}",
            self.capacity
        );
        assert!(
            self.occupied(index),
            "a texture table of {} images writes the slot {index} it holds no image in",
            self.capacity,
        );
        self.fill(index, texture);
    }

    pub fn add_shared(&mut self, texture: &Texture) -> u32 {
        match self.position(texture) {
            Some(index) => index,
            None => self.add(texture),
        }
    }

    pub fn position(&self, texture: &Texture) -> Option<u32> {
        self.indices()
            .find(|index| self.entries[*index as usize].identity() == texture.identity())
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn filled(&self) -> u32 {
        self.filled
    }

    pub(crate) fn device(&self) -> &Device {
        &self.device
    }

    pub(crate) fn identity(&self) -> ResourceId {
        self.state.identity
    }

    pub(crate) fn generation(&self) -> u64 {
        self.state.generation.load(Ordering::Relaxed)
    }

    pub(crate) fn views(&self) -> Vec<&wgpu::TextureView> {
        let grown = self.grown.max(1) as usize;
        if self.grown == 0 {
            return vec![self.fallback.view()];
        }
        self.entries[..grown]
            .iter()
            .map(|texture| texture.view())
            .collect()
    }

    fn occupied(&self, index: u32) -> bool {
        self.occupied.get(index as usize).is_some_and(|held| *held)
    }

    fn indices(&self) -> impl Iterator<Item = u32> {
        self.occupied
            .iter()
            .enumerate()
            .filter(|(_, held)| **held)
            .map(|(index, _)| index as u32)
    }

    fn fill(&mut self, index: u32, texture: &Texture) {
        assert!(
            texture.device().same(&self.device),
            "a texture table fills from another device"
        );
        assert!(
            texture.usages().contains(TextureUsages::TEXTURE_BINDING),
            "a texture table holds sampled textures"
        );
        self.entries[index as usize] = texture.clone();
        self.state.generation.fetch_add(1, Ordering::Relaxed);
    }
}

struct SamplerTableState {
    generation: AtomicU64,
    identity: ResourceId,
}

pub struct SamplerTable {
    device: Device,
    capacity: u32,
    entries: Vec<Sampler>,
    fallback: Sampler,
    occupied: Vec<bool>,
    free: Vec<u32>,
    grown: u32,
    filled: u32,
    state: Arc<SamplerTableState>,
}

impl Clone for SamplerTable {
    fn clone(&self) -> Self {
        Self {
            device: self.device.clone(),
            capacity: self.capacity,
            entries: self.entries.clone(),
            fallback: self.fallback.clone(),
            occupied: self.occupied.clone(),
            free: self.free.clone(),
            grown: self.grown,
            filled: self.filled,
            state: Arc::clone(&self.state),
        }
    }
}

impl SamplerTable {
    pub fn new(device: &Device, capacity: u32, fallback: &Sampler) -> Self {
        assert!(capacity > 0, "a sampler table holds no sampler");
        assert!(
            fallback.device().same(device),
            "a sampler table fills from another device"
        );
        Self {
            device: device.clone(),
            capacity,
            entries: vec![fallback.clone(); capacity as usize],
            fallback: fallback.clone(),
            occupied: vec![false; capacity as usize],
            free: Vec::new(),
            grown: 0,
            filled: 0,
            state: Arc::new(SamplerTableState {
                generation: AtomicU64::new(1),
                identity: ResourceId::next(),
            }),
        }
    }

    pub fn reset(&mut self, fallback: &Sampler) {
        for index in self.indices().collect::<Vec<_>>() {
            self.fill(index, fallback);
        }
        self.occupied.fill(false);
        self.free.clear();
        self.grown = 0;
        self.filled = 0;
        self.fallback = fallback.clone();
    }

    pub fn add(&mut self, sampler: &Sampler) -> u32 {
        let index = match self.free.pop() {
            Some(index) => index,
            None => {
                assert!(
                    self.grown < self.capacity,
                    "a sampler table of {} samplers is full",
                    self.capacity
                );
                self.grown += 1;
                self.grown - 1
            }
        };
        self.occupied[index as usize] = true;
        self.filled += 1;
        self.set(index, sampler);
        index
    }

    pub fn add_described(&mut self, desc: SamplerDesc) -> u32 {
        match self.position(desc) {
            Some(index) => index,
            None => {
                let sampler = Sampler::new(&self.device, desc);
                self.add(&sampler)
            }
        }
    }

    pub fn release(&mut self, index: u32, fallback: &Sampler) {
        assert!(
            self.occupied(index),
            "a sampler table of {} samplers releases the slot {index} it holds no sampler in",
            self.capacity,
        );
        self.fill(index, fallback);
        self.occupied[index as usize] = false;
        self.filled -= 1;
        self.free.push(index);
    }

    pub fn position(&self, desc: SamplerDesc) -> Option<u32> {
        self.indices()
            .find(|index| self.entries[*index as usize].desc() == desc)
    }

    pub fn set(&mut self, index: u32, sampler: &Sampler) {
        assert!(
            index < self.capacity,
            "a sampler table of {} samplers holds no slot {index}",
            self.capacity
        );
        assert!(
            self.occupied(index),
            "a sampler table of {} samplers writes the slot {index} it holds no sampler in",
            self.capacity,
        );
        self.fill(index, sampler);
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn filled(&self) -> u32 {
        self.filled
    }

    pub(crate) fn device(&self) -> &Device {
        &self.device
    }

    pub(crate) fn identity(&self) -> ResourceId {
        self.state.identity
    }

    pub(crate) fn generation(&self) -> u64 {
        self.state.generation.load(Ordering::Relaxed)
    }

    pub(crate) fn samplers(&self) -> Vec<&wgpu::Sampler> {
        let grown = self.grown.max(1) as usize;
        if self.grown == 0 {
            return vec![self.fallback.native()];
        }
        self.entries[..grown]
            .iter()
            .map(|sampler| sampler.native())
            .collect()
    }

    fn occupied(&self, index: u32) -> bool {
        self.occupied.get(index as usize).is_some_and(|held| *held)
    }

    fn indices(&self) -> impl Iterator<Item = u32> {
        self.occupied
            .iter()
            .enumerate()
            .filter(|(_, held)| **held)
            .map(|(index, _)| index as u32)
    }

    fn fill(&mut self, index: u32, sampler: &Sampler) {
        assert!(
            sampler.device().same(&self.device),
            "a sampler table fills from another device"
        );
        self.entries[index as usize] = sampler.clone();
        self.state.generation.fetch_add(1, Ordering::Relaxed);
    }
}

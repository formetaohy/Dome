use crate::capability::TextureUsages;
use crate::context::Device;
use crate::native::{NativeSamplerTable, NativeTextureTable};
use crate::sampler::{Sampler, SamplerDesc};
use crate::texture::Texture;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

struct TextureTableState {
    native: NativeTextureTable,
    generation: AtomicU64,
}

pub struct TextureTable {
    device: Device,
    capacity: u32,
    entries: Vec<Texture>,
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
            fallback.usages().contains(TextureUsages::SAMPLED),
            "a texture table fills from an unsampled texture"
        );
        let native = device.native().create_texture_table(capacity, fallback);
        Self {
            device: device.clone(),
            capacity,
            entries: vec![fallback.clone(); capacity as usize],
            occupied: vec![false; capacity as usize],
            free: Vec::new(),
            grown: 0,
            filled: 0,
            state: Arc::new(TextureTableState {
                native,
                generation: AtomicU64::new(1),
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

    pub(crate) fn entries(&self) -> impl Iterator<Item = &Texture> {
        self.entries.iter()
    }

    pub(crate) fn native(&self) -> &NativeTextureTable {
        &self.state.native
    }

    pub(crate) fn identity(&self) -> usize {
        Arc::as_ptr(&self.state) as usize
    }

    pub(crate) fn generation(&self) -> u64 {
        self.state.generation.load(Ordering::Relaxed)
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
            texture.usages().contains(TextureUsages::SAMPLED),
            "a texture table holds sampled textures"
        );
        self.entries[index as usize] = texture.clone();
        self.state.generation.fetch_add(1, Ordering::Relaxed);
        self.device
            .native()
            .fill_texture_table(&self.state.native, index, texture);
    }
}

struct SamplerTableState {
    native: NativeSamplerTable,
    generation: AtomicU64,
}

pub struct SamplerTable {
    device: Device,
    capacity: u32,
    entries: Vec<Sampler>,
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
        let native = device.native().create_sampler_table(capacity, fallback);
        Self {
            device: device.clone(),
            capacity,
            entries: vec![fallback.clone(); capacity as usize],
            occupied: vec![false; capacity as usize],
            free: Vec::new(),
            grown: 0,
            filled: 0,
            state: Arc::new(SamplerTableState {
                native,
                generation: AtomicU64::new(1),
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
        self.fill(index, sampler);
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

    pub(crate) fn entries(&self) -> impl Iterator<Item = &Sampler> {
        self.entries.iter()
    }

    pub(crate) fn native(&self) -> &NativeSamplerTable {
        &self.state.native
    }

    pub(crate) fn identity(&self) -> usize {
        Arc::as_ptr(&self.state) as usize
    }

    pub(crate) fn generation(&self) -> u64 {
        self.state.generation.load(Ordering::Relaxed)
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
        self.device
            .native()
            .fill_sampler_table(&self.state.native, index, sampler);
    }
}

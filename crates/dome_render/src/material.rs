use crate::abi::{ALPHA_BLEND, ALPHA_MASK, ALPHA_OPAQUE, MaterialRecord};
use dome_gpu::Fvec4;
use dome_gpu::{SAMPLER_TABLE_CAPACITY, TEXTURE_TABLE_CAPACITY};
use dome_gpu_device::{
    AddressMode, Device, Filter, Queue, Sampler, SamplerDesc, SamplerTable, Texture, TextureFormat,
    TextureTable, TextureUsages,
};
use dome_math::Vec3;

const WHITE: [u8; 4] = [255, 255, 255, 255];
const FLAT_NORMAL: [u8; 4] = [128, 128, 255, 255];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

impl AlphaMode {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
            Self::Mask => "mask",
            Self::Blend => "blend",
        }
    }

    pub fn of(name: &str) -> Option<Self> {
        match name {
            "opaque" => Some(Self::Opaque),
            "mask" => Some(Self::Mask),
            "blend" => Some(Self::Blend),
            _ => None,
        }
    }

    pub(crate) const fn code(self) -> u32 {
        match self {
            Self::Opaque => ALPHA_OPAQUE,
            Self::Mask => ALPHA_MASK,
            Self::Blend => ALPHA_BLEND,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Material(u32);

impl Material {
    pub(crate) fn index(self) -> u32 {
        self.0
    }
}

#[derive(Clone)]
pub struct MaterialImage {
    texture: Texture,
    sampler: Sampler,
}

impl MaterialImage {
    pub fn new(texture: Texture, sampler: Sampler) -> Self {
        assert!(
            texture.usages().contains(TextureUsages::SAMPLED),
            "a material image holds an unsampled texture",
        );
        Self { texture, sampler }
    }
}

pub struct MaterialDesc {
    base_color: Vec3,
    alpha: f32,
    alpha_mode: AlphaMode,
    alpha_cutoff: f32,
    base_color_image: Option<MaterialImage>,
    metallic: f32,
    roughness: f32,
    metallic_roughness_image: Option<MaterialImage>,
    normal_image: Option<MaterialImage>,
    normal_scale: f32,
    occlusion_image: Option<MaterialImage>,
    occlusion_strength: f32,
    emissive: Vec3,
    emissive_image: Option<MaterialImage>,
}

impl MaterialDesc {
    pub fn new(base_color: Vec3) -> Self {
        assert!(
            (0.0..=1.0).contains(&base_color.x)
                && (0.0..=1.0).contains(&base_color.y)
                && (0.0..=1.0).contains(&base_color.z),
            "a material of base color {base_color:?} lies outside the unit cube",
        );
        Self {
            base_color,
            alpha: 1.0,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            base_color_image: None,
            metallic: 0.0,
            roughness: 1.0,
            metallic_roughness_image: None,
            normal_image: None,
            normal_scale: 1.0,
            occlusion_image: None,
            occlusion_strength: 1.0,
            emissive: Vec3::ZERO,
            emissive_image: None,
        }
    }

    pub fn base_color_image(mut self, image: MaterialImage) -> Self {
        self.base_color_image = Some(image);
        self
    }

    pub fn alpha(mut self, alpha: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&alpha),
            "a material of alpha {alpha} lies outside the unit range",
        );
        self.alpha = alpha;
        self
    }

    pub fn alpha_mode(mut self, mode: AlphaMode) -> Self {
        self.alpha_mode = mode;
        self
    }

    pub fn alpha_cutoff(mut self, cutoff: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&cutoff),
            "a material of alpha cutoff {cutoff} lies outside the unit range",
        );
        self.alpha_cutoff = cutoff;
        self
    }

    pub fn metallic(mut self, metallic: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&metallic),
            "a material of metallic {metallic} lies outside the metallic range",
        );
        self.metallic = metallic;
        self
    }

    pub fn roughness(mut self, roughness: f32) -> Self {
        assert!(
            roughness > 0.0 && roughness <= 1.0,
            "a material of roughness {roughness} scatters no light",
        );
        self.roughness = roughness;
        self
    }

    pub fn metallic_roughness_image(mut self, image: MaterialImage) -> Self {
        self.metallic_roughness_image = Some(image);
        self
    }

    pub fn normal_image(mut self, image: MaterialImage) -> Self {
        self.normal_image = Some(image);
        self
    }

    pub fn normal_scale(mut self, scale: f32) -> Self {
        assert!(
            scale.is_finite() && scale >= 0.0,
            "a material of normal scale {scale} scales no normal",
        );
        self.normal_scale = scale;
        self
    }

    pub fn occlusion_image(mut self, image: MaterialImage) -> Self {
        self.occlusion_image = Some(image);
        self
    }

    pub fn occlusion_strength(mut self, strength: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&strength),
            "a material of occlusion strength {strength} lies outside the unit range",
        );
        self.occlusion_strength = strength;
        self
    }

    pub fn emissive(mut self, emissive: Vec3) -> Self {
        assert!(
            emissive.x >= 0.0 && emissive.y >= 0.0 && emissive.z >= 0.0,
            "a material emits the light {emissive:?}",
        );
        self.emissive = emissive;
        self
    }

    pub fn emissive_image(mut self, image: MaterialImage) -> Self {
        self.emissive_image = Some(image);
        self
    }
}

pub(crate) struct MaterialLibrary {
    textures: TextureTable,
    samplers: SamplerTable,
    records: Vec<MaterialRecord>,
    modes: Vec<AlphaMode>,
    white_srgb: Texture,
    white_linear: Texture,
    flat_normal: Texture,
    clamp: Sampler,
    white_srgb_index: u32,
    white_linear_index: u32,
    flat_normal_index: u32,
    clamp_index: u32,
    texture_counts: Vec<u32>,
    texture_free: Vec<u32>,
    sampler_counts: Vec<u32>,
    sampler_free: Vec<u32>,
}

impl MaterialLibrary {
    pub(crate) fn new(device: &Device, queue: &Queue) -> Self {
        let white_srgb = Texture::new(
            device,
            "dome material white",
            1,
            1,
            1,
            TextureFormat::Rgba8UnormSrgb,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        white_srgb.write(queue, &WHITE);
        let white_linear = Texture::new(
            device,
            "dome material linear white",
            1,
            1,
            1,
            TextureFormat::Rgba8Unorm,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        white_linear.write(queue, &WHITE);
        let flat_normal = Texture::new(
            device,
            "dome material flat normal",
            1,
            1,
            1,
            TextureFormat::Rgba8Unorm,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        flat_normal.write(queue, &FLAT_NORMAL);
        let clamp = Sampler::new(
            device,
            SamplerDesc {
                min_filter: Filter::Nearest,
                mag_filter: Filter::Nearest,
                mip_filter: Filter::Nearest,
                address_u: AddressMode::ClampToEdge,
                address_v: AddressMode::ClampToEdge,
            },
        );
        let textures = TextureTable::new(device, TEXTURE_TABLE_CAPACITY, &white_srgb);
        let samplers = SamplerTable::new(device, SAMPLER_TABLE_CAPACITY, &clamp);
        let mut library = Self {
            textures,
            samplers,
            records: Vec::new(),
            modes: Vec::new(),
            white_srgb,
            white_linear,
            flat_normal,
            clamp,
            white_srgb_index: 0,
            white_linear_index: 0,
            flat_normal_index: 0,
            clamp_index: 0,
            texture_counts: vec![0; TEXTURE_TABLE_CAPACITY as usize],
            texture_free: Vec::new(),
            sampler_counts: vec![0; SAMPLER_TABLE_CAPACITY as usize],
            sampler_free: Vec::new(),
        };
        library.reset();
        library
    }

    fn reset(&mut self) {
        self.textures.reset(&self.white_srgb);
        self.samplers.reset(&self.clamp);
        self.texture_counts.fill(0);
        self.texture_free.clear();
        self.sampler_counts.fill(0);
        self.sampler_free.clear();
        self.white_srgb_index = self.textures.add(&self.white_srgb);
        self.white_linear_index = self.textures.add(&self.white_linear);
        self.flat_normal_index = self.textures.add(&self.flat_normal);
        self.clamp_index = self.samplers.add(&self.clamp);
        for index in [
            self.white_srgb_index,
            self.white_linear_index,
            self.flat_normal_index,
        ] {
            self.texture_counts[index as usize] = 1;
        }
        self.sampler_counts[self.clamp_index as usize] = 1;
    }

    pub(crate) fn add(&mut self, desc: MaterialDesc) -> Material {
        self.validate(&desc);
        let record = self.record(&desc);
        let material = Material(self.records.len() as u32);
        self.modes.push(desc.alpha_mode);
        self.records.push(record);
        material
    }

    pub(crate) fn remove(&mut self, material: Material) {
        let index = material.0 as usize;
        let previous = *self
            .records
            .get(index)
            .unwrap_or_else(|| panic!("material {} belongs to another renderer", material.0));
        self.release(&previous);
        self.records[index] = self.record(&MaterialDesc::new(Vec3::ONE));
        self.modes[index] = AlphaMode::Opaque;
    }

    pub(crate) fn set(&mut self, material: Material, desc: MaterialDesc) {
        self.validate(&desc);
        let index = material.0 as usize;
        assert!(
            index < self.records.len(),
            "material {} belongs to another renderer",
            material.0
        );
        let previous = self.records[index];
        self.release(&previous);
        let record = self.record(&desc);
        self.records[index] = record;
        self.modes[index] = desc.alpha_mode;
    }

    fn validate(&self, desc: &MaterialDesc) {
        assert!(
            desc.base_color_image
                .as_ref()
                .is_none_or(|image| image.texture.format() == TextureFormat::Rgba8UnormSrgb),
            "a material samples a base color image of another format",
        );
        assert!(
            desc.emissive_image
                .as_ref()
                .is_none_or(|image| image.texture.format() == TextureFormat::Rgba8UnormSrgb),
            "a material samples an emissive image of another format",
        );
        assert!(
            desc.metallic_roughness_image
                .as_ref()
                .is_none_or(|image| image.texture.format() == TextureFormat::Rgba8Unorm),
            "a material samples a metallic roughness image of another format",
        );
        assert!(
            desc.normal_image
                .as_ref()
                .is_none_or(|image| image.texture.format() == TextureFormat::Rgba8Unorm),
            "a material samples a normal image of another format",
        );
        assert!(
            desc.occlusion_image
                .as_ref()
                .is_none_or(|image| image.texture.format() == TextureFormat::Rgba8Unorm),
            "a material samples an occlusion image of another format",
        );
        assert!(
            desc.alpha.is_finite() && (0.0..=1.0).contains(&desc.alpha),
            "a material of alpha {} lies outside the unit range",
            desc.alpha,
        );
        assert!(
            desc.alpha_cutoff.is_finite() && (0.0..=1.0).contains(&desc.alpha_cutoff),
            "a material of alpha cutoff {} lies outside the unit range",
            desc.alpha_cutoff,
        );
    }

    fn record(&mut self, desc: &MaterialDesc) -> MaterialRecord {
        let (base_color_image, base_color_sampler) = self.binding(
            desc.base_color_image.as_ref(),
            self.white_srgb_index,
            self.clamp_index,
        );
        let (metallic_roughness_image, metallic_roughness_sampler) = self.binding(
            desc.metallic_roughness_image.as_ref(),
            self.white_linear_index,
            self.clamp_index,
        );
        let (normal_image, normal_sampler) = self.binding(
            desc.normal_image.as_ref(),
            self.flat_normal_index,
            self.clamp_index,
        );
        let (occlusion_image, occlusion_sampler) = self.binding(
            desc.occlusion_image.as_ref(),
            self.white_linear_index,
            self.clamp_index,
        );
        let (emissive_image, emissive_sampler) = self.binding(
            desc.emissive_image.as_ref(),
            self.white_srgb_index,
            self.clamp_index,
        );
        MaterialRecord {
            base_color_image,
            base_color_sampler,
            metallic_roughness_image,
            metallic_roughness_sampler,
            normal_image,
            normal_sampler,
            occlusion_image,
            occlusion_sampler,
            emissive_image,
            emissive_sampler,
            base_color: Fvec4 {
                x: desc.base_color.x,
                y: desc.base_color.y,
                z: desc.base_color.z,
                w: desc.alpha,
            },
            emissive: vector(desc.emissive),
            metallic: desc.metallic,
            roughness: desc.roughness,
            normal_scale: desc.normal_scale,
            occlusion_strength: desc.occlusion_strength,
            alpha_mode: desc.alpha_mode.code(),
            alpha_cutoff: desc.alpha_cutoff,
        }
    }

    fn release(&mut self, record: &MaterialRecord) {
        self.release_texture(record.base_color_image);
        self.release_texture(record.metallic_roughness_image);
        self.release_texture(record.normal_image);
        self.release_texture(record.occlusion_image);
        self.release_texture(record.emissive_image);
        self.release_sampler(record.base_color_sampler);
        self.release_sampler(record.metallic_roughness_sampler);
        self.release_sampler(record.normal_sampler);
        self.release_sampler(record.occlusion_sampler);
        self.release_sampler(record.emissive_sampler);
    }

    pub(crate) fn mode(&self, material: Material) -> AlphaMode {
        *self
            .modes
            .get(material.0 as usize)
            .expect("a rendered instance names a material this renderer does not hold")
    }

    fn binding(&mut self, image: Option<&MaterialImage>, texture: u32, sampler: u32) -> (u32, u32) {
        match image {
            Some(image) => (
                self.acquire_texture(&image.texture),
                self.acquire_sampler(&image.sampler),
            ),
            None => {
                self.acquire_texture_index(texture);
                self.acquire_sampler_index(sampler);
                (texture, sampler)
            }
        }
    }

    fn acquire_texture(&mut self, texture: &Texture) -> u32 {
        match self.textures.position(texture) {
            Some(index) => {
                self.acquire_texture_index(index);
                index
            }
            None => {
                let index = match self.texture_free.pop() {
                    Some(index) => {
                        self.textures.set(index, texture);
                        index
                    }
                    None => self.textures.add(texture),
                };
                self.texture_counts[index as usize] = 1;
                index
            }
        }
    }

    fn acquire_texture_index(&mut self, index: u32) {
        self.texture_counts[index as usize] += 1;
    }

    fn release_texture(&mut self, index: u32) {
        let count = &mut self.texture_counts[index as usize];
        assert!(*count > 0, "a material releases an unheld texture");
        *count -= 1;
        if *count == 0 {
            self.textures.set(index, &self.white_srgb);
            self.texture_free.push(index);
        }
    }

    fn acquire_sampler(&mut self, sampler: &Sampler) -> u32 {
        match self.samplers.position(sampler.desc()) {
            Some(index) => {
                self.acquire_sampler_index(index);
                index
            }
            None => {
                let index = match self.sampler_free.pop() {
                    Some(index) => {
                        self.samplers.set(index, sampler);
                        index
                    }
                    None => self.samplers.add(sampler),
                };
                self.sampler_counts[index as usize] = 1;
                index
            }
        }
    }

    fn acquire_sampler_index(&mut self, index: u32) {
        self.sampler_counts[index as usize] += 1;
    }

    fn release_sampler(&mut self, index: u32) {
        let count = &mut self.sampler_counts[index as usize];
        assert!(*count > 0, "a material releases an unheld sampler");
        *count -= 1;
        if *count == 0 {
            self.samplers.set(index, &self.clamp);
            self.sampler_free.push(index);
        }
    }

    pub(crate) fn records(&self) -> &[MaterialRecord] {
        &self.records
    }

    pub(crate) fn textures(&self) -> &TextureTable {
        &self.textures
    }

    pub(crate) fn samplers(&self) -> &SamplerTable {
        &self.samplers
    }
}

fn vector(value: Vec3) -> Fvec4 {
    Fvec4 {
        x: value.x,
        y: value.y,
        z: value.z,
        w: 0.0,
    }
}

mod atmosphere;
mod bake;
mod brdf;
mod panorama;
mod sample;

use dome_gpu::{Sampler, Texture, TextureFormat};
use dome_math::Vec3;

pub use atmosphere::{Atmosphere, MODEL_VERSION, PANORAMA_HEIGHT, PANORAMA_WIDTH};
pub use bake::Baked;
pub use brdf::brdf;
pub use panorama::Panorama;

pub(crate) use atmosphere::{AtmosphereBaker, AtmospherePanorama};
pub(crate) use bake::{BakeJob, Baker};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BakeRequest(usize);

impl BakeRequest {
    pub(crate) fn of(index: usize) -> Self {
        Self(index)
    }

    pub(crate) fn index(self) -> usize {
        self.0
    }
}

pub const BRDF_SIZE: u32 = 64;
pub const BRDF_SAMPLES: u32 = 128;
pub const LEVELS: u32 = 8;
pub const SPECULAR_WIDTH: u32 = 256;
pub const SPECULAR_SAMPLES: u32 = 128;

#[derive(Clone)]
pub struct Environment {
    pub radiance: Texture,
    pub panorama: Texture,
    pub sampler: Sampler,
    pub levels: u32,
    pub irradiance: [Vec3; 9],
    pub intensity: f32,
    pub background: bool,
    pub local: bool,
    pub blend: f32,
    pub atmosphere: Option<Atmosphere>,
}

impl Environment {
    pub fn new(
        radiance: Texture,
        panorama: Texture,
        sampler: Sampler,
        levels: u32,
        irradiance: [Vec3; 9],
    ) -> Self {
        assert!(
            levels > 0,
            "an environment of radiance prefiltered to no level lights nothing",
        );
        assert_eq!(
            radiance.format(),
            TextureFormat::Rgba16Float,
            "an environment of radiance holds half float texels",
        );
        assert_eq!(
            radiance.mip_count(),
            levels,
            "an environment of {levels} levels holds a texture of {} levels",
            radiance.mip_count(),
        );
        assert_eq!(
            panorama.format(),
            TextureFormat::Rgba16Float,
            "an environment of panorama holds half float texels",
        );
        Self {
            radiance,
            panorama,
            sampler,
            levels,
            irradiance,
            intensity: 1.0,
            background: true,
            local: false,
            blend: 1.0,
            atmosphere: None,
        }
    }

    pub fn air(mut self, atmosphere: Atmosphere) -> Self {
        self.atmosphere = Some(atmosphere);
        self
    }

    pub fn lit(mut self, intensity: f32, background: bool) -> Self {
        assert!(
            intensity > 0.0 && intensity.is_finite(),
            "an environment of intensity {intensity} lights nothing",
        );
        self.intensity = intensity;
        self.background = background;
        self
    }

    pub fn local(mut self, blend: f32) -> Self {
        assert!(
            blend > 0.0 && blend.is_finite(),
            "a local environment of blend {blend} fades over no distance",
        );
        self.local = true;
        self.blend = blend;
        self
    }

    pub fn identity(&self) -> (usize, usize, usize) {
        (
            self.radiance.identity(),
            self.panorama.identity(),
            self.sampler.identity(),
        )
    }
}

use crate::environment::{BRDF_SIZE, Environment};
use crate::program;
use dome_gpu::{
    AddressMode, BindGroup, Binding, Device, Filter, GraphicsPipeline, Queue, Sampler, SamplerDesc,
    SamplerTable, Texture, TextureFormat, TextureTable, TextureUsages,
};
use half::f16;

pub(crate) struct Shading {
    fallback: Texture,
    radiance: Texture,
    panorama: Texture,
    sampler: Sampler,
    brdf: Texture,
    brdf_sampler: Sampler,
    identity: Option<(usize, usize, usize)>,
}

impl Shading {
    pub(crate) fn new(device: &Device, queue: &Queue) -> Self {
        let brdf = Texture::new(
            device,
            "dome brdf",
            BRDF_SIZE,
            BRDF_SIZE,
            1,
            TextureFormat::Rgba16Float,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        let codes = crate::environment::brdf(BRDF_SIZE)
            .iter()
            .flat_map(|texel| texel.iter())
            .map(|value| f16::from_f32(*value).to_bits())
            .collect::<Vec<u16>>();
        brdf.write(queue, bytemuck::cast_slice(&codes));
        let brdf_sampler = Sampler::new(
            device,
            SamplerDesc {
                min_filter: Filter::Linear,
                mag_filter: Filter::Linear,
                mip_filter: Filter::Nearest,
                address_u: AddressMode::ClampToEdge,
                address_v: AddressMode::ClampToEdge,
            },
        );
        let fallback = Texture::new(
            device,
            "dome radiance",
            1,
            1,
            1,
            TextureFormat::Rgba16Float,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        let clear = [0u16, 0u16, 0u16, f16::from_f32(1.0).to_bits()];
        fallback.write(queue, bytemuck::cast_slice(&clear));
        let sampler = Sampler::new(
            device,
            SamplerDesc {
                min_filter: Filter::Linear,
                mag_filter: Filter::Linear,
                mip_filter: Filter::Linear,
                address_u: AddressMode::Repeat,
                address_v: AddressMode::ClampToEdge,
            },
        );
        Self {
            radiance: fallback.clone(),
            panorama: fallback.clone(),
            fallback,
            sampler,
            brdf,
            brdf_sampler,
            identity: None,
        }
    }

    pub(crate) fn follow(&mut self, environment: Option<&Environment>) {
        let (radiance, panorama, sampler) = match environment {
            Some(held) => (
                held.radiance.clone(),
                held.panorama.clone(),
                held.sampler.clone(),
            ),
            None => (
                self.fallback.clone(),
                self.fallback.clone(),
                self.sampler.clone(),
            ),
        };
        let identity = (radiance.identity(), panorama.identity(), sampler.identity());
        if self.identity == Some(identity) {
            return;
        }
        self.radiance = radiance;
        self.panorama = panorama;
        self.sampler = sampler;
        self.identity = Some(identity);
    }

    pub(crate) fn identity(&self) -> Option<(usize, usize, usize)> {
        self.identity
    }

    pub(crate) fn group(
        &self,
        pipeline: &GraphicsPipeline,
        shadow: (&Texture, &Sampler),
        point: (&Texture, &Sampler),
        probes: (&TextureTable, &SamplerTable),
    ) -> BindGroup {
        pipeline.bind_group(
            2,
            &[
                Binding::texture(program::SHADOW_SHADOW, shadow.0),
                Binding::sampler(program::SHADOW_SAMPLER, shadow.1),
                Binding::texture(program::SHADE_RADIANCE, &self.radiance),
                Binding::sampler(program::SHADE_RADIANCE_SAMPLER, &self.sampler),
                Binding::texture(program::SHADE_BRDF, &self.brdf),
                Binding::sampler(program::SHADE_BRDF_SAMPLER, &self.brdf_sampler),
                Binding::texture(program::POINT_SHADOW_TEXTURE, point.0),
                Binding::sampler(program::POINT_SHADOW_SAMPLER, point.1),
                Binding::texture_table(program::PROBE_TEXTURES, probes.0),
                Binding::sampler_table(program::PROBE_SAMPLERS, probes.1),
            ],
        )
    }

    pub(crate) fn sky(&self, pipeline: &GraphicsPipeline) -> BindGroup {
        pipeline.bind_group(
            1,
            &[
                Binding::texture(program::SKY_RADIANCE, &self.panorama),
                Binding::sampler(program::SKY_SAMPLER, &self.sampler),
            ],
        )
    }

    pub(crate) fn fallback(&self) -> &Texture {
        &self.fallback
    }

    pub(crate) fn sampler(&self) -> &Sampler {
        &self.sampler
    }
}

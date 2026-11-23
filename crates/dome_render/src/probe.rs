use crate::abi::{PROBE, ProbeRecord};
use crate::environment::Environment;
use crate::growing::Growing;
use dome_gpu::{
    BufferBinding, BufferUsages, Device, Queue, Sampler, SamplerTable, Texture, TextureTable,
};
use dome_math::{Mat4, Transform, Vec3};
use dome_shader::{SAMPLER_TABLE_CAPACITY, TEXTURE_TABLE_CAPACITY};
use std::mem::size_of;

const PROBE_BYTES: u64 = size_of::<ProbeRecord>() as u64;
const CAPACITY: usize = 64;

pub(crate) struct Probes {
    textures: TextureTable,
    samplers: SamplerTable,
    records: Growing,
    identities: Vec<(usize, usize)>,
    fallback_texture: Texture,
    fallback_sampler: Sampler,
}

impl Probes {
    pub(crate) fn new(device: &Device, texture: &Texture, sampler: &Sampler) -> Self {
        Self {
            textures: TextureTable::new(device, TEXTURE_TABLE_CAPACITY, texture),
            samplers: SamplerTable::new(device, SAMPLER_TABLE_CAPACITY, sampler),
            records: Growing::new(
                device,
                "dome probes",
                BufferUsages::STORAGE | BufferUsages::COPY_DST,
            ),
            identities: Vec::new(),
            fallback_texture: texture.clone(),
            fallback_sampler: sampler.clone(),
        }
    }

    pub(crate) fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        locals: &[(&Environment, Transform)],
    ) -> (u32, bool) {
        assert!(
            locals.len() <= CAPACITY,
            "a frame of {} local environments outruns the {CAPACITY} probes its shader walks",
            locals.len(),
        );
        let identities = locals
            .iter()
            .map(|(environment, _)| {
                (
                    environment.radiance.identity(),
                    environment.sampler.identity(),
                )
            })
            .collect::<Vec<_>>();
        let changed = identities != self.identities;
        if changed {
            self.textures.reset(&self.fallback_texture);
            self.samplers.reset(&self.fallback_sampler);
            for (environment, _) in locals {
                self.textures.add(&environment.radiance);
                self.samplers.add(&environment.sampler);
            }
            self.identities = identities;
        }
        let bytes = locals.len() as u64 * PROBE_BYTES;
        self.records.fit(device, bytes);
        let records = locals
            .iter()
            .enumerate()
            .map(|(index, (environment, transform))| {
                record(environment, *transform, index as u32, index as u32)
            })
            .collect::<Vec<_>>();
        if !records.is_empty() {
            self.records
                .buffer()
                .write(queue, bytemuck::cast_slice(&records));
        }
        (locals.len() as u32, changed)
    }

    pub(crate) fn textures(&self) -> &TextureTable {
        &self.textures
    }

    pub(crate) fn samplers(&self) -> &SamplerTable {
        &self.samplers
    }

    pub(crate) fn record(&self, count: u32) -> BufferBinding<'_> {
        self.records
            .buffer()
            .binding(0, u64::from(count.max(1)) * PROBE_BYTES)
    }
}

fn record(
    environment: &Environment,
    transform: Transform,
    texture: u32,
    sampler: u32,
) -> ProbeRecord {
    let extent = transform.scale;
    assert!(
        extent.x > 0.0 && extent.y > 0.0 && extent.z > 0.0,
        "a local environment scales its box by {extent:?}",
    );
    let inverse = Mat4::from_quat(transform.rotation.normalize().inverse());
    ProbeRecord {
        center: vector(transform.translation, environment.blend),
        inverse0: column(inverse, 0),
        inverse1: column(inverse, 1),
        inverse2: column(inverse, 2),
        extent: vector(extent, environment.intensity),
        levels: (environment.levels - 1) as f32,
        texture,
        sampler,
        pad0: 0,
        irradiance0: vector(environment.irradiance[0], 0.0),
        irradiance1: vector(environment.irradiance[1], 0.0),
        irradiance2: vector(environment.irradiance[2], 0.0),
        irradiance3: vector(environment.irradiance[3], 0.0),
        irradiance4: vector(environment.irradiance[4], 0.0),
        irradiance5: vector(environment.irradiance[5], 0.0),
        irradiance6: vector(environment.irradiance[6], 0.0),
        irradiance7: vector(environment.irradiance[7], 0.0),
        irradiance8: vector(environment.irradiance[8], 0.0),
    }
}

fn vector(value: Vec3, w: f32) -> dome_shader::Fvec4 {
    dome_shader::Fvec4 {
        x: value.x,
        y: value.y,
        z: value.z,
        w,
    }
}

fn column(matrix: Mat4, index: usize) -> dome_shader::Fvec4 {
    let held = matrix.col(index);
    dome_shader::Fvec4 {
        x: held.x,
        y: held.y,
        z: held.z,
        w: held.w,
    }
}

const _: () = assert!(
    PROBE_BYTES == PROBE.size as u64,
    "the probe record keeps the layout its shader reads",
);

use crate::abi::{
    BakeLevelRecord, IRRADIANCE_PARTIALS, IRRADIANCE_TOTAL_BYTES, IrradianceRecord,
    IrradianceSetupRecord,
};
use crate::environment::{LEVELS, SPECULAR_WIDTH};
use crate::frame::vertical;
use crate::program;
use dome_gpu::{
    Binding, BufferUsages, ComputePipeline, Device, Draw, Gpu, GpuBuffer, GraphicsPipeline, Pass,
    PipelineState, Queue, Sampler, Submission, SubmissionIndex, Texture, TextureFormat,
    TextureUsages,
};
use dome_math::Vec3;
use std::mem::size_of;

const LEVEL_BYTES: u64 = size_of::<BakeLevelRecord>() as u64;
const SETUP_BYTES: u64 = size_of::<IrradianceSetupRecord>() as u64;
const PARTIAL_BYTES: u64 = size_of::<IrradianceRecord>() as u64;
const PARTIALS_BYTES: u64 = PARTIAL_BYTES * IRRADIANCE_PARTIALS as u64;
const FULLSCREEN_VERTICES: u32 = 3;
const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

pub struct Baked {
    pub radiance: Texture,
    pub levels: u32,
    pub irradiance: [Vec3; 9],
}

pub(crate) struct BakeJob {
    radiance: Texture,
    readback: GpuBuffer,
    submission: SubmissionIndex,
}

pub(crate) struct Baker {
    device: Device,
    queue: Queue,
    prefilter: GraphicsPipeline,
    integration: GraphicsPipeline,
    reduction: ComputePipeline,
    accumulation: Texture,
    levels: GpuBuffer,
    level_span: u64,
    setup: GpuBuffer,
    partials: GpuBuffer,
    total: GpuBuffer,
}

impl Baker {
    pub(crate) fn new(context: &Gpu) -> Self {
        let device = context.device().clone();
        let storage = BufferUsages::STORAGE;
        let level_span = crate::span::padded(device.binding_alignment(), LEVEL_BYTES);
        Self {
            prefilter: GraphicsPipeline::new(
                &device,
                program::prefiltering(),
                TextureFormat::Rgba16Float,
                PipelineState::SCREEN,
            ),
            integration: GraphicsPipeline::new(
                &device,
                program::integrating(),
                TextureFormat::R32Float,
                PipelineState::SCREEN,
            ),
            reduction: ComputePipeline::new(&device, program::irradiance_reduction_program()),
            accumulation: Texture::new(
                &device,
                "dome irradiance",
                IRRADIANCE_PARTIALS,
                1,
                1,
                TextureFormat::R32Float,
                TextureUsages::RENDER_ATTACHMENT,
            ),
            levels: GpuBuffer::new(
                &device,
                "dome bake levels",
                u64::from(LEVELS) * level_span,
                storage | BufferUsages::COPY_DST,
            ),
            level_span,
            setup: GpuBuffer::new(
                &device,
                "dome bake setup",
                SETUP_BYTES,
                storage | BufferUsages::COPY_DST,
            ),
            partials: GpuBuffer::new(&device, "dome bake partials", PARTIALS_BYTES, storage),
            total: GpuBuffer::new(
                &device,
                "dome bake irradiance",
                IRRADIANCE_TOTAL_BYTES,
                storage | BufferUsages::COPY_SRC,
            ),
            queue: Queue::of(&device),
            device,
        }
    }

    pub(crate) fn submit(&self, panorama: &Texture, sampler: &Sampler) -> BakeJob {
        assert_eq!(
            panorama.format(),
            TextureFormat::Rgba16Float,
            "a baked environment samples half float panorama texels",
        );
        assert!(
            panorama.usages().contains(TextureUsages::TEXTURE_BINDING),
            "a baked environment samples its panorama",
        );
        let width = panorama.width();
        let height = panorama.height();
        let texels = u64::from(width) * u64::from(height);
        assert!(
            texels <= u64::from(u32::MAX),
            "a {width} by {height} panorama outruns the texel count a bake walks",
        );
        let vertical = vertical();
        let radiance = Texture::new(
            &self.device,
            "dome radiance",
            SPECULAR_WIDTH,
            SPECULAR_WIDTH / 2,
            LEVELS,
            TextureFormat::Rgba16Float,
            TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
        );
        let levels = (0..LEVELS)
            .map(|level| BakeLevelRecord {
                roughness: level as f32 / (LEVELS - 1) as f32,
                vertical,
                pad0: 0,
                pad1: 0,
            })
            .collect::<Vec<_>>();
        let mut level_bytes = vec![0u8; (u64::from(LEVELS) * self.level_span) as usize];
        for (index, level) in levels.iter().enumerate() {
            let start = index * self.level_span as usize;
            level_bytes[start..start + LEVEL_BYTES as usize]
                .copy_from_slice(bytemuck::bytes_of(level));
        }
        self.levels.write(&self.queue, &level_bytes);
        self.setup.write(
            &self.queue,
            bytemuck::bytes_of(&IrradianceSetupRecord {
                width,
                height,
                texels: texels as u32,
                partials: IRRADIANCE_PARTIALS,
            }),
        );
        let readback = GpuBuffer::new(
            &self.device,
            "dome bake readback",
            IRRADIANCE_TOTAL_BYTES,
            BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        );
        let mut submission = Submission::new(&self.device, "dome environment");
        let source = [
            Binding::texture(program::PREFILTER_PANORAMA, panorama),
            Binding::sampler(program::PREFILTER_SAMPLER, sampler),
        ];
        let source = self.prefilter.bind_group(1, &source);
        for level in 0..LEVELS {
            let level_group = self.prefilter.bind_group(
                0,
                &[Binding::buffer(
                    program::PREFILTER_LEVEL,
                    self.levels
                        .binding(u64::from(level) * self.level_span, LEVEL_BYTES),
                )],
            );
            let groups = [level_group, source.clone()];
            submission.render(
                Pass::new(&radiance, CLEAR).level(level),
                &[Draw::new(&self.prefilter, &groups, FULLSCREEN_VERTICES)],
            );
        }
        let integration_source = [
            Binding::texture(program::INTEGRATION_PANORAMA, panorama),
            Binding::sampler(program::INTEGRATION_SAMPLER, sampler),
        ];
        let integration = [
            self.integration.bind_group(
                0,
                &[
                    Binding::buffer(
                        program::INTEGRATION_SETUP,
                        self.setup.binding(0, SETUP_BYTES),
                    ),
                    Binding::buffer(
                        program::INTEGRATION_PARTIALS,
                        self.partials.binding(0, PARTIALS_BYTES),
                    ),
                ],
            ),
            self.integration.bind_group(1, &integration_source),
        ];
        submission.render(
            Pass::new(&self.accumulation, CLEAR),
            &[Draw::new(
                &self.integration,
                &integration,
                FULLSCREEN_VERTICES,
            )],
        );
        let reduction = self.reduction.bind_group(
            0,
            &[
                Binding::buffer(
                    program::REDUCTION_PARTIALS,
                    self.partials.binding(0, PARTIALS_BYTES),
                ),
                Binding::buffer(
                    program::REDUCTION_TOTAL,
                    self.total.binding(0, IRRADIANCE_TOTAL_BYTES),
                ),
            ],
        );
        submission.dispatch(&self.reduction, &reduction, [1, 1, 1]);
        submission.copy(&self.total, 0, &readback, 0, IRRADIANCE_TOTAL_BYTES);
        let submission = submission.submit(&self.queue);
        BakeJob {
            radiance,
            readback,
            submission,
        }
    }

    pub(crate) fn take(&self, job: &BakeJob) -> Option<Baked> {
        if !self.queue.complete(job.submission.clone()).unwrap_or(false) {
            return None;
        }
        Some(self.collect(job))
    }

    pub(crate) fn wait(&self, job: &BakeJob) -> Baked {
        self.queue
            .wait(job.submission.clone(), dome_gpu::READBACK_TIMEOUT)
            .unwrap_or_else(|failure| panic!("{failure}"));
        self.collect(job)
    }

    fn collect(&self, job: &BakeJob) -> Baked {
        let bytes = job
            .readback
            .read(&self.queue, job.submission.clone(), IRRADIANCE_TOTAL_BYTES)
            .unwrap_or_else(|failure| panic!("{failure}"));
        let mut irradiance = [Vec3::ZERO; 9];
        for (coefficient, held) in irradiance
            .iter_mut()
            .zip(bytes.as_chunks::<4>().0.as_chunks::<3>().0)
        {
            *coefficient = Vec3::new(
                f32::from_ne_bytes(held[0]),
                f32::from_ne_bytes(held[1]),
                f32::from_ne_bytes(held[2]),
            );
        }
        Baked {
            radiance: job.radiance.clone(),
            levels: LEVELS,
            irradiance,
        }
    }
}

const _: () = assert!(
    PARTIAL_BYTES == 144 && PARTIALS_BYTES == 147456,
    "a bake partial spans the nine padded coefficients its shader writes",
);
const _: () = assert!(
    SETUP_BYTES == 16 && LEVEL_BYTES == 16,
    "a bake setup and level record sit on a storage binding word",
);

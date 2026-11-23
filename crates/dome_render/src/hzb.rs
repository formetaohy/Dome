use crate::abi::{HZB_LEVEL, HZB_SCALE, HZB_STEP, HzbLevelRecord, HzbStepRecord};
use crate::program;
use dome_gpu::{
    BindGroup, Binding, BufferBinding, BufferUsages, Device, Draw, GpuBuffer, GraphicsPipeline,
    Pass, PipelineHandle, Queue, Sampler, Submission, Texture, TextureFormat, TextureUsages,
    Viewport,
};
use std::mem::size_of;

const HZB_LEVEL_BYTES: u64 = size_of::<HzbLevelRecord>() as u64;
const HZB_STEP_BYTES: u64 = size_of::<HzbStepRecord>() as u64;
const HZB_LEVEL_ALIGN: u64 = 16;
const WORD_BYTES: u64 = 4;
const FULLSCREEN_VERTICES: u32 = 3;
const CLEAR: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

pub(crate) struct Hzb {
    width: u32,
    height: u32,
    depth: Texture,
    scratch: Texture,
    records: GpuBuffer,
    depths: GpuBuffer,
    base: [BindGroup; 2],
    reductions: Vec<BindGroup>,
    dimensions: Vec<(u32, u32)>,
}

impl Hzb {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        device: &Device,
        queue: &Queue,
        base_pipeline: &GraphicsPipeline,
        reduce_pipeline: &GraphicsPipeline,
        sampler: &Sampler,
        width: u32,
        height: u32,
        vertical: f32,
    ) -> Self {
        let mut dimensions = Vec::new();
        let mut current = (width.div_ceil(HZB_SCALE), height.div_ceil(HZB_SCALE));
        loop {
            dimensions.push(current);
            if current.0 == 1 && current.1 == 1 {
                break;
            }
            current = (current.0.div_ceil(2), current.1.div_ceil(2));
        }
        let depth = Texture::new(
            device,
            "dome depth",
            width,
            height,
            1,
            TextureFormat::R32Float,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::SAMPLED,
        );
        let scratch = Texture::new(
            device,
            "dome hzb scratch",
            dimensions[0].0,
            dimensions[0].1,
            1,
            TextureFormat::R32Float,
            TextureUsages::RENDER_ATTACHMENT,
        );
        let mut records = Vec::with_capacity(dimensions.len());
        let mut offsets = Vec::with_capacity(dimensions.len());
        let mut strides = Vec::with_capacity(dimensions.len());
        let mut total = 0u64;
        for (level_width, level_height) in &dimensions {
            let offset = total.next_multiple_of(HZB_LEVEL_ALIGN);
            let stride = u64::from(*level_width);
            records.push(HzbLevelRecord {
                offset: (offset / WORD_BYTES) as u32,
                width: *level_width,
                height: *level_height,
                stride: stride as u32,
            });
            offsets.push((offset / WORD_BYTES) as u32);
            strides.push(stride as u32);
            total = offset + u64::from(*level_height) * stride * WORD_BYTES;
        }
        assert!(
            total <= device.limits().max_storage_buffer_binding_size,
            "a {width} by {height} depth pyramid of {total} bytes outruns the device storage binding"
        );
        let depths = GpuBuffer::new(device, "dome hzb depths", total, BufferUsages::STORAGE);
        let level_records = GpuBuffer::new(
            device,
            "dome hzb levels",
            dimensions.len() as u64 * HZB_LEVEL_BYTES,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        level_records.write(queue, bytemuck::cast_slice(&records));
        let steps = dimensions
            .iter()
            .enumerate()
            .map(|(index, (level_width, level_height))| HzbStepRecord {
                source_width: if index == 0 {
                    width as f32
                } else {
                    dimensions[index - 1].0 as f32
                },
                source_height: if index == 0 {
                    height as f32
                } else {
                    dimensions[index - 1].1 as f32
                },
                target_width: *level_width as f32,
                target_height: *level_height as f32,
                vertical,
                source_offset: if index == 0 { 0 } else { offsets[index - 1] },
                source_stride: if index == 0 { 0 } else { strides[index - 1] },
                target_offset: offsets[index],
                target_stride: strides[index],
            })
            .collect::<Vec<_>>();
        let alignment = device
            .limits()
            .min_storage_buffer_offset_alignment
            .max(WORD_BYTES);
        let stride = HZB_STEP_BYTES.next_multiple_of(alignment);
        let mut bytes = vec![0u8; steps.len() * stride as usize];
        for (index, step) in steps.iter().enumerate() {
            let start = index * stride as usize;
            bytes[start..start + HZB_STEP_BYTES as usize].copy_from_slice(bytemuck::bytes_of(step));
        }
        let step_records = GpuBuffer::new(
            device,
            "dome hzb steps",
            bytes.len() as u64,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        step_records.write(queue, &bytes);
        let depths_binding = depths.binding(0, depths.size());
        let step_binding =
            |index: usize| step_records.binding(index as u64 * stride, HZB_STEP_BYTES);
        let base = [
            base_pipeline.bind_group(
                0,
                &[
                    Binding::buffer(program::HZB_STEP_BINDING, step_binding(0)),
                    Binding::buffer(program::HZB_DEPTHS_BINDING, depths_binding),
                ],
            ),
            base_pipeline.bind_group(
                1,
                &[
                    Binding::texture(program::HZB_SOURCE_BINDING, &depth),
                    Binding::sampler(program::HZB_SAMPLER_BINDING, sampler),
                ],
            ),
        ];
        let reductions = (1..dimensions.len())
            .map(|index| {
                reduce_pipeline.bind_group(
                    0,
                    &[
                        Binding::buffer(program::HZB_STEP_BINDING, step_binding(index)),
                        Binding::buffer(program::HZB_DEPTHS_BINDING, depths_binding),
                    ],
                )
            })
            .collect::<Vec<_>>();
        Self {
            width,
            height,
            depth,
            scratch,
            records: level_records,
            depths,
            base,
            reductions,
            dimensions,
        }
    }

    pub(crate) fn holds(&self, width: u32, height: u32) -> bool {
        self.width == width && self.height == height
    }

    pub(crate) fn depth(&self) -> &Texture {
        &self.depth
    }

    pub(crate) fn levels(&self) -> u32 {
        self.dimensions.len() as u32
    }

    pub(crate) fn build(
        &self,
        submission: &mut Submission,
        base_pipeline: &GraphicsPipeline,
        reduce_pipeline: &GraphicsPipeline,
    ) {
        submission.render(
            Pass::new(&self.scratch, CLEAR),
            &[Draw::new(base_pipeline, &self.base, FULLSCREEN_VERTICES)],
        );
        for (index, group) in self.reductions.iter().enumerate() {
            let (width, height) = self.dimensions[index + 1];
            submission.render(
                Pass::new(&self.scratch, CLEAR),
                &[Draw::new(
                    reduce_pipeline,
                    std::slice::from_ref(group),
                    FULLSCREEN_VERTICES,
                )
                .viewport(Viewport::new(0.0, 0.0, width as f32, height as f32))],
            );
        }
    }

    pub(crate) fn cull(
        &self,
        pipeline: &PipelineHandle,
        view: BufferBinding<'_>,
        instances: BufferBinding<'_>,
        drawn: BufferBinding<'_>,
        arguments: BufferBinding<'_>,
    ) -> BindGroup {
        pipeline.bind_group(
            0,
            &[
                Binding::buffer(program::CULL_VIEW, view),
                Binding::buffer(program::CULL_INSTANCES, instances),
                Binding::buffer(program::CULL_DRAWN, drawn),
                Binding::buffer(program::CULL_ARGUMENTS, arguments),
                Binding::buffer(
                    program::CULL_LEVELS,
                    self.records
                        .binding(0, self.dimensions.len() as u64 * HZB_LEVEL_BYTES),
                ),
                Binding::buffer(
                    program::CULL_DEPTHS,
                    self.depths.binding(0, self.depths.size()),
                ),
            ],
        )
    }
}

const _: () = assert!(
    HZB_LEVEL_BYTES == size_of::<HzbLevelRecord>() as u64,
    "the depth level record keeps the layout its shader reads",
);
const _: () = assert!(
    HZB_LEVEL.size as u64 == HZB_LEVEL_BYTES,
    "the depth level layout spans its record",
);
const _: () = assert!(
    HZB_STEP_BYTES == size_of::<HzbStepRecord>() as u64,
    "the depth step record keeps the layout its shader reads",
);
const _: () = assert!(
    HZB_STEP.size as u64 == HZB_STEP_BYTES,
    "the depth step layout spans its record",
);

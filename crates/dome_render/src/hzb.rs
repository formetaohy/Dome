use crate::program;
use dome_gpu::{
    BindGroup, Binding, BufferBinding, BufferUsages, ComputePipeline, Device, Draw, GpuBuffer,
    GraphicsPipeline, Pass, Queue, Sampler, Submission, Texture, TextureFormat, TextureUsages,
};
use std::mem::size_of;

const FULLSCREEN_VERTICES: u32 = 3;
const CLEAR: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const ORIENTATION_BYTES: u64 = size_of::<f32>() as u64;

pub(crate) struct Hzb {
    width: u32,
    height: u32,
    texture: Texture,
    levels: Vec<Texture>,
    orientation: GpuBuffer,
    reductions: Vec<BindGroup>,
}

impl Hzb {
    pub(crate) fn new(
        device: &Device,
        queue: &Queue,
        reduce_pipeline: &GraphicsPipeline,
        sampler: &Sampler,
        width: u32,
        height: u32,
        vertical: f32,
    ) -> Self {
        let mut dimensions = Vec::new();
        let mut current = (
            width.div_ceil(crate::abi::HZB_SCALE),
            height.div_ceil(crate::abi::HZB_SCALE),
        );
        loop {
            dimensions.push(current);
            if current.0 == 1 && current.1 == 1 {
                break;
            }
            current = (current.0.div_ceil(2), current.1.div_ceil(2));
        }
        let texture = Texture::new(
            device,
            "dome depth pyramid",
            dimensions[0].0,
            dimensions[0].1,
            dimensions.len() as u32,
            TextureFormat::R32Float,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        );
        let levels = (0..dimensions.len() as u32)
            .map(|level| texture.level(level))
            .collect::<Vec<_>>();
        let orientation = GpuBuffer::new(
            device,
            "dome depth orientation",
            ORIENTATION_BYTES,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        orientation.write(queue, bytemuck::bytes_of(&vertical));
        let reductions = (1..levels.len())
            .map(|level| {
                reduce_pipeline.bind_group(
                    1,
                    &[
                        Binding::texture(program::HZB_SOURCE_BINDING, &levels[level - 1]),
                        Binding::sampler(program::HZB_SAMPLER_BINDING, sampler),
                    ],
                )
            })
            .collect::<Vec<_>>();
        Self {
            width,
            height,
            texture,
            levels,
            orientation,
            reductions,
        }
    }

    pub(crate) fn holds(&self, width: u32, height: u32) -> bool {
        self.width == width && self.height == height
    }

    pub(crate) fn levels(&self) -> u32 {
        self.levels.len() as u32
    }

    pub(crate) fn build(
        &self,
        submission: &mut Submission,
        base_pipeline: &GraphicsPipeline,
        reduce_pipeline: &GraphicsPipeline,
        depth: &Texture,
        sampler: &Sampler,
    ) {
        let orientation = self.orientation.binding(0, ORIENTATION_BYTES);
        let base = [
            base_pipeline.bind_group(
                0,
                &[Binding::buffer(
                    program::HZB_ORIENTATION_BINDING,
                    orientation,
                )],
            ),
            base_pipeline.bind_group(
                1,
                &[
                    Binding::texture(program::HZB_SOURCE_BINDING, depth),
                    Binding::sampler(program::HZB_SAMPLER_BINDING, sampler),
                ],
            ),
        ];
        submission.render(
            Pass::new(&self.texture, CLEAR),
            &[Draw::new(base_pipeline, &base, FULLSCREEN_VERTICES)],
        );
        let shared = reduce_pipeline.bind_group(
            0,
            &[Binding::buffer(
                program::HZB_ORIENTATION_BINDING,
                orientation,
            )],
        );
        for (level, group) in self.reductions.iter().enumerate() {
            submission.render(
                Pass::new(&self.texture, CLEAR).level(level as u32 + 1),
                &[Draw::new(
                    reduce_pipeline,
                    &[shared.clone(), group.clone()],
                    FULLSCREEN_VERTICES,
                )],
            );
        }
    }

    pub(crate) fn cull(
        &self,
        pipeline: &ComputePipeline,
        sampler: &Sampler,
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
                Binding::texture(program::CULL_HZB, &self.texture),
                Binding::sampler(program::CULL_FILTER, sampler),
            ],
        )
    }
}

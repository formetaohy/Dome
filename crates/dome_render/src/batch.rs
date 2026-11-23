use crate::abi::SHADOW_CASCADES;
use crate::program;
use dome_gpu::{BindGroup, Binding, BufferBinding, GraphicsPipeline, SamplerTable, TextureTable};

const CASCADES: usize = SHADOW_CASCADES as usize;

pub(crate) struct Pipelines<'a> {
    pub(crate) depth: &'a GraphicsPipeline,
    pub(crate) opaque: &'a GraphicsPipeline,
    pub(crate) capture: &'a GraphicsPipeline,
    pub(crate) blended: &'a GraphicsPipeline,
    pub(crate) motion: &'a GraphicsPipeline,
    pub(crate) motion_blended: &'a GraphicsPipeline,
    pub(crate) shadow: &'a GraphicsPipeline,
}

pub(crate) struct Shared<'a> {
    pub(crate) view: BufferBinding<'a>,
    pub(crate) motion_view: BufferBinding<'a>,
    pub(crate) palette: BufferBinding<'a>,
    pub(crate) materials: BufferBinding<'a>,
    pub(crate) light: BufferBinding<'a>,
    pub(crate) cascades: BufferBinding<'a>,
    pub(crate) lighting: BufferBinding<'a>,
    pub(crate) probes: BufferBinding<'a>,
    pub(crate) keys: BufferBinding<'a>,
    pub(crate) colors: BufferBinding<'a>,
    pub(crate) dropped: BufferBinding<'a>,
    pub(crate) settings: BufferBinding<'a>,
}

pub(crate) struct Slot<'a> {
    pub(crate) vertices: BufferBinding<'a>,
    pub(crate) skins: BufferBinding<'a>,
    pub(crate) depth_drawn: BufferBinding<'a>,
    pub(crate) main_drawn: BufferBinding<'a>,
    pub(crate) shadow_drawn: [BufferBinding<'a>; CASCADES],
}

pub(crate) struct SlotGroups {
    depth: Option<BindGroup>,
    shade: BindGroup,
    motion: BindGroup,
    shadow: [BindGroup; CASCADES],
}

impl SlotGroups {
    pub(crate) fn build(
        pipelines: &Pipelines<'_>,
        shared: &Shared<'_>,
        slot: &Slot<'_>,
        blended: bool,
    ) -> Self {
        let depth = (!blended).then(|| {
            pipelines.depth.bind_group(
                0,
                &[
                    Binding::buffer(program::DEPTH_VIEW, shared.view),
                    Binding::buffer(program::DEPTH_PALETTE, shared.palette),
                    Binding::buffer(program::DEPTH_SKINS, slot.skins),
                    Binding::buffer(program::DEPTH_DRAWN, slot.depth_drawn),
                    Binding::buffer(program::DEPTH_VERTICES, slot.vertices),
                    Binding::buffer(program::DEPTH_MATERIALS, shared.materials),
                ],
            )
        });
        let pipeline = if blended {
            pipelines.blended
        } else {
            pipelines.opaque
        };
        let shade = pipeline.bind_group(
            0,
            &[
                Binding::buffer(program::DRAW_LIGHT, shared.light),
                Binding::buffer(program::DRAW_CASCADES, shared.cascades),
                Binding::buffer(program::DRAW_DRAWN, slot.main_drawn),
                Binding::buffer(program::DRAW_MATERIALS, shared.materials),
                Binding::buffer(program::DRAW_VIEW, shared.view),
                Binding::buffer(program::DRAW_PALETTE, shared.palette),
                Binding::buffer(program::DRAW_SKINS, slot.skins),
                Binding::buffer(program::DRAW_FRAGMENTS, shared.keys),
                Binding::buffer(program::DRAW_COLORS, shared.colors),
                Binding::buffer(program::DRAW_DROPPED, shared.dropped),
                Binding::buffer(program::DRAW_SETTINGS, shared.settings),
                Binding::buffer(program::DRAW_PROBES, shared.probes),
                Binding::buffer(program::DRAW_VERTICES, slot.vertices),
                Binding::buffer(program::DRAW_LIGHTS, shared.lighting),
            ],
        );
        let motion_pipeline = if blended {
            pipelines.motion_blended
        } else {
            pipelines.motion
        };
        let motion = motion_pipeline.bind_group(
            0,
            &[
                Binding::buffer(program::MOTION_VIEW, shared.motion_view),
                Binding::buffer(program::MOTION_DRAWN, slot.main_drawn),
                Binding::buffer(program::MOTION_PALETTE, shared.palette),
                Binding::buffer(program::MOTION_SKINS, slot.skins),
                Binding::buffer(program::MOTION_MATERIALS, shared.materials),
                Binding::buffer(program::MOTION_VERTICES, slot.vertices),
            ],
        );
        let shadow = std::array::from_fn(|cascade| {
            pipelines.shadow.bind_group(
                0,
                &[
                    Binding::buffer(program::SHADOW_PALETTE, shared.palette),
                    Binding::buffer(program::SHADOW_SKINS, slot.skins),
                    Binding::buffer(program::SHADOW_DRAWN, slot.shadow_drawn[cascade]),
                    Binding::buffer(program::SHADOW_VERTICES, slot.vertices),
                    Binding::buffer(program::SHADOW_MATERIALS, shared.materials),
                ],
            )
        });
        Self {
            depth,
            shade,
            motion,
            shadow,
        }
    }

    pub(crate) fn depth(&self) -> Option<&BindGroup> {
        self.depth.as_ref()
    }

    pub(crate) fn shade(&self) -> &BindGroup {
        &self.shade
    }

    pub(crate) fn motion(&self) -> &BindGroup {
        &self.motion
    }

    pub(crate) fn shadow(&self, cascade: usize) -> &BindGroup {
        &self.shadow[cascade]
    }
}

pub(crate) fn table(
    pipeline: &GraphicsPipeline,
    texture: u32,
    sampler: u32,
    textures: &TextureTable,
    samplers: &SamplerTable,
) -> BindGroup {
    pipeline.bind_group(
        1,
        &[
            Binding::texture_table(texture, textures),
            Binding::sampler_table(sampler, samplers),
        ],
    )
}

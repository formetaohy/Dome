use super::*;
use dome_gpu::{BindGroup, BufferRange, SamplerTable, TextureTable};

const CASCADES: usize = SHADOW_CASCADES as usize;
const SHARED_RANGES: usize = 19;

#[derive(Clone, Copy, PartialEq, Eq)]
struct SlotKey {
    vertices: BufferRange,
    skins: BufferRange,
    depth_visible: BufferRange,
    main_visible: BufferRange,
    shadow_visible: [BufferRange; CASCADES],
    blended: bool,
}

impl SlotKey {
    fn of(slot: &Slot<'_>, blended: bool) -> Self {
        Self {
            vertices: slot.vertices.range(),
            skins: slot.skins.range(),
            depth_visible: slot.depth_visible.range(),
            main_visible: slot.main_visible.range(),
            shadow_visible: std::array::from_fn(|cascade| slot.shadow_visible[cascade].range()),
            blended,
        }
    }
}

pub(super) struct SceneGroups {
    shared: [BufferRange; SHARED_RANGES],
    slots: Vec<SlotKey>,
    generations: (u64, u64),
    pub(super) groups: Vec<SlotGroups>,
    pub(super) depth_tables: BindGroup,
    pub(super) shadow_tables: BindGroup,
    pub(super) tables: BindGroup,
    pub(super) transparent_tables: BindGroup,
    pub(super) motion_tables: BindGroup,
}

impl SceneGroups {
    pub(super) fn matches(
        &self,
        shared: &Shared<'_>,
        slots: &[Slot<'_>],
        batches: &[Batch],
        generations: (u64, u64),
    ) -> bool {
        self.generations == generations
            && self.shared == shared_ranges(shared)
            && self.slots.len() == slots.len()
            && self
                .slots
                .iter()
                .zip(slots.iter().zip(batches))
                .all(|(held, (slot, batch))| *held == SlotKey::of(slot, batch.blended))
    }

    pub(super) fn build(
        pipelines: &Pipelines<'_>,
        shared: &Shared<'_>,
        slots: &[Slot<'_>],
        batches: &[Batch],
        textures: &TextureTable,
        samplers: &SamplerTable,
    ) -> Self {
        let groups = slots
            .iter()
            .zip(batches)
            .map(|(slot, batch)| SlotGroups::build(pipelines, shared, slot, batch.blended))
            .collect::<Vec<_>>();
        let depth_tables = table(
            pipelines.depth,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        );
        let shadow_tables = table(
            pipelines.shadow,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        );
        let tables = table(
            pipelines.opaque,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        );
        let transparent_tables = table(
            pipelines.blended,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        );
        let motion_tables = table(
            pipelines.motion,
            program::MOTION_TEXTURE,
            program::MOTION_SAMPLER,
            textures,
            samplers,
        );
        Self {
            shared: shared_ranges(shared),
            slots: slots
                .iter()
                .zip(batches)
                .map(|(slot, batch)| SlotKey::of(slot, batch.blended))
                .collect(),
            generations: (textures.generation(), samplers.generation()),
            groups,
            depth_tables,
            shadow_tables,
            tables,
            transparent_tables,
            motion_tables,
        }
    }
}

fn shared_ranges(shared: &Shared<'_>) -> [BufferRange; SHARED_RANGES] {
    [
        shared.view.range(),
        shared.motion_view.range(),
        shared.palette.range(),
        shared.materials.range(),
        shared.light.range(),
        shared.cascades.range(),
        shared.lighting.range(),
        shared.point_light.range(),
        shared.probes.range(),
        shared.keys.range(),
        shared.colors.range(),
        shared.dropped.range(),
        shared.settings.range(),
        shared.instances.range(),
        shared.depth_view.range(),
        shared.shadow_views[0].range(),
        shared.shadow_views[1].range(),
        shared.shadow_views[2].range(),
        shared.shadow_views[3].range(),
    ]
}

use crate::buffer::{BufferBinding, GpuBuffer};
use crate::capability::{BufferUsages, TextureUsages};
use crate::context::Device;
use crate::graphics::PipelineState;
use crate::group::{GroupKey, ResourceKey};
use crate::identity::ResourceId;
use crate::native::{NativeGraphicsPipeline, NativeGroup, NativePipeline};
use crate::sampler::Sampler;
use crate::table::{SamplerTable, TextureTable};
use crate::texture::Texture;
use dome_shader::shader::{BindingKind, BindingSpec, ComputeProgram, GraphicsProgram};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub enum Resource<'a> {
    Buffer(BufferBinding<'a>),
    Texture(&'a Texture),
    Sampler(&'a Sampler),
    TextureTable(&'a TextureTable),
    SamplerTable(&'a SamplerTable),
}

#[derive(Clone, Copy)]
pub struct Binding<'a> {
    pub index: u32,
    pub resource: Resource<'a>,
}

impl<'a> Binding<'a> {
    pub fn buffer(index: u32, binding: BufferBinding<'a>) -> Self {
        Self {
            index,
            resource: Resource::Buffer(binding),
        }
    }

    pub fn texture(index: u32, texture: &'a Texture) -> Self {
        Self {
            index,
            resource: Resource::Texture(texture),
        }
    }

    pub fn sampler(index: u32, sampler: &'a Sampler) -> Self {
        Self {
            index,
            resource: Resource::Sampler(sampler),
        }
    }

    pub fn texture_table(index: u32, table: &'a TextureTable) -> Self {
        Self {
            index,
            resource: Resource::TextureTable(table),
        }
    }

    pub fn sampler_table(index: u32, table: &'a SamplerTable) -> Self {
        Self {
            index,
            resource: Resource::SamplerTable(table),
        }
    }

    pub(crate) fn key(&self) -> ResourceKey {
        match self.resource {
            Resource::Buffer(binding) => ResourceKey::buffer(binding.range()),
            Resource::Texture(texture) => ResourceKey::texture(texture.identity()),
            Resource::Sampler(sampler) => ResourceKey::sampler(sampler.identity()),
            Resource::TextureTable(table) => ResourceKey::textures(table),
            Resource::SamplerTable(table) => ResourceKey::samplers(table),
        }
    }
}

#[derive(Clone)]
pub(crate) struct BoundBuffer {
    pub(crate) buffer: GpuBuffer,
    pub(crate) offset: u64,
    #[cfg(vulkan_backend)]
    pub(crate) size: u64,
    #[cfg(vulkan_backend)]
    pub(crate) writable: bool,
}

#[derive(Clone)]
pub(crate) enum BoundResource {
    Buffer(BoundBuffer),
    Texture(Texture),
    Sampler(Sampler),
    TextureTable(TextureTable),
    SamplerTable(SamplerTable),
}

pub(crate) struct Slot {
    pub(crate) native: NativePipeline,
    pub(crate) program: Arc<ComputeProgram>,
    pub(crate) device: Device,
    identity: ResourceId,
}

pub(crate) struct GraphicsSlot {
    pub(crate) native: NativeGraphicsPipeline,
    pub(crate) program: Arc<GraphicsProgram>,
    pub(crate) device: Device,
    pub(crate) format: Option<TextureFormat>,
    pub(crate) depth: bool,
    identity: ResourceId,
}

#[derive(Clone)]
pub(crate) enum Owner {
    Compute(Arc<Slot>),
    Graphics(Arc<GraphicsSlot>),
}

#[derive(Clone)]
pub struct PipelineHandle {
    pub(crate) slot: Arc<Slot>,
}

use crate::texture::TextureFormat;

#[derive(Clone)]
pub struct GraphicsPipeline {
    pub(crate) slot: Arc<GraphicsSlot>,
}

#[derive(Clone)]
pub struct BindGroup {
    pub(crate) native: NativeGroup,
    pub(crate) group: u32,
    #[cfg(metal_backend)]
    pub(crate) resources: Vec<BoundResource>,
    pub(crate) owner: Owner,
}

impl PipelineHandle {
    pub(crate) fn new(device: &Device, program: Arc<ComputeProgram>) -> Self {
        assert!(
            program
                .bindings()
                .iter()
                .filter(|binding| binding.kind.storage())
                .count() as u32
                <= device.limits().max_storage_buffers_per_shader_stage
        );
        let workgroup = program.workgroup_size();
        assert!(
            workgroup <= device.limits().max_compute_workgroup_size_x
                && workgroup <= device.limits().max_compute_invocations_per_workgroup,
            "the kernel {} dispatches {workgroup} invocations the device cannot run",
            program.label(),
        );
        let native = device.native().create_pipeline(&program);
        let handle = Self {
            slot: Arc::new(Slot {
                device: device.clone(),
                program,
                native,
                identity: ResourceId::next(),
            }),
        };
        handle.compile();
        handle
    }

    pub fn label(&self) -> &str {
        self.slot.program.label()
    }

    pub fn bind_group(&self, group: u32, entries: &[Binding<'_>]) -> BindGroup {
        let label = self.slot.program.label();
        let specs = group_specs(self.slot.program.bindings(), group, label);
        let owner = self.slot.identity;
        let key = GroupKey::new(owner, group, entries.iter().map(|entry| entry.key()));
        let device = &self.slot.device;
        let submission = device.recording();
        device.state.groups.fetch(key, submission, || {
            let resources = bounded(device, &specs, entries, label);
            let native = device
                .native()
                .create_group(&self.slot.native, group, &resources);
            BindGroup {
                native,
                group,
                #[cfg(metal_backend)]
                resources,
                owner: Owner::Compute(self.slot.clone()),
            }
        })
    }

    fn compile(&self) {
        self.slot.device.native().compile(
            &self.slot.native,
            &self.slot.program,
            self.slot.device.artifact_cache(),
        );
    }
}

impl GraphicsPipeline {
    pub fn new(
        device: &Device,
        program: GraphicsProgram,
        format: TextureFormat,
        state: PipelineState,
    ) -> Self {
        Self::layered(device, program, Some(format), state, false)
    }

    pub fn new_depth(
        device: &Device,
        program: GraphicsProgram,
        format: TextureFormat,
        state: PipelineState,
    ) -> Self {
        Self::layered(device, program, Some(format), state, true)
    }

    pub fn new_depth_only(device: &Device, program: GraphicsProgram, state: PipelineState) -> Self {
        Self::layered(device, program, None, state, true)
    }

    fn layered(
        device: &Device,
        program: GraphicsProgram,
        format: Option<TextureFormat>,
        state: PipelineState,
        depth: bool,
    ) -> Self {
        assert!(
            program
                .bindings()
                .iter()
                .filter(|binding| binding.kind.storage())
                .count() as u32
                <= device.limits().max_storage_buffers_per_shader_stage
        );
        assert_eq!(
            program.targets(),
            u32::from(format.is_some()),
            "the graphics program {} writes {} fragment targets into {} colour attachment",
            program.label(),
            program.targets(),
            if format.is_some() { "one" } else { "no" },
        );
        let native = device
            .native()
            .create_graphics_pipeline(&program, format, state, depth);
        let pipeline = Self {
            slot: Arc::new(GraphicsSlot {
                device: device.clone(),
                program: Arc::new(program),
                native,
                format,
                depth,
                identity: ResourceId::next(),
            }),
        };
        pipeline.compile();
        pipeline
    }

    pub fn format(&self) -> Option<TextureFormat> {
        self.slot.format
    }

    pub fn label(&self) -> &str {
        self.slot.program.label()
    }

    pub fn bind_group(&self, group: u32, entries: &[Binding<'_>]) -> BindGroup {
        let label = self.slot.program.label();
        let specs = group_specs(self.slot.program.bindings(), group, label);
        let owner = self.slot.identity;
        let key = GroupKey::new(owner, group, entries.iter().map(|entry| entry.key()));
        let device = &self.slot.device;
        let submission = device.recording();
        device.state.groups.fetch(key, submission, || {
            let resources = bounded(device, &specs, entries, label);
            let native =
                device
                    .native()
                    .create_graphics_group(&self.slot.native, group, &resources);
            BindGroup {
                native,
                group,
                #[cfg(metal_backend)]
                resources,
                owner: Owner::Graphics(self.slot.clone()),
            }
        })
    }

    fn compile(&self) {
        self.slot.device.native().compile_graphics(
            &self.slot.native,
            &self.slot.program,
            self.slot.device.artifact_cache(),
        );
    }
}

impl Owner {
    pub(crate) fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Compute(left), Self::Compute(right)) => Arc::ptr_eq(left, right),
            (Self::Graphics(left), Self::Graphics(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }
}

pub(crate) fn group_specs(bindings: &[BindingSpec], group: u32, label: &str) -> Vec<BindingSpec> {
    let groups = bindings
        .iter()
        .map(|binding| binding.group)
        .max()
        .map_or(0, |group| group + 1);
    assert!(
        group < groups,
        "{label} declares {groups} bind groups where {group} is requested"
    );
    let mut specs = bindings
        .iter()
        .filter(|binding| binding.group == group)
        .copied()
        .collect::<Vec<_>>();
    specs.sort_unstable_by_key(|spec| spec.binding);
    for (index, spec) in specs.iter().enumerate() {
        assert_eq!(
            spec.binding as usize, index,
            "{label} numbers its bindings {} where its group holds {index}",
            spec.binding,
        );
    }
    specs
}

fn bounded(
    device: &Device,
    specs: &[BindingSpec],
    entries: &[Binding<'_>],
    label: &str,
) -> Vec<BoundResource> {
    assert_eq!(
        entries.len(),
        specs.len(),
        "{label} must bind exactly the resources of its group"
    );
    entries
        .iter()
        .zip(specs)
        .map(|(entry, spec)| {
            assert_eq!(
                entry.index, spec.binding,
                "a binding of {label} occupies the wrong slot"
            );
            match (spec.kind, entry.resource) {
                (
                    BindingKind::ReadOnlyStorage
                    | BindingKind::ReadWriteStorage
                    | BindingKind::TableStorage,
                    Resource::Buffer(binding),
                ) => {
                    assert!(
                        binding.buffer.device().same(device),
                        "a bind group cannot reference a different device"
                    );
                    assert!(
                        binding.buffer.usage().contains(BufferUsages::STORAGE),
                        "a storage binding requires shader storage"
                    );
                    assert!(
                        binding
                            .offset
                            .is_multiple_of(device.limits().min_storage_buffer_offset_alignment),
                        "a storage binding of {label} is misaligned"
                    );
                    assert!(
                        binding.size <= device.limits().max_storage_buffer_binding_size,
                        "a storage binding of {label} exceeds the device's maximum range"
                    );
                    assert!(
                        binding.offset + binding.size <= binding.buffer.size(),
                        "a storage binding of {label} outruns its buffer"
                    );
                    BoundResource::Buffer(BoundBuffer {
                        buffer: binding.buffer.clone(),
                        offset: binding.offset,
                        #[cfg(vulkan_backend)]
                        size: binding.size,
                        #[cfg(vulkan_backend)]
                        writable: spec.kind.writable(),
                    })
                }
                (BindingKind::SampledImage, Resource::Texture(texture)) => {
                    assert!(
                        texture.device().same(device),
                        "a bind group cannot reference a different device"
                    );
                    assert!(
                        texture.usages().contains(TextureUsages::SAMPLED),
                        "a sampled image binding requires a sampled texture"
                    );
                    assert!(
                        !texture.format().is_depth(),
                        "a sampled image binding requires a colour texture"
                    );
                    BoundResource::Texture(texture.clone())
                }
                (BindingKind::DepthImage, Resource::Texture(texture)) => {
                    assert!(
                        texture.device().same(device),
                        "a bind group cannot reference a different device"
                    );
                    assert!(
                        texture.usages().contains(TextureUsages::SAMPLED),
                        "a depth image binding requires a sampled texture"
                    );
                    assert!(
                        texture.format().is_depth(),
                        "a depth image binding requires a depth texture"
                    );
                    BoundResource::Texture(texture.clone())
                }
                (BindingKind::Sampler, Resource::Sampler(sampler)) => {
                    assert!(
                        sampler.device().same(device),
                        "a bind group cannot reference a different device"
                    );
                    assert!(
                        sampler.desc().compare.is_none(),
                        "a plain sampler binding requires a sampler that compares no depth"
                    );
                    BoundResource::Sampler(sampler.clone())
                }
                (BindingKind::ComparisonSampler, Resource::Sampler(sampler)) => {
                    assert!(
                        sampler.device().same(device),
                        "a bind group cannot reference a different device"
                    );
                    assert!(
                        sampler.desc().compare.is_some(),
                        "a comparison sampler binding requires a sampler that compares depth"
                    );
                    BoundResource::Sampler(sampler.clone())
                }
                (BindingKind::SampledImageTable, Resource::TextureTable(table)) => {
                    assert!(
                        table.device().same(device),
                        "a bind group cannot reference a different device"
                    );
                    assert_eq!(
                        table.capacity(),
                        spec.count,
                        "the texture table of {label} holds a different number of images"
                    );
                    BoundResource::TextureTable(table.clone())
                }
                (BindingKind::SamplerTable, Resource::SamplerTable(table)) => {
                    assert!(
                        table.device().same(device),
                        "a bind group cannot reference a different device"
                    );
                    assert_eq!(
                        table.capacity(),
                        spec.count,
                        "the sampler table of {label} holds a different number of samplers"
                    );
                    BoundResource::SamplerTable(table.clone())
                }
                (kind, _) => panic!(
                    "{label} binds the wrong resource where a {} belongs",
                    match kind {
                        BindingKind::SampledImage => "sampled image",
                        BindingKind::DepthImage => "depth image",
                        BindingKind::Sampler => "sampler",
                        BindingKind::ComparisonSampler => "comparison sampler",
                        BindingKind::SampledImageTable => "sampled image table",
                        BindingKind::SamplerTable => "sampler table",
                        _ => "storage buffer",
                    }
                ),
            }
        })
        .collect()
}

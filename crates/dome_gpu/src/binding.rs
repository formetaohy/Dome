use crate::buffer::{BufferBinding, BufferRange, BufferUsages};
use crate::cache::Cache;
use crate::context::Device;
use crate::identity::{ResourceId, Retired};
use crate::pipeline::PipelineLayout;
use crate::sampler::Sampler;
use crate::shader::{BindingKind, BindingSpec};
use crate::table::{SamplerTable, TableCapacity, TextureTable};
use crate::texture::{Texture, TextureFormatExt, TextureUsages};
use std::num::NonZeroU32;
use std::num::NonZeroU64;
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
            Resource::Buffer(binding) => ResourceKey::Buffer(binding.range()),
            Resource::Texture(texture) => ResourceKey::Texture(texture.identity()),
            Resource::Sampler(sampler) => ResourceKey::Sampler(sampler.identity()),
            Resource::TextureTable(table) => {
                ResourceKey::TextureTable(table.identity(), table.generation())
            }
            Resource::SamplerTable(table) => {
                ResourceKey::SamplerTable(table.identity(), table.generation())
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ResourceKey {
    Buffer(BufferRange),
    Texture(ResourceId),
    Sampler(ResourceId),
    TextureTable(ResourceId, u64),
    SamplerTable(ResourceId, u64),
}

impl ResourceKey {
    fn retired(&self, retired: &[Retired]) -> bool {
        retired.iter().any(|held| match (self, held) {
            (Self::Buffer(range), Retired::Resource(id)) => range.identity == *id,
            (Self::Texture(id) | Self::Sampler(id), Retired::Resource(dead)) => id == dead,
            (Self::TextureTable(id, _) | Self::SamplerTable(id, _), Retired::Resource(dead)) => {
                id == dead
            }
            (
                Self::TextureTable(id, generation) | Self::SamplerTable(id, generation),
                Retired::Generation(dead, superseded),
            ) => id == dead && generation <= superseded,
            (_, Retired::Generation(..)) => false,
        })
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct GroupKey {
    layout: ResourceId,
    group: u32,
    entries: Vec<ResourceKey>,
}

impl GroupKey {
    pub(crate) fn retired(&self, retired: &[Retired]) -> bool {
        self.entries.iter().any(|entry| entry.retired(retired))
    }
}

pub struct BindGroupLayout {
    pub(crate) native: wgpu::BindGroupLayout,
    pub(crate) specs: Vec<BindingSpec>,
    pub(crate) id: ResourceId,
}

impl BindGroupLayout {
    pub(crate) fn new(native: wgpu::BindGroupLayout, specs: Vec<BindingSpec>) -> Self {
        Self {
            native,
            specs,
            id: ResourceId::next(),
        }
    }
}

#[derive(Clone)]
pub struct BindGroup {
    pub(crate) native: wgpu::BindGroup,
    pub(crate) group: u32,
    pub(crate) layout: ResourceId,
}

impl BindGroup {
    pub(crate) fn group(&self) -> u32 {
        self.group
    }

    pub(crate) fn layout(&self) -> ResourceId {
        self.layout
    }
}

pub(crate) fn layouts(
    device: &Device,
    bindings: &[BindingSpec],
    label: &str,
    graphics: bool,
) -> Vec<Arc<BindGroupLayout>> {
    let groups = crate::shader::group_count(bindings, label, !graphics);
    let tables = *device.tables();
    (0..groups)
        .map(|group| {
            let specs = crate::shader::group_specs(bindings, group, label);
            device.cache().layout(specs, graphics, |specs| {
                let entries = specs
                    .iter()
                    .map(|spec| entry(*spec, graphics, tables))
                    .collect::<Vec<_>>();
                device
                    .native()
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some(label),
                        entries: &entries,
                    })
            })
        })
        .collect()
}

fn entry(spec: BindingSpec, graphics: bool, tables: TableCapacity) -> wgpu::BindGroupLayoutEntry {
    let visibility = if graphics {
        match spec.kind {
            BindingKind::Storage => wgpu::ShaderStages::VERTEX_FRAGMENT,
            _ => wgpu::ShaderStages::FRAGMENT,
        }
    } else {
        wgpu::ShaderStages::COMPUTE
    };
    let (ty, count) = match spec.kind {
        BindingKind::Storage | BindingKind::WritableStorage => (
            wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage {
                    read_only: spec.kind == BindingKind::Storage,
                },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            None,
        ),
        BindingKind::SampledTexture | BindingKind::TextureTable => (
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            table_count(spec.kind, tables),
        ),
        BindingKind::UnfilterableTexture => (
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            None,
        ),
        BindingKind::DepthTexture => (
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            None,
        ),
        BindingKind::FilteringSampler | BindingKind::SamplerTable => (
            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            table_count(spec.kind, tables),
        ),
        BindingKind::NonFilteringSampler => (
            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
            None,
        ),
        BindingKind::ComparisonSampler => (
            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
            None,
        ),
    };
    wgpu::BindGroupLayoutEntry {
        binding: spec.binding,
        visibility,
        ty,
        count,
    }
}

fn table_count(kind: BindingKind, tables: TableCapacity) -> Option<NonZeroU32> {
    match kind {
        BindingKind::TextureTable => tables.texture_count(),
        BindingKind::SamplerTable => tables.sampler_count(),
        _ => None,
    }
}

pub(crate) fn bind_group(
    device: &Device,
    cache: &Cache,
    layout: &BindGroupLayout,
    group: u32,
    entries: &[Binding<'_>],
    label: &str,
) -> BindGroup {
    assert_eq!(
        entries.len(),
        layout.specs.len(),
        "{label} must bind exactly the resources of group {group}"
    );
    let mut keys = Vec::with_capacity(entries.len());
    for (entry, spec) in entries.iter().zip(&layout.specs) {
        assert_eq!(
            entry.index, spec.binding,
            "a binding of {label} occupies the wrong slot in group {group}"
        );
        keys.push(entry.key());
    }
    let key = GroupKey {
        layout: layout.id,
        group,
        entries: keys,
    };
    cache.group(key, || {
        let mut texture_arrays = Vec::new();
        let mut sampler_arrays = Vec::new();
        for (entry, spec) in entries.iter().zip(&layout.specs) {
            match (spec.kind, entry.resource) {
                (BindingKind::TextureTable, Resource::TextureTable(table)) => {
                    assert_table_device(device, table.device());
                    texture_arrays.push((entry.index, table.views()));
                }
                (BindingKind::SamplerTable, Resource::SamplerTable(table)) => {
                    assert_table_device(device, table.device());
                    sampler_arrays.push((entry.index, table.samplers()));
                }
                _ => {}
            }
        }
        let native_entries = entries
            .iter()
            .zip(&layout.specs)
            .map(|(entry, spec)| wgpu::BindGroupEntry {
                binding: entry.index,
                resource: resource(
                    device,
                    *spec,
                    entry,
                    &texture_arrays,
                    &sampler_arrays,
                    label,
                ),
            })
            .collect::<Vec<_>>();
        let native = device
            .native()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &layout.native,
                entries: &native_entries,
            });
        BindGroup {
            native,
            group,
            layout: layout.id,
        }
    })
}

fn assert_table_device(device: &Device, table: &Device) {
    assert!(
        table.same(device),
        "a bind group cannot reference a different device"
    );
}

fn resource<'a>(
    device: &Device,
    spec: BindingSpec,
    entry: &'a Binding<'a>,
    texture_arrays: &'a [(u32, Vec<&'a wgpu::TextureView>)],
    sampler_arrays: &'a [(u32, Vec<&'a wgpu::Sampler>)],
    label: &'a str,
) -> wgpu::BindingResource<'a> {
    match (spec.kind, entry.resource) {
        (BindingKind::Storage | BindingKind::WritableStorage, Resource::Buffer(binding)) => {
            assert!(
                binding.buffer.device().same(device),
                "a bind group cannot reference a different device"
            );
            assert!(
                binding.buffer.usage().contains(BufferUsages::STORAGE),
                "a storage binding requires shader storage"
            );
            assert!(
                binding.offset.is_multiple_of(u64::from(
                    device.limits().min_storage_buffer_offset_alignment
                )),
                "a storage binding of {label} is misaligned"
            );
            assert!(
                binding.size <= device.limits().max_storage_buffer_binding_size,
                "a storage binding of {label} exceeds the device's maximum range"
            );
            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: binding.buffer.native(),
                offset: binding.offset,
                size: NonZeroU64::new(binding.size),
            })
        }
        (BindingKind::SampledTexture, Resource::Texture(texture)) => {
            assert_texture(device, texture);
            assert!(
                texture.usages().contains(TextureUsages::TEXTURE_BINDING),
                "a sampled image binding requires a sampled texture"
            );
            assert!(
                !texture.format().is_depth(),
                "a sampled image binding requires a colour texture"
            );
            wgpu::BindingResource::TextureView(texture.view())
        }
        (BindingKind::UnfilterableTexture, Resource::Texture(texture)) => {
            assert_texture(device, texture);
            assert!(
                texture.usages().contains(TextureUsages::TEXTURE_BINDING),
                "a sampled image binding requires a sampled texture"
            );
            assert!(
                !texture.format().is_depth(),
                "a sampled image binding requires a colour texture"
            );
            wgpu::BindingResource::TextureView(texture.view())
        }
        (BindingKind::DepthTexture, Resource::Texture(texture)) => {
            assert_texture(device, texture);
            assert!(
                texture.usages().contains(TextureUsages::TEXTURE_BINDING),
                "a depth image binding requires a sampled texture"
            );
            assert!(
                texture.format().is_depth(),
                "a depth image binding requires a depth texture"
            );
            wgpu::BindingResource::TextureView(texture.view())
        }
        (BindingKind::FilteringSampler, Resource::Sampler(sampler)) => {
            assert_sampler(device, sampler);
            assert!(
                sampler.desc().compare.is_none(),
                "a plain sampler binding requires a sampler that compares no depth"
            );
            wgpu::BindingResource::Sampler(sampler.native())
        }
        (BindingKind::NonFilteringSampler, Resource::Sampler(sampler)) => {
            assert_sampler(device, sampler);
            assert!(
                sampler.desc().compare.is_none(),
                "a non filtering sampler binding requires a sampler that compares no depth"
            );
            wgpu::BindingResource::Sampler(sampler.native())
        }
        (BindingKind::ComparisonSampler, Resource::Sampler(sampler)) => {
            assert_sampler(device, sampler);
            assert!(
                sampler.desc().compare.is_some(),
                "a comparison sampler binding requires a sampler that compares depth"
            );
            wgpu::BindingResource::Sampler(sampler.native())
        }
        (BindingKind::TextureTable, Resource::TextureTable(_)) => {
            let (_, array) = texture_arrays
                .iter()
                .find(|(binding, _)| *binding == entry.index)
                .expect("a texture table holds its binding");
            wgpu::BindingResource::TextureViewArray(array)
        }
        (BindingKind::SamplerTable, Resource::SamplerTable(_)) => {
            let (_, array) = sampler_arrays
                .iter()
                .find(|(binding, _)| *binding == entry.index)
                .expect("a sampler table holds its binding");
            wgpu::BindingResource::SamplerArray(array)
        }
        (kind, _) => panic!(
            "{label} binds the wrong resource where a {} belongs",
            match kind {
                BindingKind::Storage | BindingKind::WritableStorage => "storage buffer",
                BindingKind::SampledTexture => "sampled image",
                BindingKind::UnfilterableTexture => "unfilterable image",
                BindingKind::DepthTexture => "depth image",
                BindingKind::FilteringSampler => "filtering sampler",
                BindingKind::NonFilteringSampler => "non filtering sampler",
                BindingKind::ComparisonSampler => "comparison sampler",
                BindingKind::TextureTable => "sampled image table",
                BindingKind::SamplerTable => "sampler table",
            }
        ),
    }
}

fn assert_texture(device: &Device, texture: &Texture) {
    assert!(
        texture.device().same(device),
        "a bind group cannot reference a different device"
    );
}

fn assert_sampler(device: &Device, sampler: &Sampler) {
    assert!(
        sampler.device().same(device),
        "a bind group cannot reference a different device"
    );
}

pub(crate) fn pipeline_layout(
    device: &Device,
    bindings: &[BindingSpec],
    label: &str,
    graphics: bool,
) -> PipelineLayout {
    let groups = layouts(device, bindings, label, graphics);
    let native = device
        .native()
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &groups
                .iter()
                .map(|layout| Some(&layout.native))
                .collect::<Vec<_>>(),
            immediate_size: 0,
        });
    PipelineLayout { native, groups }
}

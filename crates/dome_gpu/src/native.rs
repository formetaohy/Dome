#[cfg(metal_backend)]
pub(crate) mod metal;
#[cfg(vulkan_backend)]
pub(crate) mod vulkan;

use crate::backend::{Backend, PREFERENCE};
use crate::cache::ArtifactCache;
use crate::capability::{
    AdapterInfo, AdapterPolicy, BufferUsages, Capability, Limits, TextureUsages,
};
use crate::context::{GpuRequest, GpuUnavailable};
use crate::gate::Gate;
use crate::host::{HostDevice, HostKind};
use crate::pipeline::BoundResource;
use crate::sampler::SamplerDesc;
use crate::submission::{Command, Write};
use crate::surface::WindowHandle;
use crate::texture::TextureFormat;
use dome_shader::shader::{ComputeProgram, GraphicsProgram};
use std::sync::Arc;
use std::time::Duration;

pub(crate) const FRAMES_IN_FLIGHT: usize = 4;
pub(crate) const FRAME_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) fn hang_deadline(
    frames: impl IntoIterator<Item = (u64, bool)>,
    index: u64,
    timeout: Duration,
) -> Duration {
    let waits_for_a_first_dispatch = frames
        .into_iter()
        .any(|(frame, first_dispatch)| first_dispatch && frame <= index);
    if waits_for_a_first_dispatch {
        Duration::MAX
    } else {
        timeout
    }
}
pub const STAGING_BYTES: u64 = 1 << 20;
pub(crate) const TRANSFER_ALIGNMENT: u64 = 4;
pub(crate) const TIME_SLOTS: usize = 4096;

pub(crate) enum DeviceFailure {
    Missing { offered: Vec<AdapterInfo> },
    Unavailable { reason: String },
}

impl DeviceFailure {
    pub(crate) fn missing(offered: Vec<AdapterInfo>) -> Self {
        Self::Missing { offered }
    }

    pub(crate) fn reason(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    fn unusable(self, backend: Backend) -> GpuUnavailable {
        match self {
            Self::Unavailable { reason } => GpuUnavailable::HostUnusable { backend, reason },
            Self::Missing { .. } => unreachable!("a borrowed device enumerates no adapter"),
        }
    }
}

impl From<String> for DeviceFailure {
    fn from(reason: String) -> Self {
        Self::reason(reason)
    }
}

pub(crate) enum NativeDevice {
    #[cfg(vulkan_backend)]
    Vulkan(Arc<vulkan::Device>),
    #[cfg(metal_backend)]
    Metal(Arc<metal::Device>),
}

#[derive(Clone)]
pub(crate) enum NativeBuffer {
    #[cfg(vulkan_backend)]
    Vulkan(vulkan::Buffer),
    #[cfg(metal_backend)]
    Metal(metal::Buffer),
}

pub(crate) enum NativePipeline {
    #[cfg(vulkan_backend)]
    Vulkan(vulkan::Pipeline),
    #[cfg(metal_backend)]
    Metal(metal::Pipeline),
}

pub(crate) enum NativeTexture {
    #[cfg(vulkan_backend)]
    Vulkan(vulkan::Texture),
    #[cfg(metal_backend)]
    Metal(metal::Texture),
}

#[derive(Clone)]
pub(crate) enum NativeDepth {
    #[cfg(vulkan_backend)]
    Vulkan(vulkan::Depth),
    #[cfg(metal_backend)]
    Metal(metal::Depth),
}

pub(crate) enum NativeSurface {
    #[cfg(vulkan_backend)]
    Vulkan(vulkan::Surface),
    #[cfg(metal_backend)]
    Metal(metal::Surface),
}

#[derive(Clone)]
pub(crate) enum NativePresent {
    #[cfg(vulkan_backend)]
    Vulkan(vulkan::Present),
    #[cfg(metal_backend)]
    Metal(metal::Present),
}

impl NativePresent {
    #[cfg(vulkan_backend)]
    pub(crate) fn vulkan(&self) -> &vulkan::Present {
        match self {
            Self::Vulkan(present) => present,
        }
    }

    #[cfg(metal_backend)]
    pub(crate) fn metal(&self) -> &metal::Present {
        match self {
            Self::Metal(present) => present,
        }
    }
}

pub(crate) struct NativeFrame {
    pub(crate) texture: NativeTexture,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: TextureFormat,
    pub(crate) present: NativePresent,
}

pub(crate) enum NativeSampler {
    #[cfg(vulkan_backend)]
    Vulkan(Box<vulkan::Sampler>),
    #[cfg(metal_backend)]
    Metal(metal::Sampler),
}

pub(crate) enum NativeTextureTable {
    #[cfg(vulkan_backend)]
    Vulkan,
    #[cfg(metal_backend)]
    Metal(metal::TextureTable),
}

pub(crate) enum NativeSamplerTable {
    #[cfg(vulkan_backend)]
    Vulkan,
    #[cfg(metal_backend)]
    Metal(metal::SamplerTable),
}

pub(crate) enum NativeGraphicsPipeline {
    #[cfg(vulkan_backend)]
    Vulkan(vulkan::GraphicsPipeline),
    #[cfg(metal_backend)]
    Metal(metal::GraphicsPipeline),
}

#[derive(Clone)]
pub(crate) enum NativeGroup {
    #[cfg(vulkan_backend)]
    Vulkan(Arc<vulkan::Group>),
    #[cfg(metal_backend)]
    Metal(Arc<metal::Group>),
}

pub(crate) fn open(
    request: &GpuRequest,
) -> Result<(NativeDevice, AdapterInfo, Limits, Capability), GpuUnavailable> {
    let order: &[Backend] = match &request.backend {
        Some(backend) if PREFERENCE.contains(backend) => std::slice::from_ref(backend),
        Some(backend) => {
            return Err(GpuUnavailable::BackendMissing {
                wanted: *backend,
                offered: PREFERENCE,
            });
        }
        None => PREFERENCE,
    };
    let mut reasons = Vec::new();
    let mut offered = Vec::new();
    let mut unsupported = None;
    for backend in order {
        match candidate(*backend, request.adapter, request.surface) {
            Ok((device, info, limits, capability)) if limits.supports(&Limits::BASELINE) => {
                return Ok((device, info, limits, capability));
            }
            Ok((_, info, limits, _)) => unsupported = Some((info, limits)),
            Err(DeviceFailure::Missing { offered: found }) => offered.extend(found),
            Err(DeviceFailure::Unavailable { reason }) => {
                reasons.push(format!("{backend:?}: {reason}"));
            }
        }
    }
    if let Some((info, limits)) = unsupported {
        return Err(GpuUnavailable::UnsupportedLimits { info, limits });
    }
    if let AdapterPolicy::Identity(wanted) = request.adapter {
        return Err(GpuUnavailable::AdapterMissing { wanted, offered });
    }
    Err(GpuUnavailable::NoAdapter {
        reason: if reasons.is_empty() {
            "no requested compute backend is available on this platform".to_owned()
        } else {
            reasons.join("; ")
        },
    })
}

pub(crate) fn borrow(
    host: HostDevice<'_>,
    request: &GpuRequest,
) -> Result<(NativeDevice, AdapterInfo, Limits, Capability), GpuUnavailable> {
    let backend = host.backend();
    if let Some(wanted) = request.backend
        && wanted != backend
    {
        return Err(GpuUnavailable::BackendMismatch {
            wanted,
            host: backend,
        });
    }
    let advance = host.advance();
    let keep_alive = host.hold;
    let (native, info, limits, capability) = match host.kind {
        #[cfg(metal_backend)]
        HostKind::Metal(raw) => metal::Device::borrow(raw, advance, keep_alive).map(
            |(device, info, limits, capability)| {
                (NativeDevice::Metal(device), info, limits, capability)
            },
        ),
        #[cfg(vulkan_backend)]
        HostKind::Vulkan(raw) => vulkan::Device::borrow(raw, advance, keep_alive).map(
            |(device, info, limits, capability)| {
                (NativeDevice::Vulkan(device), info, limits, capability)
            },
        ),
    }
    .map_err(|failure| failure.unusable(backend))?;
    if let AdapterPolicy::Identity(wanted) = request.adapter
        && wanted != info.id
    {
        return Err(GpuUnavailable::AdapterMismatch { wanted, host: info });
    }
    Ok((native, info, limits, capability))
}

fn candidate(
    backend: Backend,
    policy: AdapterPolicy,
    window: Option<WindowHandle>,
) -> Result<(NativeDevice, AdapterInfo, Limits, Capability), DeviceFailure> {
    match backend {
        #[cfg(metal_backend)]
        Backend::Metal => {
            metal::Device::open(policy, window).map(|(device, info, limits, capability)| {
                (NativeDevice::Metal(device), info, limits, capability)
            })
        }
        #[cfg(vulkan_backend)]
        Backend::Vulkan => {
            vulkan::Device::open(policy, window).map(|(device, info, limits, capability)| {
                (NativeDevice::Vulkan(device), info, limits, capability)
            })
        }
        _ => unreachable!("every backend of this build opens a candidate"),
    }
}

impl NativeSurface {
    pub(crate) fn width(&self) -> u32 {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(surface) => surface.width(),
            #[cfg(metal_backend)]
            Self::Metal(surface) => surface.width(),
        }
    }

    pub(crate) fn height(&self) -> u32 {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(surface) => surface.height(),
            #[cfg(metal_backend)]
            Self::Metal(surface) => surface.height(),
        }
    }

    pub(crate) fn format(&self) -> TextureFormat {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(surface) => surface.format(),
            #[cfg(metal_backend)]
            Self::Metal(surface) => surface.format(),
        }
    }

    pub(crate) fn acquire(&mut self) -> Option<NativeFrame> {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(surface) => surface.acquire(),
            #[cfg(metal_backend)]
            Self::Metal(surface) => surface.acquire(),
        }
    }

    pub(crate) fn outdated(&self) -> bool {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(surface) => surface.outdated(),
            #[cfg(metal_backend)]
            Self::Metal(_) => false,
        }
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(surface) => surface.resize(width, height),
            #[cfg(metal_backend)]
            Self::Metal(surface) => surface.resize(width, height),
        }
    }
}

impl NativeGraphicsPipeline {
    pub(crate) fn is_compiled(&self) -> bool {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(pipeline) => pipeline.is_compiled(),
            #[cfg(metal_backend)]
            Self::Metal(pipeline) => pipeline.is_compiled(),
        }
    }
}

impl NativePipeline {
    pub(crate) fn is_compiled(&self) -> bool {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(pipeline) => pipeline.is_compiled(),
            #[cfg(metal_backend)]
            Self::Metal(pipeline) => pipeline.is_compiled(),
        }
    }
}

impl NativeDevice {
    pub(crate) fn compile(
        &self,
        pipeline: &NativePipeline,
        program: &ComputeProgram,
        artifacts: &ArtifactCache,
    ) {
        match (self, pipeline) {
            #[cfg(vulkan_backend)]
            (Self::Vulkan(device), NativePipeline::Vulkan(pipeline)) => {
                device.compile(pipeline, program, artifacts)
            }
            #[cfg(metal_backend)]
            (Self::Metal(device), NativePipeline::Metal(pipeline)) => {
                device.compile(pipeline, program, artifacts)
            }
        }
    }

    pub(crate) fn gate(&self) -> Arc<Gate> {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => Arc::clone(&device.gate),
            #[cfg(metal_backend)]
            Self::Metal(device) => Arc::clone(&device.gate),
        }
    }

    pub(crate) fn create_buffer(
        &self,
        label: &str,
        size: u64,
        usage: BufferUsages,
    ) -> NativeBuffer {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => NativeBuffer::Vulkan(device.create_buffer(label, size, usage)),
            #[cfg(metal_backend)]
            Self::Metal(device) => NativeBuffer::Metal(device.create_buffer(label, size, usage)),
        }
    }

    pub(crate) fn create_pipeline(&self, program: &ComputeProgram) -> NativePipeline {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => NativePipeline::Vulkan(device.create_pipeline(program)),
            #[cfg(metal_backend)]
            Self::Metal(device) => NativePipeline::Metal(device.create_pipeline(program)),
        }
    }

    pub(crate) fn create_texture(
        &self,
        label: &str,
        width: u32,
        height: u32,
        mip_levels: u32,
        format: TextureFormat,
        usages: TextureUsages,
    ) -> NativeTexture {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => NativeTexture::Vulkan(
                device.create_texture(label, width, height, mip_levels, format, usages),
            ),
            #[cfg(metal_backend)]
            Self::Metal(device) => NativeTexture::Metal(
                device.create_texture(label, width, height, mip_levels, format, usages),
            ),
        }
    }

    pub(crate) fn create_surface(&self, width: u32, height: u32) -> NativeSurface {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => NativeSurface::Vulkan(device.create_surface(width, height)),
            #[cfg(metal_backend)]
            Self::Metal(device) => NativeSurface::Metal(device.create_surface(width, height)),
        }
    }

    pub(crate) fn has_surface(&self) -> bool {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => device.has_surface(),
            #[cfg(metal_backend)]
            Self::Metal(device) => device.has_surface(),
        }
    }

    pub(crate) fn idle(&self) {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => device.idle(),
            #[cfg(metal_backend)]
            Self::Metal(device) => device.idle(),
        }
    }

    pub(crate) fn create_sampler(&self, desc: &SamplerDesc, anisotropy: u32) -> NativeSampler {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => {
                NativeSampler::Vulkan(Box::new(device.create_sampler(desc, anisotropy)))
            }
            #[cfg(metal_backend)]
            Self::Metal(device) => NativeSampler::Metal(device.create_sampler(desc, anisotropy)),
        }
    }

    pub(crate) fn create_texture_table(
        &self,
        capacity: u32,
        fallback: &crate::Texture,
    ) -> NativeTextureTable {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(_) => {
                let _ = (capacity, fallback);
                NativeTextureTable::Vulkan
            }
            #[cfg(metal_backend)]
            Self::Metal(device) => {
                NativeTextureTable::Metal(device.create_texture_table(capacity, fallback))
            }
        }
    }

    pub(crate) fn fill_texture_table(
        &self,
        table: &NativeTextureTable,
        index: u32,
        texture: &crate::Texture,
    ) {
        match (self, table) {
            #[cfg(vulkan_backend)]
            (Self::Vulkan(_), NativeTextureTable::Vulkan) => {
                let _ = (index, texture);
            }
            #[cfg(metal_backend)]
            (Self::Metal(_), NativeTextureTable::Metal(table)) => table.fill(index, texture),
        }
    }

    pub(crate) fn create_sampler_table(
        &self,
        capacity: u32,
        fallback: &crate::Sampler,
    ) -> NativeSamplerTable {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(_) => {
                let _ = (capacity, fallback);
                NativeSamplerTable::Vulkan
            }
            #[cfg(metal_backend)]
            Self::Metal(device) => {
                NativeSamplerTable::Metal(device.create_sampler_table(capacity, fallback))
            }
        }
    }

    pub(crate) fn fill_sampler_table(
        &self,
        table: &NativeSamplerTable,
        index: u32,
        sampler: &crate::Sampler,
    ) {
        match (self, table) {
            #[cfg(vulkan_backend)]
            (Self::Vulkan(_), NativeSamplerTable::Vulkan) => {
                let _ = (index, sampler);
            }
            #[cfg(metal_backend)]
            (Self::Metal(_), NativeSamplerTable::Metal(table)) => table.fill(index, sampler),
        }
    }

    pub(crate) fn create_graphics_pipeline(
        &self,
        program: &GraphicsProgram,
        format: crate::texture::TextureFormat,
        state: crate::graphics::PipelineState,
        depth: bool,
    ) -> NativeGraphicsPipeline {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => NativeGraphicsPipeline::Vulkan(
                device.create_graphics_pipeline(program, format, state, depth),
            ),
            #[cfg(metal_backend)]
            Self::Metal(device) => NativeGraphicsPipeline::Metal(
                device.create_graphics_pipeline(program, format, state, depth),
            ),
        }
    }

    pub(crate) fn create_depth_texture(&self, label: &str, width: u32, height: u32) -> NativeDepth {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => {
                NativeDepth::Vulkan(device.create_depth_texture(label, width, height))
            }
            #[cfg(metal_backend)]
            Self::Metal(device) => {
                NativeDepth::Metal(device.create_depth_texture(label, width, height))
            }
        }
    }

    pub(crate) fn compile_graphics(
        &self,
        pipeline: &NativeGraphicsPipeline,
        program: &GraphicsProgram,
        artifacts: &ArtifactCache,
    ) {
        match (self, pipeline) {
            #[cfg(vulkan_backend)]
            (Self::Vulkan(device), NativeGraphicsPipeline::Vulkan(pipeline)) => {
                device.compile_graphics(pipeline, program, artifacts)
            }
            #[cfg(metal_backend)]
            (Self::Metal(device), NativeGraphicsPipeline::Metal(pipeline)) => {
                device.compile_graphics(pipeline, program, artifacts)
            }
        }
    }

    pub(crate) fn create_graphics_group(
        &self,
        pipeline: &NativeGraphicsPipeline,
        group: u32,
        resources: &[BoundResource],
    ) -> NativeGroup {
        match (self, pipeline) {
            #[cfg(vulkan_backend)]
            (Self::Vulkan(device), NativeGraphicsPipeline::Vulkan(pipeline)) => {
                NativeGroup::Vulkan(Arc::new(
                    device.create_graphics_group(pipeline, group, resources),
                ))
            }
            #[cfg(metal_backend)]
            (Self::Metal(device), NativeGraphicsPipeline::Metal(pipeline)) => NativeGroup::Metal(
                Arc::new(device.create_graphics_group(pipeline, group, resources)),
            ),
        }
    }

    pub(crate) fn create_group(
        &self,
        pipeline: &NativePipeline,
        group: u32,
        resources: &[BoundResource],
    ) -> NativeGroup {
        match (self, pipeline) {
            #[cfg(vulkan_backend)]
            (Self::Vulkan(device), NativePipeline::Vulkan(pipeline)) => {
                NativeGroup::Vulkan(Arc::new(device.create_group(pipeline, group, resources)))
            }
            #[cfg(metal_backend)]
            (Self::Metal(device), NativePipeline::Metal(pipeline)) => {
                NativeGroup::Metal(Arc::new(device.create_group(pipeline, group, resources)))
            }
        }
    }

    pub(crate) fn submit(
        &self,
        writes: &[Write],
        commands: &[Command],
        first_dispatch: bool,
    ) -> u64 {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => device.submit(writes, commands, first_dispatch),
            #[cfg(metal_backend)]
            Self::Metal(device) => device.submit(writes, commands, first_dispatch),
        }
    }

    pub(crate) fn wait(&self, index: u64, timeout: Duration) {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => device.wait(index, timeout),
            #[cfg(metal_backend)]
            Self::Metal(device) => device.wait(index, timeout),
        }
    }

    pub(crate) fn complete(&self, index: u64) -> bool {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => device.complete(index),
            #[cfg(metal_backend)]
            Self::Metal(device) => device.complete(index),
        }
    }

    pub(crate) fn completed(&self) -> u64 {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => device.completed(),
            #[cfg(metal_backend)]
            Self::Metal(device) => device.completed(),
        }
    }

    pub(crate) fn seconds(&self, index: u64) -> f64 {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => device.seconds(index),
            #[cfg(metal_backend)]
            Self::Metal(device) => device.seconds(index),
        }
    }

    pub(crate) fn read(&self, buffer: &NativeBuffer, bytes: u64) -> Vec<u8> {
        match (self, buffer) {
            #[cfg(vulkan_backend)]
            (Self::Vulkan(device), NativeBuffer::Vulkan(buffer)) => device.read(buffer, bytes),
            #[cfg(metal_backend)]
            (Self::Metal(device), NativeBuffer::Metal(buffer)) => device.read(buffer, bytes),
        }
    }

    pub(crate) fn write_host(&self, buffer: &NativeBuffer, offset: u64, bytes: &[u8]) {
        match (self, buffer) {
            #[cfg(vulkan_backend)]
            (Self::Vulkan(device), NativeBuffer::Vulkan(buffer)) => {
                device.write_host(buffer, offset, bytes)
            }
            #[cfg(metal_backend)]
            (Self::Metal(device), NativeBuffer::Metal(buffer)) => {
                device.write_host(buffer, offset, bytes)
            }
        }
    }

    pub(crate) fn read_host(&self, buffer: &NativeBuffer, offset: u64, bytes: u64) -> Vec<u8> {
        match (self, buffer) {
            #[cfg(vulkan_backend)]
            (Self::Vulkan(device), NativeBuffer::Vulkan(buffer)) => {
                device.read_host(buffer, offset, bytes)
            }
            #[cfg(metal_backend)]
            (Self::Metal(device), NativeBuffer::Metal(buffer)) => {
                device.read_host(buffer, offset, bytes)
            }
        }
    }

    pub(crate) fn assert_alive(&self) {
        match self {
            #[cfg(vulkan_backend)]
            Self::Vulkan(device) => device.assert_alive(),
            #[cfg(metal_backend)]
            Self::Metal(device) => device.assert_alive(),
        }
    }
}

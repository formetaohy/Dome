use super::{
    DeviceFailure, FRAME_TIMEOUT, FRAMES_IN_FLIGHT, NativeBuffer, NativeFrame,
    NativeGraphicsPipeline, NativeGroup, NativePipeline, NativePresent, NativeTexture,
    STAGING_BYTES, TIME_SLOTS, TRANSFER_ALIGNMENT, hang_deadline,
};
use crate::backend::Backend;
use crate::buffer::GpuBuffer;
use crate::cache::{ArtifactCache, fingerprint};
use crate::capability::{
    AdapterId, AdapterInfo, AdapterPolicy, BufferUsages, Capability, CooperativeMatrix, DeviceType,
    Limits, TextureUsages,
};
use crate::gate::Gate;
use crate::graphics::{BlendState, DepthLoad, DepthState, PipelineState};
use crate::host::{HostAdvance, metal_identity};
use crate::native::NativeDepth;
use crate::pipeline::BoundResource;
use crate::sampler::{AddressMode, Filter, SamplerDesc};
use crate::submission::{Command, RecordedCount, Write};
use crate::surface::WindowHandle;
use crate::texture::TextureFormat;
use dome_gpu::shader::{
    ComputeProgram, GraphicsProgram, GraphicsTranslation, MAX_BINDING_BYTES,
    METAL_SIZE_BUFFER_SLOT, ShaderTranslation,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{ClassType, msg_send};
use objc2_core_foundation::{CGRect, CGSize};
use objc2_foundation::{NSArray, NSRange, NSString, NSURL};
use objc2_metal as mtl;
use objc2_metal::{
    MTLArgumentDescriptor, MTLArgumentEncoder, MTLBinaryArchive, MTLBinaryArchiveDescriptor,
    MTLBindingAccess, MTLBlendFactor, MTLBlendOperation, MTLBlitCommandEncoder, MTLBuffer,
    MTLClearColor, MTLColorWriteMask, MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandEncoder,
    MTLCommandQueue, MTLCompareFunction, MTLComputeCommandEncoder, MTLComputePipelineDescriptor,
    MTLComputePipelineState, MTLDataType, MTLDepthStencilDescriptor, MTLDepthStencilState,
    MTLDevice, MTLDrawable, MTLGPUFamily, MTLLibrary, MTLLoadAction, MTLOrigin, MTLPipelineOption,
    MTLPixelFormat, MTLPrimitiveType, MTLRenderCommandEncoder, MTLRenderPassDescriptor,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLRenderStages, MTLResource,
    MTLResourceOptions, MTLResourceUsage, MTLSamplerAddressMode, MTLSamplerDescriptor,
    MTLSamplerMinMagFilter, MTLSamplerMipFilter, MTLSamplerState, MTLScissorRect, MTLSize,
    MTLStorageMode, MTLStoreAction, MTLTexture, MTLTextureDescriptor, MTLTextureType,
    MTLTextureUsage, MTLViewport,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use std::any::Any;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::ptr;
use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

const RELEASE_TIMEOUT: Duration = Duration::from_secs(30);

struct Frame {
    index: u64,
    first_dispatch: bool,
    command: Option<Retained<ProtocolObject<dyn MTLCommandBuffer>>>,
    staging: Arc<BufferResource>,
    cursor: u64,
    resources: Vec<Arc<dyn Any + Send + Sync>>,
}

unsafe impl Send for Frame {}

impl Frame {
    fn new(device: &Device) -> Self {
        Self {
            index: 0,
            first_dispatch: false,
            command: None,
            staging: device.allocate(STAGING_BYTES, true),
            cursor: 0,
            resources: Vec::new(),
        }
    }

    fn begin(&mut self) {
        self.cursor = 0;
        self.resources.clear();
    }

    fn stage(
        &mut self,
        device: &Device,
        bytes: &[u8],
        alignment: u64,
    ) -> (Arc<BufferResource>, u64) {
        let size = bytes.len() as u64;
        let aligned = self.cursor.next_multiple_of(alignment);
        let (staging, offset) = if STAGING_BYTES
            .checked_sub(aligned)
            .is_some_and(|free| size <= free)
        {
            self.cursor = aligned + size;
            (self.staging.clone(), aligned)
        } else {
            let staging = device.allocate(size, true);
            self.resources.push(staging.clone());
            (staging, 0)
        };
        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                staging
                    .raw
                    .contents()
                    .as_ptr()
                    .cast::<u8>()
                    .add(offset as usize),
                bytes.len(),
            );
        }
        (staging, offset)
    }
}

struct QueueState {
    next: u64,
    completed: u64,
    frames: Vec<Frame>,
    times: [Option<(u64, f64)>; TIME_SLOTS],
}

impl Default for QueueState {
    fn default() -> Self {
        Self {
            next: 0,
            completed: 0,
            frames: Vec::new(),
            times: [None; TIME_SLOTS],
        }
    }
}

pub(crate) struct Device {
    raw: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    state: Mutex<QueueState>,
    pub(crate) gate: Arc<Gate>,
    window: Option<WindowHandle>,
    _keep_alive: Option<Arc<dyn Any + Send + Sync>>,
}

struct Lent {
    device: usize,
    native: Weak<Device>,
    info: AdapterInfo,
    limits: Limits,
    capability: Capability,
}

static LENT: LazyLock<Mutex<Vec<Lent>>> = LazyLock::new(|| Mutex::new(Vec::new()));

struct BufferResource {
    raw: Retained<ProtocolObject<dyn MTLBuffer>>,
}

unsafe impl Send for BufferResource {}
unsafe impl Sync for BufferResource {}

#[derive(Clone)]
pub(crate) struct Buffer {
    resource: Arc<BufferResource>,
}

struct PipelineResource {
    compiled: OnceLock<Retained<ProtocolObject<dyn MTLComputePipelineState>>>,
    threads: OnceLock<u32>,
}

#[derive(Clone)]
pub(crate) struct Texture {
    resource: Arc<TextureResource>,
}

struct Target {
    view: Retained<ProtocolObject<dyn MTLTexture>>,
}

#[derive(Clone)]
pub(crate) struct Depth {
    raw: Arc<DepthTexture>,
}

struct DepthTexture {
    raw: Retained<ProtocolObject<dyn MTLTexture>>,
}

unsafe impl Send for DepthTexture {}
unsafe impl Sync for DepthTexture {}

pub(crate) struct Surface {
    device: Arc<Device>,
    layer: Retained<CAMetalLayer>,
    view: WindowHandle,
    cache: HashMap<usize, Texture>,
    width: u32,
    height: u32,
}

#[derive(Clone)]
pub(crate) struct Present {
    drawable: Retained<ProtocolObject<dyn MTLDrawable>>,
}

struct TextureResource {
    raw: Retained<ProtocolObject<dyn MTLTexture>>,
    targets: Vec<Target>,
    width: u32,
    height: u32,
    format: TextureFormat,
}

pub(crate) struct Sampler {
    raw: Retained<ProtocolObject<dyn MTLSamplerState>>,
}

pub(crate) struct TextureTable {
    encoder: Retained<ProtocolObject<dyn MTLArgumentEncoder>>,
    staging: Retained<ProtocolObject<dyn MTLBuffer>>,
}

unsafe impl Send for TextureTable {}
unsafe impl Sync for TextureTable {}

impl TextureTable {
    pub(crate) fn fill(&self, index: u32, texture: &crate::Texture) {
        let texture = native_texture(texture.native());
        unsafe {
            self.encoder
                .setArgumentBuffer_offset(Some(&self.staging), 0);
            self.encoder
                .setTexture_atIndex(Some(&texture.raw), index as usize);
        }
    }

    fn snapshot<'a>(
        &self,
        device: &ProtocolObject<dyn MTLDevice>,
        textures: impl Iterator<Item = &'a crate::Texture>,
    ) -> Snapshot {
        let buffer = argument_buffer(device, self.encoder.encodedLength());
        unsafe { self.encoder.setArgumentBuffer_offset(Some(&buffer), 0) };
        for (index, texture) in textures.enumerate() {
            let texture = native_texture(texture.native());
            unsafe { self.encoder.setTexture_atIndex(Some(&texture.raw), index) };
        }
        Snapshot { buffer }
    }
}

pub(crate) struct SamplerTable {
    encoder: Retained<ProtocolObject<dyn MTLArgumentEncoder>>,
    staging: Retained<ProtocolObject<dyn MTLBuffer>>,
}

unsafe impl Send for SamplerTable {}
unsafe impl Sync for SamplerTable {}

impl SamplerTable {
    pub(crate) fn fill(&self, index: u32, sampler: &crate::Sampler) {
        let sampler = native_sampler(sampler.native());
        unsafe {
            self.encoder
                .setArgumentBuffer_offset(Some(&self.staging), 0);
            self.encoder
                .setSamplerState_atIndex(Some(&sampler.raw), index as usize);
        }
    }

    fn snapshot<'a>(
        &self,
        device: &ProtocolObject<dyn MTLDevice>,
        samplers: impl Iterator<Item = &'a crate::Sampler>,
    ) -> Snapshot {
        let buffer = argument_buffer(device, self.encoder.encodedLength());
        unsafe { self.encoder.setArgumentBuffer_offset(Some(&buffer), 0) };
        for (index, sampler) in samplers.enumerate() {
            let sampler = native_sampler(sampler.native());
            unsafe {
                self.encoder
                    .setSamplerState_atIndex(Some(&sampler.raw), index)
            };
        }
        Snapshot { buffer }
    }
}

fn argument_buffer(
    device: &ProtocolObject<dyn MTLDevice>,
    length: usize,
) -> Retained<ProtocolObject<dyn MTLBuffer>> {
    device
        .newBufferWithLength_options(length, MTLResourceOptions::StorageModeShared)
        .expect("a Metal argument buffer holds a table")
}

pub(crate) struct Snapshot {
    buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
}

pub(crate) struct Group {
    snapshots: Vec<Option<Snapshot>>,
}

unsafe impl Send for Group {}
unsafe impl Sync for Group {}

impl Group {
    fn of(device: &ProtocolObject<dyn MTLDevice>, resources: &[BoundResource]) -> Self {
        Self {
            snapshots: resources
                .iter()
                .map(|resource| match resource {
                    BoundResource::TextureTable(table) => {
                        let native = texture_table(table.native());
                        Some(native.snapshot(device, table.entries()))
                    }
                    BoundResource::SamplerTable(table) => {
                        let native = sampler_table(table.native());
                        Some(native.snapshot(device, table.entries()))
                    }
                    _ => None,
                })
                .collect(),
        }
    }

    fn buffer(&self, at: usize) -> &ProtocolObject<dyn MTLBuffer> {
        &self.snapshots[at]
            .as_ref()
            .expect("a bound resource table owns its snapshot")
            .buffer
    }
}

fn texture_table(native: &crate::native::NativeTextureTable) -> &TextureTable {
    match native {
        crate::native::NativeTextureTable::Metal(table) => table,
    }
}

fn sampler_table(native: &crate::native::NativeSamplerTable) -> &SamplerTable {
    match native {
        crate::native::NativeSamplerTable::Metal(table) => table,
    }
}

fn group(native: &NativeGroup) -> &Arc<Group> {
    match native {
        NativeGroup::Metal(group) => group,
    }
}

fn native_sampler(native: &crate::NativeSampler) -> &Sampler {
    match native {
        crate::NativeSampler::Metal(sampler) => sampler,
    }
}

unsafe impl Send for TextureResource {}
unsafe impl Sync for TextureResource {}

unsafe impl Send for Depth {}
unsafe impl Sync for Depth {}

pub(crate) struct GraphicsPipeline {
    resource: Arc<GraphicsPipelineResource>,
}

struct GraphicsPipelineResource {
    format: TextureFormat,
    state: PipelineState,
    depth_attachment: bool,
    compiled: OnceLock<Retained<ProtocolObject<dyn MTLRenderPipelineState>>>,
    depth: OnceLock<Retained<ProtocolObject<dyn MTLDepthStencilState>>>,
}

pub(crate) struct Pipeline {
    resource: Arc<PipelineResource>,
}

struct Archive {
    raw: Retained<ProtocolObject<dyn MTLBinaryArchive>>,
    key: String,
    staged: PathBuf,
}

unsafe impl Send for Archive {}
unsafe impl Sync for Archive {}

impl Archive {
    fn of(
        device: &ProtocolObject<dyn MTLDevice>,
        artifacts: &ArtifactCache,
        program: &str,
    ) -> Option<Self> {
        let key = format!("metal/{}/archive-{program}.bin", device.registryID());
        let staged = artifacts.scratch(&key)?;
        let descriptor = MTLBinaryArchiveDescriptor::new();
        if let Some(bytes) = artifacts.load(&key) {
            if fs::write(&staged, bytes).is_err() {
                let _ = fs::remove_file(&staged);
                return None;
            }
            let url = NSURL::fileURLWithPath(&NSString::from_str(&staged.to_string_lossy()));
            descriptor.setUrl(Some(&url));
        }
        let raw = device
            .newBinaryArchiveWithDescriptor_error(&descriptor)
            .or_else(|_| {
                device.newBinaryArchiveWithDescriptor_error(&MTLBinaryArchiveDescriptor::new())
            })
            .unwrap_or_else(|error| {
                panic!("creating the Metal archive {}: {error}", staged.display(),)
            });
        Some(Self { raw, key, staged })
    }
}

fn native_buffer(native: &NativeBuffer) -> &Arc<BufferResource> {
    match native {
        NativeBuffer::Metal(buffer) => &buffer.resource,
    }
}

fn buffer(gpu: &GpuBuffer) -> &Arc<BufferResource> {
    native_buffer(gpu.native())
}

fn native_texture(native: &NativeTexture) -> &Arc<TextureResource> {
    match native {
        NativeTexture::Metal(texture) => &texture.resource,
    }
}

fn graphics_pipeline(native: &NativeGraphicsPipeline) -> &Arc<GraphicsPipelineResource> {
    match native {
        NativeGraphicsPipeline::Metal(pipeline) => &pipeline.resource,
    }
}

fn pipeline(native: &NativePipeline) -> &Arc<PipelineResource> {
    match native {
        NativePipeline::Metal(pipeline) => &pipeline.resource,
    }
}

fn claim_group(native: &NativeGroup) {
    match native {
        NativeGroup::Metal(_) => {}
    }
}

fn metal_format(format: TextureFormat) -> MTLPixelFormat {
    match format {
        TextureFormat::Rgba8Unorm => MTLPixelFormat::RGBA8Unorm,
        TextureFormat::Rgba8UnormSrgb => MTLPixelFormat::RGBA8Unorm_sRGB,
        TextureFormat::Bgra8Unorm => MTLPixelFormat::BGRA8Unorm,
        TextureFormat::Rgba16Float => MTLPixelFormat::RGBA16Float,
        TextureFormat::R32Float => MTLPixelFormat::R32Float,
    }
}

fn capability(device: &ProtocolObject<dyn MTLDevice>) -> Capability {
    Capability {
        cooperative_matrix: device.supportsFamily(MTLGPUFamily::Apple7).then_some(
            CooperativeMatrix {
                subgroup: 32,
                rows: 8,
                columns: 8,
                depth: 8,
            },
        ),
        bindless: device.argumentBuffersSupport() == objc2_metal::MTLArgumentBuffersTier::Tier2,
    }
}

fn device_type(device: &ProtocolObject<dyn MTLDevice>) -> DeviceType {
    if device.hasUnifiedMemory() {
        DeviceType::Integrated
    } else {
        DeviceType::Discrete
    }
}

fn describe(device: &ProtocolObject<dyn MTLDevice>) -> AdapterInfo {
    AdapterInfo {
        name: device.name().to_string(),
        backend: Backend::Metal,
        device_type: device_type(device),
        id: AdapterId::MetalRegistry(device.registryID()),
    }
}

fn limits(device: &ProtocolObject<dyn MTLDevice>) -> Limits {
    let bytes = (device.maxBufferLength() as u64).min(MAX_BINDING_BYTES);
    Limits {
        max_storage_buffers_per_shader_stage: u32::from(METAL_SIZE_BUFFER_SLOT),
        max_sampled_images_per_shader_stage: u32::MAX,
        max_samplers_per_shader_stage: u32::MAX,
        max_storage_buffer_binding_size: bytes,
        max_buffer_size: bytes,
        max_compute_invocations_per_workgroup: device.maxThreadsPerThreadgroup().width as u32,
        max_compute_workgroup_size_x: device.maxThreadsPerThreadgroup().width as u32,
        max_compute_workgroup_storage_size: device.maxThreadgroupMemoryLength() as u32,
        max_compute_workgroups_per_dimension: 65_535,
        min_storage_buffer_offset_alignment: 256,
        max_sampler_anisotropy: 16,
    }
}

fn assemble(
    raw: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    advance: Option<HostAdvance>,
    keep_alive: Option<Arc<dyn Any + Send + Sync>>,
    window: Option<WindowHandle>,
) -> Arc<Device> {
    Arc::new(Device {
        raw,
        queue,
        state: Mutex::new(QueueState::default()),
        gate: Gate::of(advance),
        window,
        _keep_alive: keep_alive,
    })
}

fn queue(
    device: &ProtocolObject<dyn MTLDevice>,
) -> Result<Retained<ProtocolObject<dyn MTLCommandQueue>>, DeviceFailure> {
    device
        .newCommandQueue()
        .ok_or_else(|| DeviceFailure::reason("Metal refused a compute command queue"))
}

impl Device {
    pub(crate) fn open(
        policy: AdapterPolicy,
        window: Option<WindowHandle>,
    ) -> Result<(Arc<Self>, AdapterInfo, Limits, Capability), DeviceFailure> {
        let devices = mtl::MTLCopyAllDevices();
        let offered = devices
            .iter()
            .map(|device| describe(&device))
            .collect::<Vec<_>>();
        let mut candidates = devices
            .iter()
            .filter(|device| policy.wants(AdapterId::MetalRegistry(device.registryID())))
            .map(|device| (describe(&device), device))
            .collect::<Vec<_>>();
        if let AdapterPolicy::Power(preference) = policy {
            candidates.sort_by_key(|(info, _)| Reverse(info.device_type.rank(preference)));
        }
        let Some((info, raw)) = candidates.into_iter().next() else {
            return Err(match policy {
                AdapterPolicy::Identity(_) => DeviceFailure::missing(offered),
                AdapterPolicy::Power(_) => DeviceFailure::reason("no Metal compute device"),
            });
        };
        let limits = limits(&raw);
        let capability = capability(&raw);
        let queue = queue(&raw)?;
        Ok((
            assemble(raw, queue, None, None, window),
            info,
            limits,
            capability,
        ))
    }

    pub(crate) fn borrow(
        raw: &Retained<ProtocolObject<dyn MTLDevice>>,
        advance: Option<HostAdvance>,
        keep_alive: Option<Arc<dyn Any + Send + Sync>>,
    ) -> Result<(Arc<Self>, AdapterInfo, Limits, Capability), DeviceFailure> {
        let mut lent = LENT
            .lock()
            .expect("the borrowed Metal devices are never poisoned");
        lent.retain(|entry| entry.native.strong_count() > 0);
        let identity = metal_identity(raw);
        if let Some(entry) = lent.iter().find(|entry| entry.device == identity)
            && let Some(device) = entry.native.upgrade()
        {
            device.gate.advancing(advance);
            return Ok((device, entry.info.clone(), entry.limits, entry.capability));
        }
        let info = describe(raw);
        let limits = limits(raw);
        let capability = capability(raw);
        let queue = queue(raw)?;
        let device = assemble(raw.clone(), queue, advance, keep_alive, None);
        lent.push(Lent {
            device: identity,
            native: Arc::downgrade(&device),
            info: info.clone(),
            limits,
            capability,
        });
        Ok((device, info, limits, capability))
    }

    fn allocate(&self, size: u64, shared: bool) -> Arc<BufferResource> {
        let options = if shared {
            MTLResourceOptions::StorageModeShared
        } else {
            MTLResourceOptions::StorageModePrivate
        } | MTLResourceOptions::HazardTrackingModeTracked;
        let raw = self
            .raw
            .newBufferWithLength_options(size as usize, options)
            .unwrap_or_else(|| panic!("allocating {size} Metal compute bytes failed"));
        Arc::new(BufferResource { raw })
    }

    pub(crate) fn create_buffer(
        self: &Arc<Self>,
        label: &str,
        size: u64,
        usage: BufferUsages,
    ) -> Buffer {
        let resource = self.allocate(
            size,
            usage.contains(BufferUsages::MAP_READ) || usage.contains(BufferUsages::MAP_WRITE),
        );
        resource.raw.setLabel(Some(&NSString::from_str(label)));
        Buffer { resource }
    }

    pub(crate) fn create_sampler(&self, desc: &SamplerDesc, anisotropy: u32) -> Sampler {
        let filter = |filter: Filter| match filter {
            Filter::Nearest => MTLSamplerMinMagFilter::Nearest,
            Filter::Linear => MTLSamplerMinMagFilter::Linear,
        };
        let mip = |filter: Filter| match filter {
            Filter::Nearest => MTLSamplerMipFilter::Nearest,
            Filter::Linear => MTLSamplerMipFilter::Linear,
        };
        let address = |mode: AddressMode| match mode {
            AddressMode::ClampToEdge => MTLSamplerAddressMode::ClampToEdge,
            AddressMode::Repeat => MTLSamplerAddressMode::Repeat,
            AddressMode::MirrorRepeat => MTLSamplerAddressMode::MirrorRepeat,
        };
        let descriptor = MTLSamplerDescriptor::new();
        descriptor.setSupportArgumentBuffers(true);
        descriptor.setMinFilter(filter(desc.min_filter));
        descriptor.setMagFilter(filter(desc.mag_filter));
        descriptor.setMipFilter(mip(desc.mip_filter));
        descriptor.setSAddressMode(address(desc.address_u));
        descriptor.setTAddressMode(address(desc.address_v));
        descriptor.setRAddressMode(address(desc.address_u));
        if anisotropy > 1 {
            descriptor.setMaxAnisotropy(anisotropy as usize);
        }
        let raw = self
            .raw
            .newSamplerStateWithDescriptor(&descriptor)
            .expect("a Metal sampler matches its descriptor");
        Sampler { raw }
    }

    pub(crate) fn has_surface(&self) -> bool {
        self.window.is_some()
    }

    pub(crate) fn create_surface(self: &Arc<Self>, width: u32, height: u32) -> Surface {
        let window = self
            .window
            .expect("a Metal drawable belongs to a window device");
        let layer = CAMetalLayer::new();
        layer.setDevice(Some(&*self.raw));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        layer.setFramebufferOnly(true);
        layer.setMaximumDrawableCount(3);
        layer.setDrawableSize(CGSize {
            width: width as f64,
            height: height as f64,
        });
        match window {
            WindowHandle::AppKit { view } => {
                let view = unsafe { &*(view as *const AnyObject) };
                let _: () = unsafe { msg_send![view, setWantsLayer: true] };
                let _: () = unsafe { msg_send![view, setLayer: layer.as_super()] };
                let window: Option<&AnyObject> = unsafe { msg_send![view, window] };
                let scale: f64 = window.map_or(1.0, |window| unsafe {
                    msg_send![window, backingScaleFactor]
                });
                let _: () = unsafe { msg_send![layer.as_super(), setContentsScale: scale] };
            }
            WindowHandle::UiKit { view } => {
                let view = unsafe { &*(view as *const AnyObject) };
                let host: Option<&AnyObject> = unsafe { msg_send![view, layer] };
                let bounds: CGRect = unsafe { msg_send![view, bounds] };
                let _: () = unsafe { msg_send![layer.as_super(), setFrame: bounds] };
                let scale: f64 = unsafe { msg_send![view, contentScaleFactor] };
                let _: () = unsafe { msg_send![layer.as_super(), setContentsScale: scale] };
                if let Some(host) = host {
                    let _: () = unsafe { msg_send![host, addSublayer: layer.as_super()] };
                }
            }
            _ => panic!("a Metal surface needs an AppKit or UIKit view"),
        }
        Surface {
            device: self.clone(),
            layer,
            view: window,
            cache: HashMap::new(),
            width,
            height,
        }
    }

    pub(crate) fn idle(&self) {
        let next = self
            .state
            .lock()
            .expect("the Metal compute queue is never poisoned")
            .next;
        if next > 0 {
            self.wait(next, FRAME_TIMEOUT);
        }
        let mut state = self
            .state
            .lock()
            .expect("the Metal compute queue is never poisoned");
        for frame in &mut state.frames {
            frame.resources.clear();
        }
    }

    pub(crate) fn create_texture_table(
        &self,
        capacity: u32,
        fallback: &crate::Texture,
    ) -> TextureTable {
        let descriptor = MTLArgumentDescriptor::argumentDescriptor();
        descriptor.setDataType(MTLDataType::Texture);
        descriptor.setIndex(0);
        descriptor.setAccess(MTLBindingAccess::ReadOnly);
        descriptor.setTextureType(MTLTextureType::Type2D);
        descriptor.setArrayLength(capacity as usize);
        let encoder = self
            .raw
            .newArgumentEncoderWithArguments(&NSArray::from_retained_slice(&[descriptor]))
            .expect("a Metal argument encoder describes a texture table");
        let length = encoder.encodedLength();
        let buffer = argument_buffer(&self.raw, length);
        let table = TextureTable {
            encoder,
            staging: buffer,
        };
        let texture = native_texture(fallback.native());
        unsafe {
            table
                .encoder
                .setArgumentBuffer_offset(Some(&table.staging), 0);
            for index in 0..capacity as usize {
                table.encoder.setTexture_atIndex(Some(&texture.raw), index);
            }
        }
        table
    }

    pub(crate) fn create_sampler_table(
        &self,
        capacity: u32,
        fallback: &crate::Sampler,
    ) -> SamplerTable {
        let descriptor = MTLArgumentDescriptor::argumentDescriptor();
        descriptor.setDataType(MTLDataType::Sampler);
        descriptor.setIndex(0);
        descriptor.setAccess(MTLBindingAccess::ReadOnly);
        descriptor.setArrayLength(capacity as usize);
        let encoder = self
            .raw
            .newArgumentEncoderWithArguments(&NSArray::from_retained_slice(&[descriptor]))
            .expect("a Metal argument encoder describes a sampler table");
        let length = encoder.encodedLength();
        let buffer = argument_buffer(&self.raw, length);
        let table = SamplerTable {
            encoder,
            staging: buffer,
        };
        let sampler = native_sampler(fallback.native());
        unsafe {
            table
                .encoder
                .setArgumentBuffer_offset(Some(&table.staging), 0);
            for index in 0..capacity as usize {
                table
                    .encoder
                    .setSamplerState_atIndex(Some(&sampler.raw), index);
            }
        }
        table
    }

    pub(crate) fn create_texture(
        self: &Arc<Self>,
        _label: &str,
        width: u32,
        height: u32,
        mip_levels: u32,
        format: TextureFormat,
        usages: TextureUsages,
    ) -> Texture {
        let declared = metal_format(format);
        let descriptor = MTLTextureDescriptor::new();
        descriptor.setTextureType(MTLTextureType::Type2D);
        descriptor.setPixelFormat(declared);
        unsafe {
            descriptor.setWidth(width as usize);
            descriptor.setHeight(height as usize);
            descriptor.setMipmapLevelCount(mip_levels as usize);
        }
        let mut usage = MTLTextureUsage::ShaderRead;
        if usages.contains(TextureUsages::RENDER_ATTACHMENT) {
            usage |= MTLTextureUsage::RenderTarget;
        }
        descriptor.setUsage(usage);
        descriptor.setStorageMode(MTLStorageMode::Private);
        let raw = self
            .raw
            .newTextureWithDescriptor(&descriptor)
            .unwrap_or_else(|| panic!("allocating a Metal texture of {width} by {height} failed"));
        let targets = if usages.contains(TextureUsages::RENDER_ATTACHMENT) {
            (0..mip_levels)
                .map(|level| {
                    let view = if level == 0 {
                        raw.clone()
                    } else {
                        unsafe {
                            raw.newTextureViewWithPixelFormat_textureType_levels_slices(
                                declared,
                                MTLTextureType::Type2D,
                                NSRange {
                                    location: level as usize,
                                    length: 1,
                                },
                                NSRange {
                                    location: 0,
                                    length: 1,
                                },
                            )
                            .unwrap_or_else(|| {
                                panic!("a Metal texture views its own level {level}")
                            })
                        }
                    };
                    Target { view }
                })
                .collect()
        } else {
            Vec::new()
        };
        Texture {
            resource: Arc::new(TextureResource {
                raw,
                targets,
                width,
                height,
                format,
            }),
        }
    }

    pub(crate) fn create_depth_texture(&self, label: &str, width: u32, height: u32) -> Depth {
        let descriptor = MTLTextureDescriptor::new();
        descriptor.setTextureType(MTLTextureType::Type2D);
        descriptor.setPixelFormat(MTLPixelFormat::Depth32Float);
        unsafe {
            descriptor.setWidth(width as usize);
            descriptor.setHeight(height as usize);
        }
        descriptor.setUsage(MTLTextureUsage::RenderTarget);
        descriptor.setStorageMode(MTLStorageMode::Private);
        Depth {
            raw: Arc::new(DepthTexture {
                raw: self
                    .raw
                    .newTextureWithDescriptor(&descriptor)
                    .unwrap_or_else(|| {
                        panic!("allocating the {label} depth texture of {width} by {height} failed")
                    }),
            }),
        }
    }

    pub(crate) fn create_graphics_pipeline(
        self: &Arc<Self>,
        _program: &GraphicsProgram,
        format: TextureFormat,
        state: PipelineState,
        depth: bool,
    ) -> GraphicsPipeline {
        GraphicsPipeline {
            resource: Arc::new(GraphicsPipelineResource {
                format,
                state,
                depth_attachment: depth,
                compiled: OnceLock::new(),
                depth: OnceLock::new(),
            }),
        }
    }

    pub(crate) fn create_group(
        &self,
        _pipeline: &Pipeline,
        _group: u32,
        resources: &[BoundResource],
    ) -> Group {
        Group::of(&self.raw, resources)
    }

    pub(crate) fn create_graphics_group(
        &self,
        _pipeline: &GraphicsPipeline,
        _group: u32,
        resources: &[BoundResource],
    ) -> Group {
        Group::of(&self.raw, resources)
    }

    pub(crate) fn compile_graphics(
        &self,
        pipeline: &GraphicsPipeline,
        program: &GraphicsProgram,
        _artifacts: &ArtifactCache,
    ) {
        pipeline.resource.compiled.get_or_init(|| {
            let GraphicsTranslation::Msl {
                source,
                vertex,
                fragment,
            } = program.translate(Backend::Metal)
            else {
                panic!("Metal accepts MSL graphics programs");
            };
            let library = self
                .raw
                .newLibraryWithSource_options_error(&NSString::from_str(&source), None)
                .unwrap_or_else(|error| {
                    panic!("compiling the MSL of {}: {error}", program.label())
                });
            let vertex_function = library
                .newFunctionWithName(&NSString::from_str(&vertex))
                .unwrap_or_else(|| {
                    panic!(
                        "{} has no Metal vertex entry named {vertex}",
                        program.label()
                    )
                });
            let fragment_function = library
                .newFunctionWithName(&NSString::from_str(&fragment))
                .unwrap_or_else(|| {
                    panic!(
                        "{} has no Metal fragment entry named {fragment}",
                        program.label()
                    )
                });
            let descriptor = MTLRenderPipelineDescriptor::new();
            descriptor.setLabel(Some(&NSString::from_str(program.label())));
            descriptor.setVertexFunction(Some(&vertex_function));
            descriptor.setFragmentFunction(Some(&fragment_function));
            let attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
            attachment.setPixelFormat(metal_format(pipeline.resource.format));
            let (source, destination, source_alpha, destination_alpha) =
                match pipeline.resource.state.blend {
                    BlendState::Opaque => (
                        MTLBlendFactor::One,
                        MTLBlendFactor::Zero,
                        MTLBlendFactor::One,
                        MTLBlendFactor::Zero,
                    ),
                    BlendState::Premultiplied => (
                        MTLBlendFactor::One,
                        MTLBlendFactor::OneMinusSourceAlpha,
                        MTLBlendFactor::OneMinusDestinationAlpha,
                        MTLBlendFactor::One,
                    ),
                    BlendState::Preserve => (
                        MTLBlendFactor::Zero,
                        MTLBlendFactor::One,
                        MTLBlendFactor::Zero,
                        MTLBlendFactor::One,
                    ),
                };
            attachment
                .setBlendingEnabled(!matches!(pipeline.resource.state.blend, BlendState::Opaque));
            attachment.setSourceRGBBlendFactor(source);
            attachment.setDestinationRGBBlendFactor(destination);
            attachment.setRgbBlendOperation(MTLBlendOperation::Add);
            attachment.setSourceAlphaBlendFactor(source_alpha);
            attachment.setDestinationAlphaBlendFactor(destination_alpha);
            attachment.setAlphaBlendOperation(MTLBlendOperation::Add);
            attachment.setWriteMask(if pipeline.resource.state.alpha {
                MTLColorWriteMask::All
            } else {
                MTLColorWriteMask::Red | MTLColorWriteMask::Green | MTLColorWriteMask::Blue
            });
            if pipeline.resource.depth_attachment {
                descriptor.setDepthAttachmentPixelFormat(MTLPixelFormat::Depth32Float);
            }
            self.raw
                .newRenderPipelineStateWithDescriptor_error(&descriptor)
                .unwrap_or_else(|error| {
                    panic!(
                        "creating the Metal graphics pipeline of {}: {error}",
                        program.label()
                    )
                })
        });
        pipeline.resource.depth.get_or_init(|| {
            let descriptor = MTLDepthStencilDescriptor::new();
            descriptor.setDepthCompareFunction(match pipeline.resource.state.depth {
                DepthState::TestWrite | DepthState::Test => MTLCompareFunction::Less,
                DepthState::TestCovered => MTLCompareFunction::LessEqual,
                DepthState::Off => MTLCompareFunction::Always,
            });
            descriptor.setDepthWriteEnabled(matches!(
                pipeline.resource.state.depth,
                DepthState::TestWrite
            ));
            self.raw
                .newDepthStencilStateWithDescriptor(&descriptor)
                .expect("a Metal depth state matches its descriptor")
        });
    }

    pub(crate) fn create_pipeline(self: &Arc<Self>, _program: &ComputeProgram) -> Pipeline {
        Pipeline {
            resource: Arc::new(PipelineResource {
                compiled: OnceLock::new(),
                threads: OnceLock::new(),
            }),
        }
    }

    pub(crate) fn compile(
        &self,
        pipeline: &Pipeline,
        program: &ComputeProgram,
        artifacts: &ArtifactCache,
    ) {
        pipeline.resource.compiled.get_or_init(|| {
            let ShaderTranslation::Msl { source, entry } = program.translate(Backend::Metal) else {
                panic!("Metal accepts MSL compute programs");
            };
            let library = self
                .raw
                .newLibraryWithSource_options_error(&NSString::from_str(&source), None)
                .unwrap_or_else(|error| {
                    panic!("compiling the MSL of {}: {error}", program.label())
                });
            let function = library
                .newFunctionWithName(&NSString::from_str(&entry))
                .unwrap_or_else(|| panic!("{} has no Metal entry named {entry}", program.label()));
            let archive = Archive::of(
                &self.raw,
                artifacts,
                &fingerprint(&[source.as_bytes(), entry.as_bytes()]),
            );
            let descriptor = MTLComputePipelineDescriptor::new();
            descriptor.setLabel(Some(&NSString::from_str(program.label())));
            descriptor.setComputeFunction(Some(&function));
            let archives = archive
                .as_ref()
                .map(|archive| NSArray::from_retained_slice(std::slice::from_ref(&archive.raw)));
            if let Some(archives) = &archives {
                descriptor.setBinaryArchives(Some(archives));
            }
            let compiled = match self
                .raw
                .newComputePipelineStateWithDescriptor_options_reflection_error(
                    &descriptor,
                    MTLPipelineOption::None,
                    None,
                ) {
                Ok(compiled) => compiled,
                Err(error) => {
                    let Some(archive) = &archive else {
                        panic!(
                            "creating the Metal pipeline of {}: {error}",
                            program.label()
                        );
                    };
                    artifacts.discard(&archive.key);
                    descriptor.setBinaryArchives(None);
                    self.raw
                        .newComputePipelineStateWithDescriptor_options_reflection_error(
                            &descriptor,
                            MTLPipelineOption::None,
                            None,
                        )
                        .unwrap_or_else(|error| {
                            panic!(
                                "creating the Metal pipeline of {}: {error}",
                                program.label()
                            )
                        })
                }
            };
            if let Some(archive) = &archive {
                let url =
                    NSURL::fileURLWithPath(&NSString::from_str(&archive.staged.to_string_lossy()));
                if archive
                    .raw
                    .addComputePipelineFunctionsWithDescriptor_error(&descriptor)
                    .is_ok()
                    && archive.raw.serializeToURL_error(&url).is_ok()
                {
                    artifacts.publish(&archive.key, &archive.staged);
                } else {
                    let _ = fs::remove_file(&archive.staged);
                }
            }
            let workgroup = program.workgroup_size();
            assert!(
                workgroup > 0 && workgroup as usize <= compiled.maxTotalThreadsPerThreadgroup(),
                "a Metal compute pipeline cannot schedule its declared workgroup"
            );
            pipeline
                .resource
                .threads
                .set(workgroup)
                .expect("a pipeline stores its workgroup once");
            compiled
        });
    }

    fn blit(
        &self,
        command: &ProtocolObject<dyn MTLCommandBuffer>,
    ) -> Retained<ProtocolObject<dyn MTLBlitCommandEncoder>> {
        command
            .blitCommandEncoder()
            .expect("a Metal command buffer can encode copies")
    }

    pub(crate) fn submit(
        &self,
        writes: &[Write],
        commands: &[Command],
        first_dispatch: bool,
    ) -> u64 {
        let mut state = self
            .state
            .lock()
            .expect("the Metal compute queue is never poisoned");
        self.retire(&mut state)
            .unwrap_or_else(|error| panic!("{error}"));
        if writes.is_empty() && commands.is_empty() {
            return state.next;
        }
        let slot = self.acquire(&mut state);
        state.next = state
            .next
            .checked_add(1)
            .expect("compute submission indices fit in u64");
        let index = state.next;
        let frame = &mut state.frames[slot];
        frame.begin();
        frame.index = index;
        frame.first_dispatch = first_dispatch;
        let command = self
            .queue
            .commandBuffer()
            .expect("a Metal compute command buffer exists");
        let mut presenting = None;
        for write in writes {
            let (upload, offset) = frame.stage(self, &write.bytes, TRANSFER_ALIGNMENT);
            let target = native_buffer(&write.buffer);
            let blit = self.blit(&command);
            unsafe {
                blit.copyFromBuffer_sourceOffset_toBuffer_destinationOffset_size(
                    &upload.raw,
                    offset as usize,
                    &target.raw,
                    write.offset as usize,
                    write.bytes.len(),
                );
            }
            blit.endEncoding();
            frame.resources.push(target.clone());
        }
        for operation in commands {
            match operation {
                Command::Copy {
                    source,
                    source_offset,
                    target,
                    target_offset,
                    bytes,
                } => {
                    let source = buffer(source);
                    let target = buffer(target);
                    let blit = self.blit(&command);
                    unsafe {
                        blit.copyFromBuffer_sourceOffset_toBuffer_destinationOffset_size(
                            &source.raw,
                            *source_offset as usize,
                            &target.raw,
                            *target_offset as usize,
                            *bytes as usize,
                        );
                    }
                    blit.endEncoding();
                    frame.resources.push(source.clone());
                    frame.resources.push(target.clone());
                }
                Command::Clear {
                    buffer: target,
                    offset,
                    bytes,
                } => {
                    let target = buffer(target);
                    let blit = self.blit(&command);
                    blit.fillBuffer_range_value(
                        &target.raw,
                        NSRange {
                            location: *offset as usize,
                            length: *bytes as usize,
                        },
                        0,
                    );
                    blit.endEncoding();
                    frame.resources.push(target.clone());
                }
                Command::Render {
                    target,
                    level,
                    clear,
                    depth,
                    draws,
                } => {
                    let texture = native_texture(target.native());
                    let target = texture
                        .targets
                        .get(*level as usize)
                        .expect("a render level names a level its target holds");
                    let pass = MTLRenderPassDescriptor::renderPassDescriptor();
                    let attachment = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
                    attachment.setTexture(Some(&target.view));
                    attachment.setLoadAction(MTLLoadAction::Clear);
                    attachment.setStoreAction(MTLStoreAction::Store);
                    attachment.setClearColor(MTLClearColor {
                        red: f64::from(clear[0]),
                        green: f64::from(clear[1]),
                        blue: f64::from(clear[2]),
                        alpha: f64::from(clear[3]),
                    });
                    if let Some((held, ops)) = depth {
                        let NativeDepth::Metal(held) = held.native();
                        let attachment = pass.depthAttachment();
                        attachment.setTexture(Some(&held.raw.raw));
                        attachment.setLoadAction(match ops.load {
                            DepthLoad::Clear => MTLLoadAction::Clear,
                            DepthLoad::Keep => MTLLoadAction::Load,
                        });
                        attachment.setStoreAction(if ops.store {
                            MTLStoreAction::Store
                        } else {
                            MTLStoreAction::DontCare
                        });
                        attachment.setClearDepth(1.0);
                        frame
                            .resources
                            .push(Arc::clone(&held.raw) as Arc<dyn Any + Send + Sync>);
                    }
                    let encoder = command
                        .renderCommandEncoderWithDescriptor(&pass)
                        .expect("a Metal command buffer encodes a render pass");
                    let mut declared = std::collections::HashSet::new();
                    for draw in draws {
                        let pipeline = graphics_pipeline(&draw.pipeline.native);
                        encoder.setViewport(MTLViewport {
                            originX: draw.viewport.x as f64,
                            originY: draw.viewport.y as f64,
                            width: draw.viewport.width as f64,
                            height: draw.viewport.height as f64,
                            znear: 0.0,
                            zfar: 1.0,
                        });
                        encoder.setScissorRect(MTLScissorRect {
                            x: draw.scissor.x as usize,
                            y: draw.scissor.y as usize,
                            width: draw.scissor.width as usize,
                            height: draw.scissor.height as usize,
                        });
                        let compiled = pipeline
                            .compiled
                            .get()
                            .expect("a Metal graphics pipeline is compiled before a draw");
                        encoder.setRenderPipelineState(compiled);
                        encoder.setDepthStencilState(Some(
                            pipeline
                                .depth
                                .get()
                                .expect("a Metal graphics pipeline owns a depth state"),
                        ));
                        let mut buffers = 0usize;
                        let mut textures = 0usize;
                        let mut samplers = 0usize;
                        for bound in &draw.groups {
                            claim_group(&bound.native);
                            let tables = group(&bound.native);
                            for (at, resource) in bound.resources.iter().enumerate() {
                                match resource {
                                    BoundResource::Buffer(binding) => {
                                        let target = buffer(&binding.buffer);
                                        let offset = binding.offset as usize;
                                        unsafe {
                                            encoder.setVertexBuffer_offset_atIndex(
                                                Some(&target.raw),
                                                offset,
                                                buffers,
                                            );
                                            encoder.setFragmentBuffer_offset_atIndex(
                                                Some(&target.raw),
                                                offset,
                                                buffers,
                                            );
                                        }
                                        frame.resources.push(target.clone());
                                        buffers += 1;
                                    }
                                    BoundResource::Texture(texture) => {
                                        let texture = native_texture(texture.native());
                                        unsafe {
                                            encoder.setVertexTexture_atIndex(
                                                Some(&texture.raw),
                                                textures,
                                            );
                                            encoder.setFragmentTexture_atIndex(
                                                Some(&texture.raw),
                                                textures,
                                            );
                                        }
                                        frame.resources.push(texture.clone());
                                        textures += 1;
                                    }
                                    BoundResource::Sampler(sampler) => {
                                        let sampler = native_sampler(sampler.native());
                                        unsafe {
                                            encoder.setVertexSamplerState_atIndex(
                                                Some(&sampler.raw),
                                                samplers,
                                            );
                                            encoder.setFragmentSamplerState_atIndex(
                                                Some(&sampler.raw),
                                                samplers,
                                            );
                                        }
                                        samplers += 1;
                                    }
                                    BoundResource::TextureTable(table) => {
                                        let snapshot = tables.buffer(at);
                                        let key = snapshot as *const ProtocolObject<dyn MTLBuffer>
                                            as usize;
                                        if declared.insert(key) {
                                            for texture in table.entries() {
                                                let texture = native_texture(texture.native());
                                                encoder.useResource_usage_stages(
                                                    texture.raw.as_ref(),
                                                    MTLResourceUsage::Read,
                                                    MTLRenderStages::Fragment,
                                                );
                                                frame.resources.push(texture.clone());
                                            }
                                        }
                                        unsafe {
                                            encoder.setVertexBuffer_offset_atIndex(
                                                Some(snapshot),
                                                0,
                                                buffers,
                                            );
                                            encoder.setFragmentBuffer_offset_atIndex(
                                                Some(snapshot),
                                                0,
                                                buffers,
                                            );
                                        }
                                        frame.resources.push(tables.clone());
                                        buffers += 1;
                                    }
                                    BoundResource::SamplerTable(_) => {
                                        let snapshot = tables.buffer(at);
                                        unsafe {
                                            encoder.setVertexBuffer_offset_atIndex(
                                                Some(snapshot),
                                                0,
                                                buffers,
                                            );
                                            encoder.setFragmentBuffer_offset_atIndex(
                                                Some(snapshot),
                                                0,
                                                buffers,
                                            );
                                        }
                                        frame.resources.push(tables.clone());
                                        buffers += 1;
                                    }
                                }
                            }
                        }
                        match &draw.count {
                            RecordedCount::Direct {
                                vertices,
                                instances,
                            } => unsafe {
                                encoder.drawPrimitives_vertexStart_vertexCount_instanceCount(
                                    MTLPrimitiveType::Triangle,
                                    0,
                                    *vertices as usize,
                                    *instances as usize,
                                );
                            },
                            RecordedCount::Indirect { arguments, offset } => {
                                let arguments = buffer(arguments);
                                unsafe {
                                    encoder.drawPrimitives_indirectBuffer_indirectBufferOffset(
                                        MTLPrimitiveType::Triangle,
                                        &arguments.raw,
                                        *offset as usize,
                                    );
                                }
                                frame.resources.push(arguments.clone());
                            }
                        }
                        frame.resources.push(pipeline.clone());
                    }
                    encoder.endEncoding();
                    frame.resources.push(texture.clone());
                }
                Command::WriteTexture {
                    texture,
                    level,
                    bytes,
                } => {
                    let texture = native_texture(texture.native());
                    let width = (texture.width >> level).max(1);
                    let height = (texture.height >> level).max(1);
                    let texel = u64::from(texture.format.bytes_per_texel());
                    let row_bytes = (u64::from(width) * texel).div_ceil(256) * 256;
                    let (staging, offset) = frame.stage(self, bytes, texel.max(TRANSFER_ALIGNMENT));
                    assert!(
                        offset.is_multiple_of(texel),
                        "a texture copy reads the staging buffer at {offset}, which is no multiple of its {texel} byte texel block",
                    );
                    let blit = self.blit(&command);
                    unsafe {
                        blit.copyFromBuffer_sourceOffset_sourceBytesPerRow_sourceBytesPerImage_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                            &staging.raw,
                            offset as usize,
                            row_bytes as usize,
                            (row_bytes * u64::from(height)) as usize,
                            MTLSize {
                                width: width as usize,
                                height: height as usize,
                                depth: 1,
                            },
                            &texture.raw,
                            0,
                            *level as usize,
                            MTLOrigin { x: 0, y: 0, z: 0 },
                        );
                    }
                    blit.endEncoding();
                    frame.resources.push(staging);
                    frame.resources.push(texture.clone());
                }
                Command::CopyTexture {
                    texture,
                    target,
                    target_offset,
                } => {
                    let texture = native_texture(texture.native());
                    let target = buffer(target);
                    let row_bytes = (u64::from(texture.width)
                        * u64::from(texture.format.bytes_per_texel()))
                    .div_ceil(256)
                        * 256;
                    let blit = self.blit(&command);
                    unsafe {
                        blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
                            &texture.raw,
                            0,
                            0,
                            MTLOrigin { x: 0, y: 0, z: 0 },
                            MTLSize {
                                width: texture.width as usize,
                                height: texture.height as usize,
                                depth: 1,
                            },
                            &target.raw,
                            *target_offset as usize,
                            row_bytes as usize,
                            (row_bytes * u64::from(texture.height)) as usize,
                        );
                    }
                    blit.endEncoding();
                    frame.resources.push(texture.clone());
                    frame.resources.push(target.clone());
                }
                Command::Present {
                    texture, native, ..
                } => {
                    let texture = native_texture(texture.native());
                    assert!(
                        !texture.targets.is_empty(),
                        "a presented Metal texture holds its render target"
                    );
                    presenting = Some(native.metal());
                }
                Command::Dispatch {
                    pipeline: handle,
                    group,
                    groups,
                } => {
                    claim_group(&group.native);
                    let pipeline = pipeline(&handle.slot.native);
                    let compiled = pipeline
                        .compiled
                        .get()
                        .expect("a Metal pipeline is compiled before dispatch");
                    let threads = *pipeline
                        .threads
                        .get()
                        .expect("a Metal workgroup size is known");
                    let compute = command
                        .computeCommandEncoder()
                        .expect("a Metal command buffer encodes compute");
                    compute.setComputePipelineState(compiled);
                    let mut buffers = 0usize;
                    let mut textures = 0usize;
                    let mut samplers = 0usize;
                    for resource in &group.resources {
                        match resource {
                            BoundResource::Buffer(binding) => {
                                let target = buffer(&binding.buffer);
                                unsafe {
                                    compute.setBuffer_offset_atIndex(
                                        Some(&target.raw),
                                        binding.offset as usize,
                                        buffers,
                                    )
                                };
                                frame.resources.push(target.clone());
                                buffers += 1;
                            }
                            BoundResource::Texture(texture) => {
                                let texture = native_texture(texture.native());
                                unsafe { compute.setTexture_atIndex(Some(&texture.raw), textures) };
                                frame.resources.push(texture.clone());
                                textures += 1;
                            }
                            BoundResource::Sampler(sampler) => {
                                let sampler = native_sampler(sampler.native());
                                unsafe {
                                    compute.setSamplerState_atIndex(Some(&sampler.raw), samplers)
                                };
                                samplers += 1;
                            }
                            BoundResource::TextureTable(_) | BoundResource::SamplerTable(_) => {
                                panic!("a compute dispatch binds no resource table")
                            }
                        }
                    }
                    compute.dispatchThreadgroups_threadsPerThreadgroup(
                        MTLSize {
                            width: groups[0] as usize,
                            height: groups[1] as usize,
                            depth: groups[2] as usize,
                        },
                        MTLSize {
                            width: threads as usize,
                            height: 1,
                            depth: 1,
                        },
                    );
                    compute.endEncoding();
                    frame.resources.push(pipeline.clone());
                }
            }
        }
        if let Some(present) = presenting {
            present.present(&command);
        }
        let turn = self.gate.take();
        command.commit();
        drop(turn);
        frame.command = Some(command);
        index
    }

    fn acquire(&self, state: &mut QueueState) -> usize {
        if let Some(slot) = state
            .frames
            .iter()
            .position(|frame| frame.index <= state.completed)
        {
            return slot;
        }
        if state.frames.len() < FRAMES_IN_FLIGHT {
            state.frames.push(Frame::new(self));
            return state.frames.len() - 1;
        }
        let slot = state
            .frames
            .iter()
            .enumerate()
            .min_by_key(|(_, frame)| frame.index)
            .map(|(slot, _)| slot)
            .expect("a queue that holds a frame recycles one");
        let index = state.frames[slot].index;
        self.await_completion(state, index, FRAME_TIMEOUT)
            .unwrap_or_else(|error| panic!("{error}"));
        slot
    }

    fn retire(&self, state: &mut QueueState) -> Result<(), String> {
        let mut completed = state.completed;
        while let Some(slot) = state
            .frames
            .iter()
            .position(|frame| frame.index == completed + 1)
        {
            let command = state.frames[slot]
                .command
                .as_ref()
                .expect("a submitted Metal command owns its command buffer");
            match command.status() {
                MTLCommandBufferStatus::Completed => {
                    let index = state.frames[slot].index;
                    let seconds = command.GPUEndTime() - command.GPUStartTime();
                    state.times[index as usize % TIME_SLOTS] = Some((index, seconds));
                    completed += 1;
                }
                MTLCommandBufferStatus::Error => {
                    return Err(format!("Metal compute failed: {:?}", command.error()));
                }
                _ => break,
            }
        }
        state.completed = completed;
        Ok(())
    }

    pub(crate) fn complete(&self, index: u64) -> bool {
        let mut state = self
            .state
            .lock()
            .expect("the Metal compute queue is never poisoned");
        self.retire(&mut state)
            .unwrap_or_else(|error| panic!("{error}"));
        index <= state.completed
    }

    pub(crate) fn completed(&self) -> u64 {
        self.state
            .lock()
            .expect("the Metal compute queue is never poisoned")
            .completed
    }

    pub(crate) fn seconds(&self, index: u64) -> f64 {
        let mut state = self
            .state
            .lock()
            .expect("the Metal compute queue is never poisoned");
        self.await_completion(&mut state, index, FRAME_TIMEOUT)
            .unwrap_or_else(|error| panic!("{error}"));
        match state.times[index as usize % TIME_SLOTS] {
            Some((kept, seconds)) if kept == index => seconds,
            _ => panic!(
                "the device time of submission {index} has left the last {TIME_SLOTS} of this queue"
            ),
        }
    }

    pub(crate) fn wait(&self, index: u64, timeout: Duration) {
        let mut state = self
            .state
            .lock()
            .expect("the Metal compute queue is never poisoned");
        self.await_completion(&mut state, index, timeout)
            .unwrap_or_else(|error| panic!("{error}"));
    }

    fn await_completion(
        &self,
        state: &mut QueueState,
        index: u64,
        timeout: Duration,
    ) -> Result<(), String> {
        if index > state.next {
            return Err("waiting on an unsubmitted Metal command".to_owned());
        }
        self.retire(state)?;
        if index <= state.completed {
            return Ok(());
        }
        let timeout = hang_deadline(
            state
                .frames
                .iter()
                .map(|frame| (frame.index, frame.first_dispatch)),
            index,
            timeout,
        );
        let started = Instant::now();
        loop {
            self.retire(state)?;
            let command = state
                .frames
                .iter()
                .find(|frame| frame.index == index)
                .expect("an unfinished Metal command owns a command buffer")
                .command
                .as_ref()
                .expect("a submitted Metal command owns its command buffer");
            if command.status() == MTLCommandBufferStatus::Completed {
                return Ok(());
            }
            if started.elapsed() >= timeout {
                return Err("waiting for Metal compute work timed out".to_owned());
            }
            if let Some(advance) = self.gate.advance() {
                advance.advance();
            }
            std::thread::sleep(Duration::from_micros(250));
        }
    }

    fn release(&self) {
        let mut state = self
            .state
            .lock()
            .expect("the Metal compute queue is never poisoned");
        let indices = state
            .frames
            .iter()
            .map(|frame| frame.index)
            .collect::<Vec<_>>();
        for index in indices {
            let _ = self.await_completion(&mut state, index, RELEASE_TIMEOUT);
        }
    }

    pub(crate) fn write_host(&self, buffer: &Buffer, offset: u64, bytes: &[u8]) {
        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                buffer
                    .resource
                    .raw
                    .contents()
                    .as_ptr()
                    .cast::<u8>()
                    .add(offset as usize),
                bytes.len(),
            )
        };
    }

    pub(crate) fn read_host(&self, buffer: &Buffer, offset: u64, bytes: u64) -> Vec<u8> {
        unsafe {
            std::slice::from_raw_parts(
                buffer
                    .resource
                    .raw
                    .contents()
                    .as_ptr()
                    .cast::<u8>()
                    .add(offset as usize),
                bytes as usize,
            )
        }
        .to_vec()
    }

    pub(crate) fn read(&self, buffer: &Buffer, bytes: u64) -> Vec<u8> {
        unsafe {
            std::slice::from_raw_parts(
                buffer.resource.raw.contents().as_ptr().cast::<u8>(),
                bytes as usize,
            )
        }
        .to_vec()
    }

    pub(crate) fn assert_alive(&self) {
        let mut state = self
            .state
            .lock()
            .expect("the Metal compute queue is never poisoned");
        if let Err(error) = self.retire(&mut state) {
            panic!("{error}");
        }
    }
}

impl GraphicsPipeline {
    pub(crate) fn is_compiled(&self) -> bool {
        self.resource.compiled.get().is_some()
    }
}

impl Pipeline {
    pub(crate) fn is_compiled(&self) -> bool {
        self.resource.compiled.get().is_some()
    }
}

impl Surface {
    pub(crate) fn width(&self) -> u32 {
        self.width
    }

    pub(crate) fn height(&self) -> u32 {
        self.height
    }

    pub(crate) fn format(&self) -> TextureFormat {
        TextureFormat::Bgra8Unorm
    }

    pub(crate) fn acquire(&mut self) -> Option<NativeFrame> {
        let drawable = self.layer.nextDrawable()?;
        let texture = drawable.texture();
        let width = texture.width() as u32;
        let height = texture.height() as u32;
        let key = Retained::as_ptr(&texture) as usize;
        let native = if let Some(held) = self.cache.get(&key) {
            held.clone()
        } else {
            let native = Texture {
                resource: Arc::new(TextureResource {
                    raw: texture.clone(),
                    targets: vec![Target { view: texture }],
                    width,
                    height,
                    format: TextureFormat::Bgra8Unorm,
                }),
            };
            self.cache.insert(key, native.clone());
            native
        };
        self.width = width;
        self.height = height;
        let drawable = unsafe { Retained::cast_unchecked(drawable) };
        Some(NativeFrame {
            texture: NativeTexture::Metal(native),
            width,
            height,
            format: TextureFormat::Bgra8Unorm,
            present: NativePresent::Metal(Present { drawable }),
        })
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        self.device.idle();
        self.cache.clear();
        self.layer.setDrawableSize(CGSize {
            width: width as f64,
            height: height as f64,
        });
        match self.view {
            WindowHandle::AppKit { view } => {
                let view = unsafe { &*(view as *const AnyObject) };
                let window: Option<&AnyObject> = unsafe { msg_send![view, window] };
                let scale: f64 = window.map_or(1.0, |window| unsafe {
                    msg_send![window, backingScaleFactor]
                });
                let _: () = unsafe { msg_send![self.layer.as_super(), setContentsScale: scale] };
            }
            WindowHandle::UiKit { view } => {
                let view = unsafe { &*(view as *const AnyObject) };
                let bounds: CGRect = unsafe { msg_send![view, bounds] };
                let _: () = unsafe { msg_send![self.layer.as_super(), setFrame: bounds] };
                let scale: f64 = unsafe { msg_send![view, contentScaleFactor] };
                let _: () = unsafe { msg_send![self.layer.as_super(), setContentsScale: scale] };
            }
            _ => unreachable!("a Metal surface keeps an AppKit or UIKit view"),
        }
        self.width = width;
        self.height = height;
    }
}

impl Present {
    pub(crate) fn present(&self, command: &ProtocolObject<dyn MTLCommandBuffer>) {
        command.presentDrawable(&self.drawable);
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        self.release();
    }
}

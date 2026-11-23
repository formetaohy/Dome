use super::{
    DeviceFailure, FRAME_TIMEOUT, FRAMES_IN_FLIGHT, NativeBuffer, NativeFrame,
    NativeGraphicsPipeline, NativeGroup, NativePipeline, NativePresent, NativeTexture,
    STAGING_BYTES, TIME_SLOTS, TRANSFER_ALIGNMENT,
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
use crate::host::{HOST_SLICE, HostAdvance, VulkanHost};
use crate::pipeline::BoundResource;
use crate::sampler::{AddressMode, CompareOp, Filter, SamplerDesc};
use crate::submission::{Command, RecordedCount, Write};
use crate::surface::WindowHandle;
use crate::texture::TextureFormat;
use ash::{Entry, Instance, vk};
use dome_shader::shader::{ComputeProgram, GraphicsProgram, MAX_BINDING_BYTES};
use gpu_allocator::MemoryLocation;
use memory::{Block, Memory};
use std::any::Any;
use std::cmp::Reverse;
use std::ffi::{CStr, CString, c_void};
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

mod memory;

const ACQUIRE_SLOTS: usize = FRAMES_IN_FLIGHT * 2;
const ACQUIRE_TIMEOUT_NANOS: u64 = 1_000_000_000;

struct InstanceOwner {
    entry: Entry,
    instance: Instance,
    surface_loader: ash::khr::surface::Instance,
    surface: Option<vk::SurfaceKHR>,
}

struct Facts {
    limits: Limits,
    ticks: f64,
    bits: u32,
    uuid: [u8; vk::UUID_SIZE],
    portability: bool,
    capability: Option<CooperativeMatrix>,
    bindless: bool,
    swapchain: bool,
    max_sampler_anisotropy: u32,
}

struct Candidate {
    physical: vk::PhysicalDevice,
    family: u32,
    info: AdapterInfo,
    facts: Result<Facts, String>,
}

struct Selection {
    offered: Vec<AdapterInfo>,
    candidates: Vec<Candidate>,
}

struct Parts {
    raw: ash::Device,
    queue: vk::Queue,
    pool: vk::CommandPool,
    stamps: vk::QueryPool,
    physical: vk::PhysicalDevice,
    memory: Arc<Memory>,
    advance: Option<HostAdvance>,
    keep_alive: Option<Arc<dyn Any + Send + Sync>>,
}

const RELEASE_TIMEOUT: Duration = Duration::from_secs(30);
const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;

struct Core {
    raw: ash::Device,
    instance: Option<InstanceOwner>,
    physical: vk::PhysicalDevice,
    memory: Option<Arc<Memory>>,
}

impl std::ops::Deref for Core {
    type Target = ash::Device;

    fn deref(&self) -> &ash::Device {
        &self.raw
    }
}

impl Core {
    fn memory(&self) -> &Arc<Memory> {
        self.memory
            .as_ref()
            .expect("a Vulkan device holds the pool its resources suballocate from")
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        self.memory.take();
        if self.instance.is_some() {
            unsafe { self.raw.destroy_device(None) };
        }
    }
}

impl Drop for InstanceOwner {
    fn drop(&mut self) {
        if let Some(surface) = self.surface {
            unsafe { self.surface_loader.destroy_surface(surface, None) };
        }
        unsafe { self.instance.destroy_instance(None) };
    }
}

impl InstanceOwner {
    fn create(entry: Entry, window: Option<WindowHandle>) -> Result<Self, DeviceFailure> {
        let available =
            complete_enumeration(|| unsafe { entry.enumerate_instance_extension_properties(None) })
                .map_err(|error| format!("enumerating instance extensions: {error:?}"))?;
        let advertised = |name: &CStr| {
            available.iter().any(|extension| unsafe {
                CStr::from_ptr(extension.extension_name.as_ptr()) == name
            })
        };
        let portability = advertised(ash::khr::portability_enumeration::NAME);
        let mut extension_names = Vec::new();
        if portability {
            extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
        }
        if advertised(ash::khr::get_physical_device_properties2::NAME) {
            extension_names.push(ash::khr::get_physical_device_properties2::NAME.as_ptr());
        }
        if let Some(window) = window {
            let platform = match window {
                WindowHandle::Windows { .. } => ash::khr::win32_surface::NAME,
                WindowHandle::Xlib { .. } => ash::khr::xlib_surface::NAME,
                WindowHandle::Wayland { .. } => ash::khr::wayland_surface::NAME,
                WindowHandle::Android { .. } => ash::khr::android_surface::NAME,
                WindowHandle::AppKit { .. } | WindowHandle::UiKit { .. } => {
                    return Err(DeviceFailure::reason(
                        "Vulkan presents no Apple window surface outside MoltenVK",
                    ));
                }
            };
            if !advertised(ash::khr::surface::NAME) || !advertised(platform) {
                return Err(DeviceFailure::reason(format!(
                    "the Vulkan loader offers no {} surface",
                    platform.to_string_lossy()
                )));
            }
            extension_names.push(ash::khr::surface::NAME.as_ptr());
            extension_names.push(platform.as_ptr());
        }
        let diagnostics =
            CString::new("VK_LAYER_KHRONOS_validation").expect("a constant layer name");
        let layers = if cfg!(debug_assertions)
            && complete_enumeration(|| unsafe { entry.enumerate_instance_layer_properties() })
                .map_err(|error| format!("enumerating layers: {error:?}"))?
                .iter()
                .any(|layer| unsafe {
                    CStr::from_ptr(layer.layer_name.as_ptr()) == diagnostics.as_c_str()
                }) {
            vec![diagnostics.as_ptr()]
        } else {
            Vec::new()
        };
        let application = CString::new("dome").expect("a constant application name");
        let loaded = unsafe { entry.try_enumerate_instance_version() }
            .map_err(|error| format!("reading the loader version: {error:?}"))?
            .unwrap_or(vk::API_VERSION_1_0);
        let application_info = vk::ApplicationInfo::default()
            .application_name(&application)
            .api_version(loaded.min(vk::API_VERSION_1_3));
        let flags = if portability {
            vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
        } else {
            vk::InstanceCreateFlags::empty()
        };
        let info = vk::InstanceCreateInfo::default()
            .application_info(&application_info)
            .flags(flags)
            .enabled_extension_names(&extension_names)
            .enabled_layer_names(&layers);
        let raw = unsafe { entry.create_instance(&info, None) }
            .map_err(|error| format!("creating an instance: {error:?}"))?;
        let surface_loader = ash::khr::surface::Instance::new(&entry, &raw);
        let surface = match window {
            None => None,
            Some(window) => match create_window_surface(&entry, &raw, window) {
                Ok(surface) => Some(surface),
                Err(error) => {
                    unsafe { raw.destroy_instance(None) };
                    return Err(error);
                }
            },
        };
        Ok(Self {
            entry,
            instance: raw,
            surface_loader,
            surface,
        })
    }

    fn accepts(&self, physical: vk::PhysicalDevice, family: u32) -> bool {
        self.surface.is_none_or(|surface| {
            unsafe {
                self.surface_loader
                    .get_physical_device_surface_support(physical, family, surface)
            }
            .unwrap_or(false)
        })
    }
}

fn create_window_surface(
    entry: &Entry,
    vulkan: &Instance,
    window: WindowHandle,
) -> Result<vk::SurfaceKHR, DeviceFailure> {
    match window {
        WindowHandle::Windows { window, instance } => {
            let loader = ash::khr::win32_surface::Instance::new(entry, vulkan);
            let info = vk::Win32SurfaceCreateInfoKHR::default()
                .hinstance(instance as isize)
                .hwnd(window as isize);
            unsafe { loader.create_win32_surface(&info, None) }.map_err(|error| {
                DeviceFailure::reason(format!("creating a Win32 surface: {error:?}"))
            })
        }
        WindowHandle::Xlib { display, window } => {
            let loader = ash::khr::xlib_surface::Instance::new(entry, vulkan);
            let info = vk::XlibSurfaceCreateInfoKHR::default()
                .dpy(display as *mut c_void)
                .window(window as _);
            unsafe { loader.create_xlib_surface(&info, None) }.map_err(|error| {
                DeviceFailure::reason(format!("creating an Xlib surface: {error:?}"))
            })
        }
        WindowHandle::Wayland { display, surface } => {
            let loader = ash::khr::wayland_surface::Instance::new(entry, vulkan);
            let info = vk::WaylandSurfaceCreateInfoKHR::default()
                .display(display as *mut c_void)
                .surface(surface as *mut c_void);
            unsafe { loader.create_wayland_surface(&info, None) }.map_err(|error| {
                DeviceFailure::reason(format!("creating a Wayland surface: {error:?}"))
            })
        }
        WindowHandle::Android { window } => {
            let loader = ash::khr::android_surface::Instance::new(entry, vulkan);
            let info = vk::AndroidSurfaceCreateInfoKHR::default().window(window as *mut c_void);
            unsafe { loader.create_android_surface(&info, None) }.map_err(|error| {
                DeviceFailure::reason(format!("creating an Android surface: {error:?}"))
            })
        }
        WindowHandle::AppKit { .. } | WindowHandle::UiKit { .. } => Err(DeviceFailure::reason(
            "Vulkan presents no Apple window surface outside MoltenVK",
        )),
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Access {
    stage: vk::PipelineStageFlags,
    access: vk::AccessFlags,
    written: bool,
}

impl Access {
    fn read(stage: vk::PipelineStageFlags, access: vk::AccessFlags) -> Self {
        Self {
            stage,
            access,
            written: false,
        }
    }

    fn write(stage: vk::PipelineStageFlags, access: vk::AccessFlags) -> Self {
        Self {
            stage,
            access,
            written: true,
        }
    }
}

#[derive(Default)]
struct Pending {
    src_stage: vk::PipelineStageFlags,
    src_access: vk::AccessFlags,
    dst_stage: vk::PipelineStageFlags,
    dst_access: vk::AccessFlags,
    images: Vec<vk::ImageMemoryBarrier<'static>>,
    touched: bool,
}

impl Pending {
    fn touch(&mut self, held: &Mutex<Access>, next: Access) {
        let mut access = held.lock().expect("a Vulkan resource is never poisoned");
        self.record(&mut access, next);
    }

    fn record(&mut self, held: &mut Access, next: Access) {
        if held.written || next.written {
            self.src_stage |= held.stage;
            self.src_access |= held.access;
            self.dst_stage |= next.stage;
            self.dst_access |= next.access;
            self.touched = true;
            *held = next;
            return;
        }
        *held = Access {
            stage: held.stage | next.stage,
            access: held.access | next.access,
            written: false,
        };
    }

    fn transition(
        &mut self,
        texture: &TextureResource,
        next: vk::ImageLayout,
        next_access: Access,
    ) {
        let mut state = texture
            .state
            .lock()
            .expect("a Vulkan texture layout is never poisoned");
        let TextureState { layout, access } = &mut *state;
        if *layout == next {
            self.record(access, next_access);
            return;
        }
        let barrier = vk::ImageMemoryBarrier::default()
            .old_layout(*layout)
            .new_layout(next)
            .image(texture.image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(texture.aspect)
                    .base_mip_level(0)
                    .level_count(texture.mip_levels)
                    .base_array_layer(0)
                    .layer_count(1),
            )
            .src_access_mask(access.access)
            .dst_access_mask(next_access.access);
        self.images.push(barrier);
        self.src_stage |= access.stage;
        self.src_access |= access.access;
        self.dst_stage |= next_access.stage;
        self.dst_access |= next_access.access;
        self.touched = true;
        *layout = next;
        *access = next_access;
    }

    fn flush(&mut self, raw: &ash::Device, command: vk::CommandBuffer) {
        if !self.touched {
            return;
        }
        let src_stage = if self.src_stage.is_empty() {
            vk::PipelineStageFlags::TOP_OF_PIPE
        } else {
            self.src_stage
        };
        let dst_stage = if self.dst_stage.is_empty() {
            vk::PipelineStageFlags::ALL_COMMANDS
        } else {
            self.dst_stage
        };

        let memory = [vk::MemoryBarrier::default()
            .src_access_mask(self.src_access)
            .dst_access_mask(self.dst_access)];
        unsafe {
            raw.cmd_pipeline_barrier(
                command,
                src_stage,
                dst_stage,
                vk::DependencyFlags::empty(),
                &memory,
                &[],
                &self.images,
            )
        };
        self.src_stage = vk::PipelineStageFlags::empty();
        self.src_access = vk::AccessFlags::empty();
        self.dst_stage = vk::PipelineStageFlags::empty();
        self.dst_access = vk::AccessFlags::empty();
        self.images.clear();
        self.touched = false;
    }
}

struct Arena {
    core: Arc<Core>,
    pool: vk::DescriptorPool,
    sets: u32,
}

impl Arena {
    fn new(core: &Arc<Core>, sizes: &[(vk::DescriptorType, u32)], sets: u32) -> Self {
        let hold = sizes
            .iter()
            .map(|(kind, count)| {
                vk::DescriptorPoolSize::default()
                    .ty(*kind)
                    .descriptor_count(count * sets)
            })
            .collect::<Vec<_>>();
        let pool = unsafe {
            core.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(sets)
                    .pool_sizes(&hold),
                None,
            )
        }
        .unwrap_or_else(|error| panic!("creating a Vulkan descriptor arena: {error:?}"));
        Self {
            core: Arc::clone(core),
            pool,
            sets,
        }
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        unsafe { self.core.destroy_descriptor_pool(self.pool, None) };
    }
}

struct Releases {
    submitted: AtomicU64,
    held: Mutex<Vec<(u64, Arc<GroupPool>, vk::DescriptorSet)>>,
}

impl Releases {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            submitted: AtomicU64::new(0),
            held: Mutex::new(Vec::new()),
        })
    }

    fn submitted(&self, index: u64) {
        self.submitted.store(index, Ordering::Release);
    }

    fn hold(&self, pool: &Arc<GroupPool>, set: vk::DescriptorSet) {
        let index = self.submitted.load(Ordering::Acquire);
        self.held
            .lock()
            .expect("the Vulkan descriptor releases are never poisoned")
            .push((index, Arc::clone(pool), set));
    }

    fn advance(&self, completed: u64) {
        let mut held = self
            .held
            .lock()
            .expect("the Vulkan descriptor releases are never poisoned");
        let mut at = 0;
        while at < held.len() {
            if held[at].0 > completed {
                at += 1;
                continue;
            }
            let (_, pool, set) = held.swap_remove(at);
            pool.reclaim(set);
        }
    }

    fn reclaim_all(&self) {
        let mut held = self
            .held
            .lock()
            .expect("the Vulkan descriptor releases are never poisoned");
        for (_, pool, set) in held.drain(..) {
            pool.reclaim(set);
        }
    }
}

struct GroupPool {
    core: Arc<Core>,
    sizes: Vec<(vk::DescriptorType, u32)>,
    free: Mutex<Vec<vk::DescriptorSet>>,
    arenas: Mutex<Vec<Arc<Arena>>>,
    releases: Arc<Releases>,
}

impl GroupPool {
    fn new(
        core: &Arc<Core>,
        sizes: Vec<(vk::DescriptorType, u32)>,
        releases: Arc<Releases>,
    ) -> Arc<Self> {
        Arc::new(Self {
            core: Arc::clone(core),
            sizes,
            free: Mutex::new(Vec::new()),
            arenas: Mutex::new(Vec::new()),
            releases,
        })
    }

    fn set(&self, layout: vk::DescriptorSetLayout) -> vk::DescriptorSet {
        if let Some(set) = self
            .free
            .lock()
            .expect("a Vulkan descriptor free list is never poisoned")
            .pop()
        {
            return set;
        }
        let mut arenas = self
            .arenas
            .lock()
            .expect("the Vulkan descriptor arenas are never poisoned");
        if arenas.is_empty() {
            arenas.push(Arc::new(Arena::new(&self.core, &self.sizes, 16)));
        }
        loop {
            let arena = arenas.last().expect("a descriptor pool holds one arena");
            let layouts = [layout];
            match unsafe {
                self.core.allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(arena.pool)
                        .set_layouts(&layouts),
                )
            } {
                Ok(sets) => return sets[0],
                Err(vk::Result::ERROR_OUT_OF_POOL_MEMORY)
                | Err(vk::Result::ERROR_FRAGMENTED_POOL) => {
                    let sets = arena.sets * 2;
                    arenas.push(Arc::new(Arena::new(&self.core, &self.sizes, sets)));
                }
                Err(error) => panic!("allocating Vulkan descriptors: {error:?}"),
            }
        }
    }

    fn release(self: &Arc<Self>, set: vk::DescriptorSet) {
        self.releases.hold(self, set);
    }

    fn reclaim(&self, set: vk::DescriptorSet) {
        self.free
            .lock()
            .expect("a Vulkan descriptor free list is never poisoned")
            .push(set);
    }
}
struct Frame {
    index: u64,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    staging: Arc<BufferResource>,
    cursor: u64,
    resources: Vec<Arc<dyn Any + Send + Sync>>,
}

impl Frame {
    fn new(device: &Device) -> Self {
        let command = unsafe {
            device.core.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(device.pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
        }
        .unwrap_or_else(|error| panic!("allocating a Vulkan compute command buffer: {error:?}"))[0];
        let fence = unsafe {
            device
                .core
                .create_fence(&vk::FenceCreateInfo::default(), None)
        }
        .unwrap_or_else(|error| panic!("creating a Vulkan completion fence: {error:?}"));
        Self {
            index: 0,
            command,
            fence,
            staging: device.allocate(
                "dome upload",
                STAGING_BYTES,
                vk::BufferUsageFlags::TRANSFER_SRC,
                true,
            ),
            cursor: 0,
            resources: Vec::new(),
        }
    }

    fn begin(&mut self, device: &Device) {
        let raw = &device.core;
        unsafe { raw.reset_command_buffer(self.command, vk::CommandBufferResetFlags::empty()) }
            .unwrap_or_else(|error| panic!("recycling a Vulkan compute command buffer: {error:?}"));
        unsafe { raw.reset_fences(&[self.fence]) }
            .unwrap_or_else(|error| panic!("recycling a Vulkan completion fence: {error:?}"));
        unsafe { raw.begin_command_buffer(self.command, &vk::CommandBufferBeginInfo::default()) }
            .unwrap_or_else(|error| panic!("starting a Vulkan compute submission: {error:?}"));
        self.cursor = 0;
        self.resources.clear();
    }

    fn stage(&mut self, device: &Device, bytes: &[u8], alignment: u64) -> (vk::Buffer, u64) {
        let size = bytes.len() as u64;
        let aligned = self.cursor.next_multiple_of(alignment);
        let (staging, offset) = if STAGING_BYTES
            .checked_sub(aligned)
            .is_some_and(|free| size <= free)
        {
            self.cursor = aligned + size;
            (self.staging.clone(), aligned)
        } else {
            let staging = device.allocate(
                "dome upload",
                size,
                vk::BufferUsageFlags::TRANSFER_SRC,
                true,
            );
            self.resources.push(staging.clone());
            (staging, 0)
        };
        let mapped = staging
            .block
            .mapped()
            .unwrap_or_else(|| panic!("a Vulkan upload buffer holds no host memory"));
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), mapped.add(offset as usize), bytes.len());
        }
        (staging.handle, offset)
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
    queue: vk::Queue,
    pool: vk::CommandPool,
    stamps: vk::QueryPool,
    render_passes: Mutex<Vec<(PassKey, vk::RenderPass)>>,
    ticks: f64,
    bits: u32,
    uuid: [u8; vk::UUID_SIZE],
    state: Mutex<QueueState>,
    releases: Arc<Releases>,
    pub(crate) gate: Arc<Gate>,
    _keep_alive: Option<Arc<dyn Any + Send + Sync>>,
    core: Arc<Core>,
}

struct Lent {
    device: vk::Device,
    queue: vk::Queue,
    native: Weak<Device>,
    info: AdapterInfo,
    limits: Limits,
    capability: Capability,
}

static LENT: LazyLock<Mutex<Vec<Lent>>> = LazyLock::new(|| Mutex::new(Vec::new()));

pub(crate) struct BufferResource {
    block: Block,
    handle: vk::Buffer,
    access: Mutex<Access>,
    core: Arc<Core>,
}

#[derive(Clone)]
pub(crate) struct Buffer {
    resource: Arc<BufferResource>,
}

fn instance_fn<T>(entry: &Entry, instance: vk::Instance, name: &CStr) -> Option<T> {
    let raw = unsafe { entry.get_instance_proc_addr(instance, name.as_ptr()) };
    let raw = raw?;
    Some(unsafe { std::mem::transmute_copy(&raw) })
}

fn descriptor_indexing(entry: &Entry, instance: &Instance, physical: vk::PhysicalDevice) -> bool {
    let Some(features2) = instance_fn::<vk::PFN_vkGetPhysicalDeviceFeatures2>(
        entry,
        instance.handle(),
        c"vkGetPhysicalDeviceFeatures2KHR",
    )
    .or_else(|| instance_fn(entry, instance.handle(), c"vkGetPhysicalDeviceFeatures2")) else {
        return false;
    };
    let mut indexing = vk::PhysicalDeviceDescriptorIndexingFeatures::default();
    let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut indexing);
    unsafe { features2(physical, &mut features) };
    indexing.shader_sampled_image_array_non_uniform_indexing == vk::TRUE
}

fn cooperation(
    entry: &Entry,
    instance: &Instance,
    physical: vk::PhysicalDevice,
    available: &[vk::ExtensionProperties],
) -> Option<CooperativeMatrix> {
    let advertised = available.iter().any(|extension| unsafe {
        CStr::from_ptr(extension.extension_name.as_ptr()) == ash::khr::cooperative_matrix::NAME
    });
    if !advertised {
        return None;
    }
    let properties2: vk::PFN_vkGetPhysicalDeviceProperties2 = instance_fn(
        entry,
        instance.handle(),
        c"vkGetPhysicalDeviceProperties2KHR",
    )
    .or_else(|| instance_fn(entry, instance.handle(), c"vkGetPhysicalDeviceProperties2"))?;
    let features2: vk::PFN_vkGetPhysicalDeviceFeatures2 =
        instance_fn(entry, instance.handle(), c"vkGetPhysicalDeviceFeatures2KHR")
            .or_else(|| instance_fn(entry, instance.handle(), c"vkGetPhysicalDeviceFeatures2"))?;
    let cooperative_properties: vk::PFN_vkGetPhysicalDeviceCooperativeMatrixPropertiesKHR =
        instance_fn(
            entry,
            instance.handle(),
            c"vkGetPhysicalDeviceCooperativeMatrixPropertiesKHR",
        )?;
    let mut subgroup = vk::PhysicalDeviceSubgroupProperties::default();
    let mut stages = vk::PhysicalDeviceCooperativeMatrixPropertiesKHR::default();
    let mut properties = vk::PhysicalDeviceProperties2::default();
    properties = properties.push_next(&mut subgroup).push_next(&mut stages);
    unsafe {
        properties2(physical, &mut properties);
    }
    if !stages
        .cooperative_matrix_supported_stages
        .contains(vk::ShaderStageFlags::COMPUTE)
    {
        return None;
    }
    let mut memory_model = vk::PhysicalDeviceVulkan12Features::default();
    let mut matrix = vk::PhysicalDeviceCooperativeMatrixFeaturesKHR::default();
    let mut features = vk::PhysicalDeviceFeatures2::default();
    features = features.push_next(&mut memory_model).push_next(&mut matrix);
    unsafe {
        features2(physical, &mut features);
    }
    if memory_model.shader_float16 != vk::TRUE
        || memory_model.vulkan_memory_model != vk::TRUE
        || memory_model.vulkan_memory_model_device_scope != vk::TRUE
        || matrix.cooperative_matrix != vk::TRUE
    {
        return None;
    }
    let mut count = 0u32;
    if unsafe { cooperative_properties(physical, &mut count, std::ptr::null_mut()) }
        != vk::Result::SUCCESS
    {
        return None;
    }
    let mut properties = vec![vk::CooperativeMatrixPropertiesKHR::default(); count as usize];
    if count == 0
        || unsafe { cooperative_properties(physical, &mut count, properties.as_mut_ptr()) }
            != vk::Result::SUCCESS
    {
        return None;
    }
    properties.truncate(count as usize);
    let mut best: Option<(u32, u32, u32)> = None;
    for property in properties {
        if property.a_type != vk::ComponentTypeKHR::FLOAT16
            || property.b_type != vk::ComponentTypeKHR::FLOAT16
            || property.c_type != vk::ComponentTypeKHR::FLOAT32
            || property.result_type != vk::ComponentTypeKHR::FLOAT32
            || property.scope != vk::ScopeKHR::SUBGROUP
            || property.saturating_accumulation != vk::FALSE
        {
            continue;
        }
        let shape = (property.m_size, property.n_size, property.k_size);
        let volume =
            |shape: (u32, u32, u32)| u64::from(shape.0) * u64::from(shape.1) * u64::from(shape.2);
        if best.is_none_or(|present| volume(shape) > volume(present)) {
            best = Some(shape);
        }
    }
    let (rows, columns, depth) = best?;
    Some(CooperativeMatrix {
        subgroup: subgroup.subgroup_size,
        rows,
        columns,
        depth,
    })
}

struct PipelineResource {
    core: Arc<Core>,
    layout: vk::PipelineLayout,
    groups: Vec<vk::DescriptorSetLayout>,
    pools: Vec<Arc<GroupPool>>,
    compiled: OnceLock<vk::Pipeline>,
}

pub(crate) struct Pipeline {
    resource: Arc<PipelineResource>,
}

pub(crate) struct Group {
    resource: Arc<GroupResource>,
}

struct GroupResource {
    set: vk::DescriptorSet,
    pool: Arc<GroupPool>,
    buffers: Vec<GroupBuffer>,
    textures: Vec<Arc<TextureResource>>,
    _samplers: Vec<crate::Sampler>,
    _pipeline: Arc<dyn Any + Send + Sync>,
}

struct GroupBuffer {
    resource: Arc<BufferResource>,
    writable: bool,
}

impl GroupBuffer {
    fn access(&self, stages: vk::PipelineStageFlags) -> Access {
        if self.writable {
            Access::write(
                stages,
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            )
        } else {
            Access::read(stages, vk::AccessFlags::SHADER_READ)
        }
    }
}

struct Target {
    view: vk::ImageView,
    owned_view: bool,
    width: u32,
    height: u32,
    framebuffers: Mutex<Vec<Framebuffer>>,
}

struct Framebuffer {
    key: PassKey,
    render_pass: vk::RenderPass,
    raw: vk::Framebuffer,
    depth: Option<Arc<TextureResource>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PassKey {
    format: Option<TextureFormat>,
    depth: Option<(DepthLoad, bool)>,
}

#[derive(Clone, Copy)]
struct Image<'a> {
    name: &'a str,
    width: u32,
    height: u32,
    mip_levels: u32,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
    aspect: vk::ImageAspectFlags,
}

struct TextureState {
    layout: vk::ImageLayout,
    access: Access,
}

struct TextureResource {
    block: Option<Block>,
    image: vk::Image,
    view: vk::ImageView,
    targets: Vec<Target>,
    width: u32,
    height: u32,
    mip_levels: u32,
    format: TextureFormat,
    usages: TextureUsages,
    aspect: vk::ImageAspectFlags,
    present: bool,
    state: Mutex<TextureState>,
    core: Arc<Core>,
}

#[derive(Clone)]
pub(crate) struct Texture {
    resource: Arc<TextureResource>,
}

impl TextureResource {
    fn record(&self, pending: &mut Pending, next: Access) {
        let mut state = self
            .state
            .lock()
            .expect("a Vulkan texture layout is never poisoned");
        pending.record(&mut state.access, next);
    }

    fn sampled_layout(&self) -> vk::ImageLayout {
        if self.aspect.contains(vk::ImageAspectFlags::DEPTH) {
            vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL
        } else {
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        }
    }
}

pub(crate) struct Surface {
    device: Arc<Device>,
    surface: vk::SurfaceKHR,
    loader: ash::khr::swapchain::Device,
    raw: vk::SwapchainKHR,
    images: Vec<Texture>,
    acquire: Vec<vk::Semaphore>,
    acquired: u64,
    present: Vec<vk::Semaphore>,
    width: u32,
    height: u32,
    format: TextureFormat,
    outdated: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Clone)]
pub(crate) struct Present {
    loader: ash::khr::swapchain::Device,
    raw: vk::SwapchainKHR,
    image: u32,
    acquire: vk::Semaphore,
    semaphore: vk::Semaphore,
    outdated: Arc<std::sync::atomic::AtomicBool>,
}

pub(crate) struct Sampler {
    core: Arc<Core>,
    raw: vk::Sampler,
}

pub(crate) struct GraphicsPipeline {
    resource: Arc<GraphicsPipelineResource>,
}

struct GraphicsPipelineResource {
    core: Arc<Core>,
    layout: vk::PipelineLayout,
    groups: Vec<vk::DescriptorSetLayout>,
    pools: Vec<Arc<GroupPool>>,
    render_pass: vk::RenderPass,
    format: Option<TextureFormat>,
    state: PipelineState,
    compiled: OnceLock<vk::Pipeline>,
}

fn native_buffer(native: &NativeBuffer) -> &Arc<BufferResource> {
    match native {
        NativeBuffer::Vulkan(buffer) => &buffer.resource,
    }
}

fn buffer(gpu: &GpuBuffer) -> &Arc<BufferResource> {
    native_buffer(gpu.native())
}

fn native_texture(native: &NativeTexture) -> &Arc<TextureResource> {
    match native {
        NativeTexture::Vulkan(texture) => &texture.resource,
    }
}

fn graphics_pipeline(native: &NativeGraphicsPipeline) -> &Arc<GraphicsPipelineResource> {
    match native {
        NativeGraphicsPipeline::Vulkan(pipeline) => &pipeline.resource,
    }
}

fn pipeline(native: &NativePipeline) -> &Arc<PipelineResource> {
    match native {
        NativePipeline::Vulkan(pipeline) => &pipeline.resource,
    }
}

fn native_sampler(native: &crate::NativeSampler) -> &Sampler {
    match native {
        crate::NativeSampler::Vulkan(sampler) => sampler,
    }
}

fn descriptor_type(kind: &BoundResource) -> vk::DescriptorType {
    match kind {
        BoundResource::Buffer(_) => vk::DescriptorType::STORAGE_BUFFER,
        BoundResource::Texture(_) | BoundResource::TextureTable(_) => {
            vk::DescriptorType::SAMPLED_IMAGE
        }
        BoundResource::Sampler(_) | BoundResource::SamplerTable(_) => vk::DescriptorType::SAMPLER,
    }
}

fn filter(filter: Filter) -> vk::Filter {
    match filter {
        Filter::Nearest => vk::Filter::NEAREST,
        Filter::Linear => vk::Filter::LINEAR,
    }
}

fn mipmap_filter(filter: Filter) -> vk::SamplerMipmapMode {
    match filter {
        Filter::Nearest => vk::SamplerMipmapMode::NEAREST,
        Filter::Linear => vk::SamplerMipmapMode::LINEAR,
    }
}

fn address(mode: AddressMode) -> vk::SamplerAddressMode {
    match mode {
        AddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        AddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
        AddressMode::MirrorRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
    }
}

fn compare_op(compare: CompareOp) -> vk::CompareOp {
    match compare {
        CompareOp::Never => vk::CompareOp::NEVER,
        CompareOp::Less => vk::CompareOp::LESS,
        CompareOp::Equal => vk::CompareOp::EQUAL,
        CompareOp::LessOrEqual => vk::CompareOp::LESS_OR_EQUAL,
        CompareOp::Greater => vk::CompareOp::GREATER,
        CompareOp::NotEqual => vk::CompareOp::NOT_EQUAL,
        CompareOp::GreaterOrEqual => vk::CompareOp::GREATER_OR_EQUAL,
        CompareOp::Always => vk::CompareOp::ALWAYS,
    }
}

fn group_resource(native: &NativeGroup) -> &Arc<GroupResource> {
    match native {
        NativeGroup::Vulkan(group) => &group.resource,
    }
}

fn device_type(device: vk::PhysicalDeviceType) -> DeviceType {
    match device {
        vk::PhysicalDeviceType::DISCRETE_GPU => DeviceType::Discrete,
        vk::PhysicalDeviceType::INTEGRATED_GPU => DeviceType::Integrated,
        vk::PhysicalDeviceType::VIRTUAL_GPU => DeviceType::Virtual,
        vk::PhysicalDeviceType::CPU => DeviceType::Cpu,
        _ => DeviceType::Other,
    }
}

fn identity(props: &vk::PhysicalDeviceProperties) -> AdapterId {
    AdapterId::Numeric {
        vendor: props.vendor_id,
        device: props.device_id,
    }
}

fn describe(props: &vk::PhysicalDeviceProperties, device_type: DeviceType) -> AdapterInfo {
    AdapterInfo {
        name: unsafe { CStr::from_ptr(props.device_name.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
        backend: Backend::Vulkan,
        device_type,
        id: identity(props),
    }
}

fn complete_enumeration<T>(
    mut enumerate: impl FnMut() -> Result<T, vk::Result>,
) -> Result<T, vk::Result> {
    for _ in 0..32 {
        match enumerate() {
            Err(vk::Result::INCOMPLETE) => std::thread::yield_now(),
            result => return result,
        }
    }
    Err(vk::Result::INCOMPLETE)
}

impl Facts {
    fn of(
        entry: &Entry,
        instance: &Instance,
        physical: vk::PhysicalDevice,
        family: u32,
        properties: &vk::PhysicalDeviceProperties,
    ) -> Result<Self, String> {
        let families = unsafe { instance.get_physical_device_queue_family_properties(physical) };
        let Some(queue) = families.get(family as usize) else {
            return Err(format!(
                "the queue family {family} outruns the {} families of this device",
                families.len()
            ));
        };
        if !queue
            .queue_flags
            .contains(vk::QueueFlags::COMPUTE | vk::QueueFlags::GRAPHICS)
        {
            return Err(format!(
                "the queue family {family} carries no combined graphics and compute queue"
            ));
        }
        let bits = queue.timestamp_valid_bits;
        let ticks = f64::from(properties.limits.timestamp_period);
        if bits == 0 || ticks == 0.0 {
            return Err(format!(
                "its compute queue carries no timestamp of {bits} valid bits over {ticks} nanoseconds"
            ));
        }
        let available = complete_enumeration(|| unsafe {
            instance.enumerate_device_extension_properties(physical)
        })
        .map_err(|error| format!("enumerating device extensions: {error:?}"))?;
        let portability = available.iter().any(|extension| unsafe {
            CStr::from_ptr(extension.extension_name.as_ptr()) == ash::khr::portability_subset::NAME
        });
        let swapchain = available.iter().any(|extension| unsafe {
            CStr::from_ptr(extension.extension_name.as_ptr()) == ash::khr::swapchain::NAME
        });
        let memory = unsafe { instance.get_physical_device_memory_properties(physical) };
        let capability = if vk::api_version_major(properties.api_version) > 1
            || vk::api_version_minor(properties.api_version) >= 3
        {
            cooperation(entry, instance, physical, &available)
        } else {
            None
        };
        let bindless = if vk::api_version_major(properties.api_version) > 1
            || (vk::api_version_major(properties.api_version) == 1
                && vk::api_version_minor(properties.api_version) >= 2)
        {
            descriptor_indexing(entry, instance, physical)
        } else {
            false
        };
        let features = unsafe { instance.get_physical_device_features(physical) };
        let mut limits = limits(properties, &memory, allocation(entry, instance, physical));
        let max_sampler_anisotropy = if features.sampler_anisotropy == vk::TRUE {
            limits.max_sampler_anisotropy
        } else {
            1
        };
        limits.max_sampler_anisotropy = max_sampler_anisotropy;
        Ok(Self {
            limits,
            ticks,
            bits,
            uuid: properties.pipeline_cache_uuid,
            portability,
            capability,
            bindless,
            swapchain,
            max_sampler_anisotropy,
        })
    }

    fn assemble(&self, parts: Parts, instance: Option<InstanceOwner>) -> Arc<Device> {
        let core = Arc::new(Core {
            raw: parts.raw,
            instance,
            physical: parts.physical,
            memory: Some(parts.memory),
        });
        Arc::new(Device {
            queue: parts.queue,
            pool: parts.pool,
            stamps: parts.stamps,
            render_passes: Mutex::new(Vec::new()),
            ticks: self.ticks,
            bits: self.bits,
            uuid: self.uuid,
            state: Mutex::new(QueueState::default()),
            releases: Releases::new(),
            gate: Gate::of(parts.advance),
            _keep_alive: parts.keep_alive,
            core,
        })
    }
}

fn pools(device: &ash::Device, family: u32) -> Result<(vk::CommandPool, vk::QueryPool), String> {
    let pool = unsafe {
        device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )
    }
    .map_err(|error| format!("creating a compute command pool failed: {error:?}"))?;
    let stamps = match unsafe {
        device.create_query_pool(
            &vk::QueryPoolCreateInfo::default()
                .query_type(vk::QueryType::TIMESTAMP)
                .query_count(2 * FRAMES_IN_FLIGHT as u32),
            None,
        )
    } {
        Ok(stamps) => stamps,
        Err(error) => {
            unsafe { device.destroy_command_pool(pool, None) };
            return Err(format!("creating a timestamp pool failed: {error:?}"));
        }
    };
    Ok((pool, stamps))
}

fn create(
    instance: &Instance,
    physical: vk::PhysicalDevice,
    family: u32,
    facts: &Facts,
    swapchain: bool,
) -> Result<Parts, String> {
    if swapchain && !facts.swapchain {
        return Err("VK_KHR_swapchain is missing".to_owned());
    }
    let priority = [1.0];
    let queues = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(family)
        .queue_priorities(&priority)];
    let cooperative = facts.capability.is_some();
    let mut extensions = if facts.portability {
        vec![ash::khr::portability_subset::NAME.as_ptr()]
    } else {
        Vec::new()
    };
    if cooperative {
        extensions.push(ash::khr::cooperative_matrix::NAME.as_ptr());
    }
    if swapchain {
        extensions.push(ash::khr::swapchain::NAME.as_ptr());
    }
    let mut memory_model = vk::PhysicalDeviceVulkan12Features::default();
    let mut matrix = vk::PhysicalDeviceCooperativeMatrixFeaturesKHR::default();
    if cooperative {
        memory_model.shader_float16 = vk::TRUE;
        memory_model.vulkan_memory_model = vk::TRUE;
        memory_model.vulkan_memory_model_device_scope = vk::TRUE;
        matrix.cooperative_matrix = vk::TRUE;
    }
    if facts.bindless {
        memory_model.shader_sampled_image_array_non_uniform_indexing = vk::TRUE;
    }
    let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut memory_model);
    if cooperative {
        features = features.push_next(&mut matrix);
    }
    let core = vk::PhysicalDeviceFeatures {
        fragment_stores_and_atomics: vk::TRUE,
        sampler_anisotropy: if facts.max_sampler_anisotropy > 1 {
            vk::TRUE
        } else {
            vk::FALSE
        },
        ..Default::default()
    };
    let mut config = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queues)
        .enabled_extension_names(&extensions);
    if cooperative || facts.bindless {
        features.features = core;
        config = config.push_next(&mut features);
    } else {
        config = config.enabled_features(&core);
    }
    let raw = unsafe { instance.create_device(physical, &config, None) }
        .map_err(|error| format!("creating a compute device failed: {error:?}"))?;
    let queue = unsafe { raw.get_device_queue(family, 0) };
    let (pool, stamps) = match pools(&raw, family) {
        Ok(pools) => pools,
        Err(reason) => {
            unsafe { raw.destroy_device(None) };
            return Err(reason);
        }
    };
    let memory = Memory::new(instance, &raw, physical);
    Ok(Parts {
        raw,
        queue,
        pool,
        stamps,
        physical,
        memory,
        advance: None,
        keep_alive: None,
    })
}

fn depth_load_access(load: Option<DepthLoad>) -> vk::AccessFlags {
    match load {
        Some(DepthLoad::Keep) => vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
        _ => vk::AccessFlags::empty(),
    }
}

fn render_pass(raw: &ash::Device, key: PassKey) -> Result<vk::RenderPass, String> {
    let color_stages = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
        | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS;
    let color_access =
        vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE;
    let dependencies = [vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(color_stages)
        .dst_stage_mask(color_stages)
        .dst_access_mask(color_access)];
    let references = [vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
    let depth_attachment = key.depth.map(|_| if key.format.is_some() { 1 } else { 0 });
    let depth_reference = depth_attachment.map(|attachment| {
        vk::AttachmentReference::default()
            .attachment(attachment)
            .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
    });
    let mut attachments = Vec::with_capacity(2);
    if let Some(format) = key.format {
        attachments.push(
            vk::AttachmentDescription::default()
                .format(vk_format(format))
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL),
        );
    }
    if let Some((load, store)) = key.depth {
        attachments.push(
            vk::AttachmentDescription::default()
                .format(DEPTH_FORMAT)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(match load {
                    DepthLoad::Clear => vk::AttachmentLoadOp::CLEAR,
                    DepthLoad::Keep => vk::AttachmentLoadOp::LOAD,
                })
                .store_op(if store {
                    vk::AttachmentStoreOp::STORE
                } else {
                    vk::AttachmentStoreOp::DONT_CARE
                })
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(match load {
                    DepthLoad::Clear => vk::ImageLayout::UNDEFINED,
                    DepthLoad::Keep => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                })
                .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
        );
    }
    let mut subpass =
        vk::SubpassDescription::default().pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS);
    if key.format.is_some() {
        subpass = subpass.color_attachments(&references);
    }
    if let Some(reference) = &depth_reference {
        subpass = subpass.depth_stencil_attachment(reference);
    }
    unsafe {
        raw.create_render_pass(
            &vk::RenderPassCreateInfo::default()
                .attachments(&attachments)
                .subpasses(std::slice::from_ref(&subpass))
                .dependencies(&dependencies),
            None,
        )
    }
    .map_err(|error| format!("creating the Vulkan render pass: {error:?}"))
}

fn select(owner: &InstanceOwner, policy: AdapterPolicy) -> Result<Selection, DeviceFailure> {
    let physical_devices =
        complete_enumeration(|| unsafe { owner.instance.enumerate_physical_devices() })
            .map_err(|error| format!("enumerating devices: {error:?}"))?;
    let mut candidates = Vec::new();
    for physical in physical_devices {
        let properties = unsafe { owner.instance.get_physical_device_properties(physical) };
        if vk::api_version_major(properties.api_version) < 1
            || (vk::api_version_major(properties.api_version) == 1
                && vk::api_version_minor(properties.api_version) < 1)
        {
            continue;
        }
        let families = unsafe {
            owner
                .instance
                .get_physical_device_queue_family_properties(physical)
        };
        let Some(family) = families
            .iter()
            .enumerate()
            .filter(|(_, queue)| {
                queue.queue_count > 0
                    && queue
                        .queue_flags
                        .contains(vk::QueueFlags::COMPUTE | vk::QueueFlags::GRAPHICS)
            })
            .map(|(index, _)| index as u32)
            .find(|family| owner.accepts(physical, *family))
        else {
            continue;
        };
        candidates.push(Candidate {
            physical,
            family,
            info: describe(&properties, device_type(properties.device_type)),
            facts: Facts::of(&owner.entry, &owner.instance, physical, family, &properties),
        });
    }
    let offered = candidates
        .iter()
        .map(|candidate| candidate.info.clone())
        .collect::<Vec<_>>();
    candidates.retain(|candidate| policy.wants(candidate.info.id));
    if let AdapterPolicy::Power(preference) = policy {
        candidates.sort_by_key(|candidate| {
            Reverse((
                candidate
                    .facts
                    .as_ref()
                    .is_ok_and(|facts| facts.limits.supports(&Limits::BASELINE)),
                candidate.info.device_type.rank(preference),
            ))
        });
    }
    Ok(Selection {
        offered,
        candidates,
    })
}

impl Device {
    pub(crate) fn open(
        policy: AdapterPolicy,
        window: Option<WindowHandle>,
    ) -> Result<(Arc<Self>, AdapterInfo, Limits, Capability), DeviceFailure> {
        let entry = unsafe { Entry::load() }.map_err(|error| error.to_string())?;
        let owner = InstanceOwner::create(entry, window)?;
        let swapchain = owner.surface.is_some();
        let Selection {
            offered,
            candidates,
        } = select(&owner, policy)?;
        if candidates.is_empty() {
            return Err(match policy {
                AdapterPolicy::Identity(wanted)
                    if offered.iter().any(|adapter| adapter.id == wanted) =>
                {
                    DeviceFailure::reason(format!(
                        "the requested adapter {wanted} cannot run the compute baseline"
                    ))
                }
                AdapterPolicy::Identity(_) => DeviceFailure::missing(offered),
                AdapterPolicy::Power(_) => {
                    DeviceFailure::reason("no Vulkan 1.1 device carries a compute queue")
                }
            });
        }
        let mut failures = Vec::new();
        for Candidate {
            physical,
            family,
            info,
            facts,
        } in candidates
        {
            let facts = match facts {
                Ok(facts) => facts,
                Err(reason) => {
                    failures.push(format!("{}: {reason}", info.name));
                    continue;
                }
            };
            let capability = facts.capability;
            match create(&owner.instance, physical, family, &facts, swapchain) {
                Ok(parts) => {
                    return Ok((
                        facts.assemble(parts, Some(owner)),
                        info,
                        facts.limits,
                        Capability {
                            cooperative_matrix: capability,
                            bindless: facts.bindless,
                        },
                    ));
                }
                Err(reason) => failures.push(format!("{}: {reason}", info.name)),
            }
        }
        Err(DeviceFailure::reason(if failures.is_empty() {
            "no Vulkan 1.1 device with a compute queue".to_owned()
        } else {
            failures.join("; ")
        }))
    }

    pub(crate) fn borrow(
        host: VulkanHost<'_>,
        advance: Option<HostAdvance>,
        keep_alive: Option<Arc<dyn Any + Send + Sync>>,
    ) -> Result<(Arc<Self>, AdapterInfo, Limits, Capability), DeviceFailure> {
        let mut lent = LENT
            .lock()
            .expect("the borrowed Vulkan devices are never poisoned");
        lent.retain(|entry| entry.native.strong_count() > 0);
        if let Some(entry) = lent
            .iter()
            .find(|entry| entry.device == host.device.handle() && entry.queue == host.queue)
            && let Some(device) = entry.native.upgrade()
        {
            device.gate.advancing(advance);
            return Ok((device, entry.info.clone(), entry.limits, entry.capability));
        }
        let properties = unsafe { host.instance.get_physical_device_properties(host.physical) };
        let info = describe(&properties, device_type(properties.device_type));
        let facts = Facts::of(
            host.entry,
            host.instance,
            host.physical,
            host.queue_family,
            &properties,
        )
        .map_err(DeviceFailure::reason)?;
        let capability = if host.cooperative_matrix {
            Some(facts.capability.ok_or_else(|| {
                DeviceFailure::reason(format!(
                    "the host claims cooperative matrices, and {} cannot run them",
                    info.name
                ))
            })?)
        } else {
            None
        };
        let (pool, stamps) =
            pools(host.device, host.queue_family).map_err(DeviceFailure::reason)?;
        let memory = Memory::new(host.instance, host.device, host.physical);
        let parts = Parts {
            raw: host.device.clone(),
            queue: host.queue,
            pool,
            stamps,
            physical: host.physical,
            memory,
            advance,
            keep_alive,
        };
        let device = facts.assemble(parts, None);
        let capability = Capability {
            cooperative_matrix: capability,
            bindless: facts.bindless,
        };
        lent.push(Lent {
            device: host.device.handle(),
            queue: host.queue,
            native: Arc::downgrade(&device),
            info: info.clone(),
            limits: facts.limits,
            capability,
        });
        Ok((device, info, facts.limits, capability))
    }

    pub(crate) fn compile(
        &self,
        pipeline: &Pipeline,
        program: &ComputeProgram,
        artifacts: &ArtifactCache,
    ) {
        pipeline.resource.compiled.get_or_init(|| {
            let key = format!(
                "vulkan/{}/{}.bin",
                uuids(&self.uuid),
                fingerprint(&[&bytes_of(program.spirv())]),
            );
            let initial = artifacts.load(&key);
            let pipeline_cache = pipeline_cache(&self.core, initial.as_deref());
            let spirv = program.spirv();
            let module = unsafe {
                self.core
                    .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(spirv), None)
            }
            .unwrap_or_else(|error| {
                panic!(
                    "creating the Vulkan SPIR-V module of {}: {error:?}",
                    program.label()
                )
            });
            let entry =
                CString::new(program.entry()).expect("a compute entry point has no zero byte");
            let stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(module)
                .name(&entry);
            let pipelines = unsafe {
                self.core.create_compute_pipelines(
                    pipeline_cache,
                    &[vk::ComputePipelineCreateInfo::default()
                        .stage(stage)
                        .layout(pipeline.resource.layout)],
                    None,
                )
            };
            unsafe { self.core.destroy_shader_module(module, None) };
            let compiled = pipelines.unwrap_or_else(|(_, error)| {
                panic!(
                    "compiling Vulkan compute pipeline {}: {error:?}",
                    program.label()
                )
            })[0];
            let data = unsafe { self.core.get_pipeline_cache_data(pipeline_cache) }.unwrap_or_else(
                |error| {
                    panic!(
                        "reading the Vulkan pipeline cache of {}: {error:?}",
                        program.label()
                    )
                },
            );
            artifacts.store(&key, &data);
            unsafe { self.core.destroy_pipeline_cache(pipeline_cache, None) };
            compiled
        });
    }

    fn allocate(
        &self,
        name: &str,
        size: u64,
        usage: vk::BufferUsageFlags,
        host: bool,
    ) -> Arc<BufferResource> {
        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let handle = unsafe { self.core.create_buffer(&info, None) }
            .unwrap_or_else(|error| panic!("allocating {size} Vulkan buffer bytes: {error:?}"));
        let requirements = unsafe { self.core.get_buffer_memory_requirements(handle) };
        let location = if host {
            MemoryLocation::CpuToGpu
        } else {
            MemoryLocation::GpuOnly
        };
        let block = self
            .core
            .memory()
            .allocate(name, requirements, location, true);
        unsafe {
            self.core
                .bind_buffer_memory(handle, block.memory(), block.offset())
        }
        .unwrap_or_else(|error| panic!("binding Vulkan device memory: {error:?}"));
        Arc::new(BufferResource {
            block,
            core: self.core.clone(),
            handle,
            access: Mutex::new(Access::default()),
        })
    }

    pub(crate) fn create_buffer(
        self: &Arc<Self>,
        label: &str,
        size: u64,
        usage: BufferUsages,
    ) -> Buffer {
        let mut flags = vk::BufferUsageFlags::empty();
        if usage.contains(BufferUsages::STORAGE) {
            flags |= vk::BufferUsageFlags::STORAGE_BUFFER;
        }
        if usage.contains(BufferUsages::COPY_SRC) {
            flags |= vk::BufferUsageFlags::TRANSFER_SRC;
        }
        if usage.contains(BufferUsages::COPY_DST) {
            flags |= vk::BufferUsageFlags::TRANSFER_DST;
        }
        if usage.contains(BufferUsages::INDIRECT) {
            flags |= vk::BufferUsageFlags::INDIRECT_BUFFER;
        }
        Buffer {
            resource: self.allocate(
                label,
                size,
                flags,
                usage.contains(BufferUsages::MAP_READ) || usage.contains(BufferUsages::MAP_WRITE),
            ),
        }
    }

    pub(crate) fn create_pipeline(self: &Arc<Self>, program: &ComputeProgram) -> Pipeline {
        let (groups, pools, layout) = self.pipeline_layout(
            program.bindings(),
            vk::ShaderStageFlags::COMPUTE,
            program.label(),
        );
        Pipeline {
            resource: Arc::new(PipelineResource {
                core: self.core.clone(),
                layout,
                groups,
                pools,
                compiled: OnceLock::new(),
            }),
        }
    }

    fn pipeline_layout(
        &self,
        bindings: &[dome_shader::shader::BindingSpec],
        stages: vk::ShaderStageFlags,
        label: &str,
    ) -> (
        Vec<vk::DescriptorSetLayout>,
        Vec<Arc<GroupPool>>,
        vk::PipelineLayout,
    ) {
        let count = bindings
            .iter()
            .map(|binding| binding.group)
            .max()
            .map_or(0, |group| group + 1);
        let mut groups = Vec::new();
        let mut pools = Vec::new();
        for group in 0..count {
            let bindings = bindings
                .iter()
                .filter(|binding| binding.group == group)
                .map(|spec| {
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(spec.binding)
                        .descriptor_type(match spec.kind {
                            dome_shader::shader::BindingKind::SampledImage
                            | dome_shader::shader::BindingKind::DepthImage
                            | dome_shader::shader::BindingKind::SampledImageTable => {
                                vk::DescriptorType::SAMPLED_IMAGE
                            }
                            dome_shader::shader::BindingKind::Sampler
                            | dome_shader::shader::BindingKind::ComparisonSampler
                            | dome_shader::shader::BindingKind::SamplerTable => {
                                vk::DescriptorType::SAMPLER
                            }
                            _ => vk::DescriptorType::STORAGE_BUFFER,
                        })
                        .descriptor_count(spec.count)
                        .stage_flags(stages)
                })
                .collect::<Vec<_>>();
            let layout = unsafe {
                self.core.create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
            }
            .unwrap_or_else(|error| {
                panic!("creating group {group} of {label}'s Vulkan layout: {error:?}")
            });
            groups.push(layout);
            let mut sizes = Vec::new();
            for binding in &bindings {
                match sizes
                    .iter_mut()
                    .find(|(kind, _)| *kind == binding.descriptor_type)
                {
                    Some((_, count)) => *count += binding.descriptor_count,
                    None => sizes.push((binding.descriptor_type, binding.descriptor_count)),
                }
            }
            pools.push(GroupPool::new(
                &self.core,
                sizes,
                Arc::clone(&self.releases),
            ));
        }
        let layout = unsafe {
            self.core.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&groups),
                None,
            )
        }
        .unwrap_or_else(|error| panic!("creating {label}'s Vulkan pipeline layout: {error:?}"));
        (groups, pools, layout)
    }

    pub(crate) fn create_group(
        self: &Arc<Self>,
        pipeline: &Pipeline,
        group: u32,
        resources: &[BoundResource],
    ) -> Group {
        self.assemble_group(
            pipeline.resource.groups[group as usize],
            Arc::clone(&pipeline.resource.pools[group as usize]),
            resources,
            pipeline.resource.clone(),
        )
    }

    pub(crate) fn create_graphics_group(
        self: &Arc<Self>,
        pipeline: &GraphicsPipeline,
        group: u32,
        resources: &[BoundResource],
    ) -> Group {
        self.assemble_group(
            pipeline.resource.groups[group as usize],
            Arc::clone(&pipeline.resource.pools[group as usize]),
            resources,
            pipeline.resource.clone(),
        )
    }

    fn assemble_group(
        &self,
        layout: vk::DescriptorSetLayout,
        pool: Arc<GroupPool>,
        resources: &[BoundResource],
        pipeline: Arc<dyn Any + Send + Sync>,
    ) -> Group {
        let set = pool.set(layout);
        let mut buffers = Vec::new();
        let mut textures = Vec::new();
        let mut samplers = Vec::new();
        let mut buffer_infos = Vec::new();
        let mut image_infos = Vec::new();
        let mut sources = Vec::new();
        for resource in resources {
            match resource {
                BoundResource::Buffer(binding) => {
                    sources.push((descriptor_type(resource), buffer_infos.len(), 1));
                    buffers.push(GroupBuffer {
                        resource: buffer(&binding.buffer).clone(),
                        writable: binding.writable,
                    });
                    buffer_infos.push(
                        vk::DescriptorBufferInfo::default()
                            .buffer(buffer(&binding.buffer).handle)
                            .offset(binding.offset)
                            .range(binding.size),
                    );
                }
                BoundResource::Texture(texture) => {
                    let native = native_texture(texture.native());
                    sources.push((descriptor_type(resource), image_infos.len(), 1));
                    textures.push(native.clone());
                    image_infos.push(
                        vk::DescriptorImageInfo::default()
                            .image_view(native.view)
                            .image_layout(native.sampled_layout()),
                    );
                }
                BoundResource::Sampler(sampler) => {
                    sources.push((descriptor_type(resource), image_infos.len(), 1));
                    samplers.push(sampler.clone());
                    image_infos.push(
                        vk::DescriptorImageInfo::default()
                            .sampler(native_sampler(sampler.native()).raw),
                    );
                }
                BoundResource::TextureTable(table) => {
                    match table.native() {
                        super::NativeTextureTable::Vulkan => {}
                    }
                    sources.push((
                        descriptor_type(resource),
                        image_infos.len(),
                        table.capacity() as usize,
                    ));
                    for texture in table.entries() {
                        let native = native_texture(texture.native());
                        textures.push(native.clone());
                        image_infos.push(
                            vk::DescriptorImageInfo::default()
                                .image_view(native.view)
                                .image_layout(native.sampled_layout()),
                        );
                    }
                }
                BoundResource::SamplerTable(table) => {
                    match table.native() {
                        super::NativeSamplerTable::Vulkan => {}
                    }
                    sources.push((
                        descriptor_type(resource),
                        image_infos.len(),
                        table.capacity() as usize,
                    ));
                    for sampler in table.entries() {
                        samplers.push(sampler.clone());
                        image_infos.push(
                            vk::DescriptorImageInfo::default()
                                .sampler(native_sampler(sampler.native()).raw),
                        );
                    }
                }
            }
        }
        let writes = sources
            .iter()
            .enumerate()
            .map(|(index, (ty, at, count))| {
                let write = vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(index as u32)
                    .descriptor_type(*ty);
                match *ty {
                    vk::DescriptorType::STORAGE_BUFFER => {
                        write.buffer_info(&buffer_infos[*at..*at + count])
                    }
                    _ => write.image_info(&image_infos[*at..*at + count]),
                }
            })
            .collect::<Vec<_>>();
        unsafe { self.core.update_descriptor_sets(&writes, &[]) };
        Group {
            resource: Arc::new(GroupResource {
                set,
                pool,
                buffers,
                textures,
                _samplers: samplers,
                _pipeline: pipeline,
            }),
        }
    }

    pub(crate) fn create_sampler(&self, desc: &SamplerDesc, anisotropy: u32) -> Sampler {
        let mut info = vk::SamplerCreateInfo::default()
            .mag_filter(filter(desc.mag_filter))
            .min_filter(filter(desc.min_filter))
            .mipmap_mode(mipmap_filter(desc.mip_filter))
            .address_mode_u(address(desc.address_u))
            .address_mode_v(address(desc.address_v))
            .address_mode_w(address(desc.address_u))
            .max_lod(vk::LOD_CLAMP_NONE);
        if let Some(compare) = desc.compare {
            info = info.compare_enable(true).compare_op(compare_op(compare));
        }
        if anisotropy > 1 {
            info = info
                .anisotropy_enable(true)
                .max_anisotropy(anisotropy as f32);
        }
        let raw = unsafe { self.core.create_sampler(&info, None) }
            .unwrap_or_else(|error| panic!("creating a Vulkan sampler: {error:?}"));
        Sampler {
            core: self.core.clone(),
            raw,
        }
    }

    pub(crate) fn create_texture(
        self: &Arc<Self>,
        label: &str,
        width: u32,
        height: u32,
        mip_levels: u32,
        format: TextureFormat,
        usages: TextureUsages,
    ) -> Texture {
        let declared = vk_format(format);
        let aspect = if format.is_depth() {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::COLOR
        };
        let mut flags = vk::ImageUsageFlags::empty();
        if usages.contains(TextureUsages::SAMPLED) {
            flags |= vk::ImageUsageFlags::SAMPLED;
        }
        if usages.contains(TextureUsages::COPY_SRC) {
            flags |= vk::ImageUsageFlags::TRANSFER_SRC;
        }
        if usages.contains(TextureUsages::COPY_DST) {
            flags |= vk::ImageUsageFlags::TRANSFER_DST;
        }
        if usages.contains(TextureUsages::RENDER_ATTACHMENT) {
            flags |= if format.is_depth() {
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
            } else {
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC
            };
        }
        let (image, block, view) = self.attachment(Image {
            name: label,
            width,
            height,
            mip_levels,
            format: declared,
            usage: flags,
            aspect,
        });
        let targets = if usages.contains(TextureUsages::RENDER_ATTACHMENT) {
            (0..mip_levels)
                .map(|level| {
                    let extent = ((width >> level).max(1), (height >> level).max(1));
                    let shared = level == 0 && mip_levels == 1;
                    let level_view = if shared {
                        view
                    } else {
                        unsafe {
                            self.core
                                .create_image_view(
                                    &vk::ImageViewCreateInfo::default()
                                        .image(image)
                                        .view_type(vk::ImageViewType::TYPE_2D)
                                        .format(declared)
                                        .subresource_range(vk::ImageSubresourceRange {
                                            aspect_mask: aspect,
                                            base_mip_level: level,
                                            level_count: 1,
                                            base_array_layer: 0,
                                            layer_count: 1,
                                        }),
                                    None,
                                )
                                .unwrap_or_else(|error| {
                                    panic!("creating the Vulkan view of level {level}: {error:?}")
                                })
                        }
                    };
                    Target {
                        view: level_view,
                        owned_view: !shared,
                        width: extent.0,
                        height: extent.1,
                        framebuffers: Mutex::new(Vec::new()),
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        Texture {
            resource: Arc::new(TextureResource {
                block: Some(block),
                core: self.core.clone(),
                image,
                view,
                targets,
                width,
                height,
                mip_levels,
                format,
                usages,
                aspect,
                present: false,
                state: Mutex::new(TextureState {
                    layout: vk::ImageLayout::UNDEFINED,
                    access: Access::default(),
                }),
            }),
        }
    }

    pub(crate) fn create_texture_view(&self, texture: &Texture, level: u32) -> Texture {
        let held = &texture.resource;
        assert!(
            level < held.mip_levels,
            "a texture of {} levels views no level {level}",
            held.mip_levels,
        );
        let width = (held.width >> level).max(1);
        let height = (held.height >> level).max(1);
        let view = unsafe {
            self.core
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(held.image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk_format(held.format))
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: held.aspect,
                            base_mip_level: level,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )
                .unwrap_or_else(|error| {
                    panic!("creating the Vulkan view of level {level}: {error:?}")
                })
        };
        Texture {
            resource: Arc::new(TextureResource {
                block: None,
                core: self.core.clone(),
                image: held.image,
                view,
                targets: Vec::new(),
                width,
                height,
                mip_levels: 1,
                format: held.format,
                usages: held.usages,
                aspect: held.aspect,
                present: false,
                state: Mutex::new(TextureState {
                    layout: vk::ImageLayout::UNDEFINED,
                    access: Access::default(),
                }),
            }),
        }
    }

    fn framing(
        &self,
        color: Option<&Target>,
        key: PassKey,
        depth: Option<&Arc<TextureResource>>,
    ) -> (vk::RenderPass, vk::Framebuffer) {
        let render_pass = self.render_pass(key);
        let (cache, width, height) = match color {
            Some(target) => (&target.framebuffers, target.width, target.height),
            None => {
                let held = depth.expect("a depth only pass names its depth texture");
                let target = held
                    .targets
                    .first()
                    .expect("a depth attachment renders into its own level");
                (&target.framebuffers, held.width, held.height)
            }
        };
        let mut held = cache
            .lock()
            .expect("the Vulkan framebuffers of a target are never poisoned");
        let matching = held.iter().find(|frame| {
            frame.key == key
                && frame.depth.as_ref().map(|held| held.view) == depth.map(|held| held.view)
                && frame.render_pass == render_pass
        });
        if let Some(frame) = matching {
            return (frame.render_pass, frame.raw);
        }
        let mut attachments = Vec::with_capacity(2);
        if let Some(target) = color {
            attachments.push(target.view);
        }
        if let Some(texture) = depth {
            attachments.push(texture.view);
        }
        let raw = unsafe {
            self.core.create_framebuffer(
                &vk::FramebufferCreateInfo::default()
                    .render_pass(render_pass)
                    .attachments(&attachments)
                    .width(width)
                    .height(height)
                    .layers(1),
                None,
            )
        }
        .unwrap_or_else(|error| panic!("creating a Vulkan framebuffer: {error:?}"));
        held.push(Framebuffer {
            key,
            render_pass,
            raw,
            depth: depth.cloned(),
        });
        (render_pass, raw)
    }

    pub(crate) fn has_surface(&self) -> bool {
        self.core
            .instance
            .as_ref()
            .is_some_and(|owner| owner.surface.is_some())
    }

    pub(crate) fn create_surface(self: &Arc<Self>, width: u32, height: u32) -> Surface {
        let owner = self
            .core
            .instance
            .as_ref()
            .expect("a Vulkan swapchain belongs to a window device");
        let surface = owner
            .surface
            .expect("a Vulkan window device owns its surface");
        let loader = ash::khr::swapchain::Device::new(&owner.instance, &self.core);
        let (raw, images, present, width, height, format) =
            self.build_swapchain(&loader, surface, width, height);
        let acquire = (0..ACQUIRE_SLOTS)
            .map(|_| {
                unsafe {
                    self.core
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                }
                .unwrap_or_else(|error| panic!("creating a Vulkan acquire semaphore: {error:?}"))
            })
            .collect();
        Surface {
            device: self.clone(),
            surface,
            loader,
            raw,
            images,
            acquire,
            acquired: 0,
            present,
            width,
            height,
            format,
            outdated: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    fn build_swapchain(
        self: &Arc<Self>,
        loader: &ash::khr::swapchain::Device,
        surface: vk::SurfaceKHR,
        width: u32,
        height: u32,
    ) -> (
        vk::SwapchainKHR,
        Vec<Texture>,
        Vec<vk::Semaphore>,
        u32,
        u32,
        TextureFormat,
    ) {
        let owner = self
            .core
            .instance
            .as_ref()
            .expect("a Vulkan swapchain keeps its surface loader");
        let capabilities = unsafe {
            owner
                .surface_loader
                .get_physical_device_surface_capabilities(self.core.physical, surface)
        }
        .unwrap_or_else(|error| panic!("reading Vulkan surface capabilities: {error:?}"));
        let formats = unsafe {
            owner
                .surface_loader
                .get_physical_device_surface_formats(self.core.physical, surface)
        }
        .unwrap_or_else(|error| panic!("reading Vulkan surface formats: {error:?}"));
        let chosen = formats
            .iter()
            .find(|format| format.format == vk::Format::B8G8R8A8_UNORM)
            .or_else(|| {
                formats
                    .iter()
                    .find(|format| format.format == vk::Format::R8G8B8A8_UNORM)
            })
            .unwrap_or_else(|| panic!("a Vulkan surface offers no 8 bit RGBA format"));
        let format = match chosen.format {
            vk::Format::B8G8R8A8_UNORM => TextureFormat::Bgra8Unorm,
            vk::Format::R8G8B8A8_UNORM => TextureFormat::Rgba8Unorm,
            _ => unreachable!("a chosen Vulkan surface format is 8 bit RGBA"),
        };
        let extent = if capabilities.current_extent.width == u32::MAX {
            vk::Extent2D {
                width: width.clamp(
                    capabilities.min_image_extent.width,
                    capabilities.max_image_extent.width,
                ),
                height: height.clamp(
                    capabilities.min_image_extent.height,
                    capabilities.max_image_extent.height,
                ),
            }
        } else {
            capabilities.current_extent
        };
        assert!(
            extent.width > 0 && extent.height > 0,
            "a Vulkan surface spans {} by {} texels",
            extent.width,
            extent.height
        );
        let upper = if capabilities.max_image_count == 0 {
            3
        } else {
            capabilities.max_image_count
        };
        let count = capabilities.min_image_count.max(2).min(upper);
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(count)
            .image_format(chosen.format)
            .image_color_space(chosen.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true);
        let raw = unsafe { loader.create_swapchain(&info, None) }
            .unwrap_or_else(|error| panic!("creating a Vulkan swapchain: {error:?}"));
        let images = unsafe { loader.get_swapchain_images(raw) }
            .unwrap_or_else(|error| panic!("reading Vulkan swapchain images: {error:?}"));
        let textures = images
            .iter()
            .map(|image| self.swapchain_texture(*image, extent.width, extent.height, format))
            .collect::<Vec<_>>();
        let present = images
            .iter()
            .map(|_| {
                unsafe {
                    self.core
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                }
                .unwrap_or_else(|error| panic!("creating a Vulkan present semaphore: {error:?}"))
            })
            .collect();
        (raw, textures, present, extent.width, extent.height, format)
    }

    fn swapchain_texture(
        self: &Arc<Self>,
        image: vk::Image,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> Texture {
        let view = unsafe {
            self.core.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(vk_format(format))
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1),
                    ),
                None,
            )
        }
        .unwrap_or_else(|error| panic!("creating a Vulkan swapchain view: {error:?}"));
        Texture {
            resource: Arc::new(TextureResource {
                block: None,
                core: self.core.clone(),
                image,
                view,
                targets: vec![Target {
                    view,
                    owned_view: false,
                    width,
                    height,
                    framebuffers: Mutex::new(Vec::new()),
                }],
                width,
                height,
                mip_levels: 1,
                format,
                usages: TextureUsages::RENDER_ATTACHMENT,
                aspect: vk::ImageAspectFlags::COLOR,
                present: true,
                state: Mutex::new(TextureState {
                    layout: vk::ImageLayout::UNDEFINED,
                    access: Access::default(),
                }),
            }),
        }
    }

    pub(crate) fn idle(&self) {
        unsafe { self.core.device_wait_idle() }
            .unwrap_or_else(|error| panic!("waiting for a Vulkan device: {error:?}"));
        let mut state = self
            .state
            .lock()
            .expect("the Vulkan queue is never poisoned");
        self.retire(&mut state);
        for frame in &mut state.frames {
            frame.resources.clear();
        }
    }

    fn attachment(&self, image: Image<'_>) -> (vk::Image, Block, vk::ImageView) {
        let Image {
            name,
            width,
            height,
            mip_levels,
            format,
            usage,
            aspect,
        } = image;
        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };
        let image = unsafe {
            self.core.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(extent)
                    .mip_levels(mip_levels)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED),
                None,
            )
        }
        .unwrap_or_else(|error| {
            panic!("allocating a Vulkan attachment of {width} by {height}: {error:?}")
        });
        let requirements = unsafe { self.core.get_image_memory_requirements(image) };
        let block = self
            .core
            .memory()
            .allocate(name, requirements, MemoryLocation::GpuOnly, false);
        unsafe {
            self.core
                .bind_image_memory(image, block.memory(), block.offset())
        }
        .unwrap_or_else(|error| panic!("binding Vulkan attachment memory: {error:?}"));
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(aspect)
            .base_mip_level(0)
            .level_count(mip_levels)
            .base_array_layer(0)
            .layer_count(1);
        let view = unsafe {
            self.core.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(range),
                None,
            )
        }
        .unwrap_or_else(|error| panic!("creating a Vulkan attachment view: {error:?}"));
        (image, block, view)
    }

    pub(crate) fn create_graphics_pipeline(
        self: &Arc<Self>,
        program: &GraphicsProgram,
        format: Option<TextureFormat>,
        state: PipelineState,
        depth: bool,
    ) -> GraphicsPipeline {
        let (groups, pools, layout) = self.pipeline_layout(
            program.bindings(),
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            program.label(),
        );
        GraphicsPipeline {
            resource: Arc::new(GraphicsPipelineResource {
                core: self.core.clone(),
                layout,
                groups,
                pools,
                render_pass: self.render_pass(PassKey {
                    format,
                    depth: if depth {
                        Some((DepthLoad::Clear, false))
                    } else {
                        None
                    },
                }),
                format,
                state,
                compiled: OnceLock::new(),
            }),
        }
    }

    fn render_pass(&self, key: PassKey) -> vk::RenderPass {
        let mut passes = self
            .render_passes
            .lock()
            .expect("the Vulkan render passes are never poisoned");
        if let Some((_, pass)) = passes.iter().find(|(held, _)| *held == key) {
            return *pass;
        }
        let pass = render_pass(&self.core, key).unwrap_or_else(|error| {
            panic!(
                "creating the {:?} Vulkan render pass over {:?}: {error}",
                key.format, key.depth,
            )
        });
        passes.push((key, pass));
        pass
    }

    pub(crate) fn compile_graphics(
        &self,
        pipeline: &GraphicsPipeline,
        program: &GraphicsProgram,
        artifacts: &ArtifactCache,
    ) {
        pipeline.resource.compiled.get_or_init(|| {
            let dome_shader::shader::GraphicsTranslation::Spirv {
                module,
                vertex,
                fragment,
            } = program.translate(Backend::Vulkan)
            else {
                panic!("Vulkan accepts SPIR-V graphics programs");
            };
            let key = format!(
                "vulkan/{}/graphics-{:?}-{}.bin",
                uuids(&self.uuid),
                pipeline.resource.format,
                fingerprint(&[&bytes_of(&module), vertex.as_bytes(), fragment.as_bytes()]),
            );
            let initial = artifacts.load(&key);
            let pipeline_cache = pipeline_cache(&self.core, initial.as_deref());
            let vertex_module = unsafe {
                self.core.create_shader_module(
                    &vk::ShaderModuleCreateInfo::default().code(&module),
                    None,
                )
            }
            .unwrap_or_else(|error| panic!("creating the Vulkan vertex module: {error:?}"));
            let fragment_module = unsafe {
                self.core.create_shader_module(
                    &vk::ShaderModuleCreateInfo::default().code(&module),
                    None,
                )
            }
            .unwrap_or_else(|error| panic!("creating the Vulkan fragment module: {error:?}"));
            let vertex_entry = CString::new(vertex).expect("a vertex entry point has no zero byte");
            let fragment_entry =
                CString::new(fragment).expect("a fragment entry point has no zero byte");
            let stages = [
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vertex_module)
                    .name(&vertex_entry),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fragment_module)
                    .name(&fragment_entry),
            ];
            let (source, destination, source_alpha, destination_alpha) =
                match pipeline.resource.state.blend {
                    BlendState::Opaque => (
                        vk::BlendFactor::ONE,
                        vk::BlendFactor::ZERO,
                        vk::BlendFactor::ONE,
                        vk::BlendFactor::ZERO,
                    ),
                    BlendState::Premultiplied => (
                        vk::BlendFactor::ONE,
                        vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
                        vk::BlendFactor::ONE_MINUS_DST_ALPHA,
                        vk::BlendFactor::ONE,
                    ),
                    BlendState::Preserve => (
                        vk::BlendFactor::ZERO,
                        vk::BlendFactor::ONE,
                        vk::BlendFactor::ZERO,
                        vk::BlendFactor::ONE,
                    ),
                };
            let blend = [vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(if pipeline.resource.state.alpha {
                    vk::ColorComponentFlags::RGBA
                } else {
                    vk::ColorComponentFlags::R
                        | vk::ColorComponentFlags::G
                        | vk::ColorComponentFlags::B
                })
                .blend_enable(!matches!(pipeline.resource.state.blend, BlendState::Opaque))
                .src_color_blend_factor(source)
                .dst_color_blend_factor(destination)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(source_alpha)
                .dst_alpha_blend_factor(destination_alpha)
                .alpha_blend_op(vk::BlendOp::ADD)];
            let blend_attachments = if pipeline.resource.format.is_some() {
                &blend[..]
            } else {
                &[][..]
            };
            let input = vk::PipelineVertexInputStateCreateInfo::default();
            let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
            let viewport = vk::PipelineViewportStateCreateInfo::default()
                .viewport_count(1)
                .scissor_count(1);
            let raster = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::NONE)
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                .line_width(1.0);
            let multisample = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);
            let depth = vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(!matches!(pipeline.resource.state.depth, DepthState::Off))
                .depth_write_enable(matches!(
                    pipeline.resource.state.depth,
                    DepthState::TestWrite
                ))
                .depth_compare_op(match pipeline.resource.state.depth {
                    DepthState::TestCovered => vk::CompareOp::LESS_OR_EQUAL,
                    _ => vk::CompareOp::LESS,
                });
            let color =
                vk::PipelineColorBlendStateCreateInfo::default().attachments(blend_attachments);
            let dynamic = vk::PipelineDynamicStateCreateInfo::default()
                .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);
            let info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&input)
                .input_assembly_state(&assembly)
                .viewport_state(&viewport)
                .rasterization_state(&raster)
                .multisample_state(&multisample)
                .depth_stencil_state(&depth)
                .color_blend_state(&color)
                .dynamic_state(&dynamic)
                .layout(pipeline.resource.layout)
                .render_pass(pipeline.resource.render_pass)
                .subpass(0);
            let compiled = unsafe {
                self.core
                    .create_graphics_pipelines(pipeline_cache, &[info], None)
            }
            .unwrap_or_else(|(_, error)| {
                panic!(
                    "compiling Vulkan graphics pipeline {}: {error:?}",
                    program.label()
                )
            })[0];
            unsafe {
                self.core.destroy_shader_module(vertex_module, None);
                self.core.destroy_shader_module(fragment_module, None);
            }
            let data = unsafe { self.core.get_pipeline_cache_data(pipeline_cache) }.unwrap_or_else(
                |error| {
                    panic!(
                        "reading the Vulkan pipeline cache of {}: {error:?}",
                        program.label()
                    )
                },
            );
            artifacts.store(&key, &data);
            unsafe { self.core.destroy_pipeline_cache(pipeline_cache, None) };
            compiled
        });
    }

    pub(crate) fn submit(&self, writes: &[Write], commands: &[Command]) -> u64 {
        let mut state = self
            .state
            .lock()
            .expect("the Vulkan queue is never poisoned");
        self.retire(&mut state);
        if writes.is_empty() && commands.is_empty() {
            return state.next;
        }
        let slot = self.acquire(&mut state);
        state.next = state
            .next
            .checked_add(1)
            .expect("compute submission indices fit in u64");
        let index = state.next;
        self.releases.submitted(index);
        let QueueState { frames, .. } = &mut *state;
        let frame = &mut frames[slot];
        frame.begin(self);
        frame.index = index;
        let command = frame.command;
        unsafe {
            self.core
                .cmd_reset_query_pool(command, self.stamps, (slot * 2) as u32, 2);
            self.core.cmd_write_timestamp(
                command,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.stamps,
                (slot * 2) as u32,
            );
        }
        let mut pending = Pending::default();
        for write in writes {
            let held = native_buffer(&write.buffer);
            let handle = held.handle;
            pending.touch(
                &held.access,
                Access::write(
                    vk::PipelineStageFlags::TRANSFER,
                    vk::AccessFlags::TRANSFER_WRITE,
                ),
            );
            pending.flush(&self.core, command);
            let (staging, offset) = frame.stage(self, &write.bytes, TRANSFER_ALIGNMENT);
            let region = [vk::BufferCopy::default()
                .src_offset(offset)
                .dst_offset(write.offset)
                .size(write.bytes.len() as u64)];
            unsafe { self.core.cmd_copy_buffer(command, staging, handle, &region) };
            frame.resources.push(held.clone());
        }
        let mut presenting = None;
        for operation in commands {
            match operation {
                Command::Copy {
                    source,
                    source_offset,
                    target,
                    target_offset,
                    bytes,
                } => {
                    pending.touch(
                        &buffer(source).access,
                        Access::read(
                            vk::PipelineStageFlags::TRANSFER,
                            vk::AccessFlags::TRANSFER_READ,
                        ),
                    );
                    pending.touch(
                        &buffer(target).access,
                        Access::write(
                            vk::PipelineStageFlags::TRANSFER,
                            vk::AccessFlags::TRANSFER_WRITE,
                        ),
                    );
                    pending.flush(&self.core, command);
                    let region = [vk::BufferCopy::default()
                        .src_offset(*source_offset)
                        .dst_offset(*target_offset)
                        .size(*bytes)];
                    unsafe {
                        self.core.cmd_copy_buffer(
                            command,
                            buffer(source).handle,
                            buffer(target).handle,
                            &region,
                        )
                    };
                    frame.resources.push(buffer(source).clone());
                    frame.resources.push(buffer(target).clone());
                }
                Command::Clear {
                    buffer: target,
                    offset,
                    bytes,
                } => {
                    pending.touch(
                        &buffer(target).access,
                        Access::write(
                            vk::PipelineStageFlags::TRANSFER,
                            vk::AccessFlags::TRANSFER_WRITE,
                        ),
                    );
                    pending.flush(&self.core, command);
                    unsafe {
                        self.core.cmd_fill_buffer(
                            command,
                            buffer(target).handle,
                            *offset,
                            *bytes,
                            0,
                        )
                    };
                    frame.resources.push(buffer(target).clone());
                }
                Command::Render {
                    target,
                    level,
                    clear,
                    depth,
                    draws,
                } => {
                    let color = target.as_ref().map(|target| {
                        let texture = native_texture(target.native());
                        texture
                            .targets
                            .get(*level as usize)
                            .expect("a render level names a level its target holds")
                    });
                    let depth_texture = depth
                        .as_ref()
                        .map(|(held, _)| native_texture(held.native()));
                    let (width, height) = match (color, &depth_texture) {
                        (Some(frame), _) => (frame.width, frame.height),
                        (None, Some(held)) => (held.width, held.height),
                        (None, None) => unreachable!("a render pass names an attachment"),
                    };
                    let extent = vk::Extent2D { width, height };
                    let key = PassKey {
                        format: color.map(|_| {
                            native_texture(
                                target
                                    .as_ref()
                                    .expect("a colour attachment names its target")
                                    .native(),
                            )
                            .format
                        }),
                        depth: depth.as_ref().map(|(_, ops)| (ops.load, ops.store)),
                    };
                    if let (Some((_, ops)), Some(held)) = (&depth, &depth_texture)
                        && ops.load == DepthLoad::Keep
                    {
                        let state = held
                            .state
                            .lock()
                            .expect("a Vulkan texture layout is never poisoned");
                        assert!(
                            state.layout == vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                            "a render pass keeps a depth texture no storing pass left",
                        );
                    }
                    let (render_pass, framebuffer) = self.framing(color, key, depth_texture);
                    let mut clears = Vec::with_capacity(2);
                    if color.is_some() {
                        clears.push(vk::ClearValue {
                            color: vk::ClearColorValue { float32: *clear },
                        });
                    }
                    if depth.is_some() {
                        clears.push(vk::ClearValue {
                            depth_stencil: vk::ClearDepthStencilValue {
                                depth: 1.0,
                                stencil: 0,
                            },
                        });
                    }
                    let info = vk::RenderPassBeginInfo::default()
                        .render_pass(render_pass)
                        .framebuffer(framebuffer)
                        .render_area(vk::Rect2D {
                            offset: vk::Offset2D { x: 0, y: 0 },
                            extent,
                        })
                        .clear_values(&clears);
                    for draw in draws {
                        for bindings in &draw.groups {
                            let group = group_resource(&bindings.native);
                            for held in &group.textures {
                                pending.transition(
                                    held,
                                    held.sampled_layout(),
                                    Access::read(
                                        vk::PipelineStageFlags::FRAGMENT_SHADER,
                                        vk::AccessFlags::SHADER_READ,
                                    ),
                                );
                            }
                            for held in &group.buffers {
                                pending.touch(
                                    &held.resource.access,
                                    held.access(
                                        vk::PipelineStageFlags::VERTEX_SHADER
                                            | vk::PipelineStageFlags::FRAGMENT_SHADER,
                                    ),
                                );
                            }
                        }
                        if let RecordedCount::Indirect { arguments, .. } = &draw.count {
                            pending.touch(
                                &native_buffer(arguments.native()).access,
                                Access::read(
                                    vk::PipelineStageFlags::DRAW_INDIRECT,
                                    vk::AccessFlags::INDIRECT_COMMAND_READ,
                                ),
                            );
                        }
                    }
                    if let Some(target) = &target {
                        native_texture(target.native()).record(
                            &mut pending,
                            Access::write(
                                vk::PipelineStageFlags::ALL_COMMANDS,
                                vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                                    | vk::AccessFlags::MEMORY_WRITE,
                            ),
                        );
                    }
                    if let Some(texture) = &depth_texture {
                        texture.record(
                            &mut pending,
                            Access::write(
                                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                                    | vk::AccessFlags::MEMORY_WRITE
                                    | depth_load_access(depth.as_ref().map(|(_, ops)| ops.load)),
                            ),
                        );
                    }
                    pending.flush(&self.core, command);
                    if let Some(texture) = &depth_texture {
                        frame
                            .resources
                            .push(Arc::clone(texture) as Arc<dyn Any + Send + Sync>);
                    }
                    unsafe {
                        self.core.cmd_begin_render_pass(
                            command,
                            &info,
                            vk::SubpassContents::INLINE,
                        );
                    }
                    for draw in draws {
                        let pipeline = graphics_pipeline(&draw.pipeline.native);
                        let groups = draw
                            .groups
                            .iter()
                            .map(|group| group_resource(&group.native).clone())
                            .collect::<Vec<_>>();
                        let sets = groups.iter().map(|group| group.set).collect::<Vec<_>>();
                        let compiled = *pipeline
                            .compiled
                            .get()
                            .expect("a graphics pipeline is compiled before a draw");
                        unsafe {
                            self.core.cmd_set_viewport(
                                command,
                                0,
                                &[vk::Viewport {
                                    x: draw.viewport.x,
                                    y: draw.viewport.y,
                                    width: draw.viewport.width,
                                    height: draw.viewport.height,
                                    min_depth: 0.0,
                                    max_depth: 1.0,
                                }],
                            );
                            self.core.cmd_set_scissor(
                                command,
                                0,
                                &[vk::Rect2D {
                                    offset: vk::Offset2D {
                                        x: draw.scissor.x as i32,
                                        y: draw.scissor.y as i32,
                                    },
                                    extent: vk::Extent2D {
                                        width: draw.scissor.width,
                                        height: draw.scissor.height,
                                    },
                                }],
                            );
                            self.core.cmd_bind_pipeline(
                                command,
                                vk::PipelineBindPoint::GRAPHICS,
                                compiled,
                            );
                            self.core.cmd_bind_descriptor_sets(
                                command,
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline.layout,
                                0,
                                &sets,
                                &[],
                            );
                            match &draw.count {
                                RecordedCount::Direct {
                                    vertices,
                                    instances,
                                } => self.core.cmd_draw(command, *vertices, *instances, 0, 0),
                                RecordedCount::Indirect { arguments, offset } => {
                                    self.core.cmd_draw_indirect(
                                        command,
                                        native_buffer(arguments.native()).handle,
                                        *offset,
                                        1,
                                        16,
                                    );
                                    frame
                                        .resources
                                        .push(native_buffer(arguments.native()).clone());
                                }
                            }
                        }
                        frame.resources.push(pipeline.clone());
                        for group in groups {
                            frame.resources.push(group);
                        }
                    }
                    unsafe { self.core.cmd_end_render_pass(command) };
                    if let Some(target) = &target {
                        let texture = native_texture(target.native());
                        texture
                            .state
                            .lock()
                            .expect("a Vulkan texture layout is never poisoned")
                            .layout = vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
                        frame.resources.push(texture.clone());
                    }
                    if let Some(texture) = &depth_texture {
                        texture
                            .state
                            .lock()
                            .expect("a Vulkan texture layout is never poisoned")
                            .layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
                    }
                }
                Command::WriteTexture {
                    texture,
                    level,
                    bytes,
                } => {
                    let texture = native_texture(texture.native());
                    let texel = u64::from(texture.format.bytes_per_texel());
                    let (staging, offset) = frame.stage(self, bytes, texel.max(TRANSFER_ALIGNMENT));
                    assert!(
                        offset.is_multiple_of(texel),
                        "a texture copy reads the staging buffer at {offset}, which is no multiple of its {texel} byte texel block",
                    );
                    pending.transition(
                        texture,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        Access::write(
                            vk::PipelineStageFlags::TRANSFER,
                            vk::AccessFlags::TRANSFER_WRITE,
                        ),
                    );
                    pending.flush(&self.core, command);
                    let width = (texture.width >> level).max(1);
                    let height = (texture.height >> level).max(1);
                    let row = u64::from(width) * texel;
                    let stride = row.div_ceil(256) * 256;
                    let regions = [vk::BufferImageCopy::default()
                        .buffer_offset(offset)
                        .buffer_row_length((stride / texel) as u32)
                        .buffer_image_height(height)
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .mip_level(*level)
                                .base_array_layer(0)
                                .layer_count(1),
                        )
                        .image_extent(vk::Extent3D {
                            width,
                            height,
                            depth: 1,
                        })];
                    unsafe {
                        self.core.cmd_copy_buffer_to_image(
                            command,
                            staging,
                            texture.image,
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            &regions,
                        )
                    };
                    if texture.usages.contains(TextureUsages::SAMPLED) {
                        pending.transition(
                            texture,
                            texture.sampled_layout(),
                            Access::read(
                                vk::PipelineStageFlags::FRAGMENT_SHADER,
                                vk::AccessFlags::SHADER_READ,
                            ),
                        );
                        pending.flush(&self.core, command);
                    }
                    frame.resources.push(texture.clone());
                }
                Command::CopyTexture {
                    texture,
                    target,
                    target_offset,
                } => {
                    let texture = native_texture(texture.native());
                    let target = buffer(target);
                    pending.transition(
                        texture,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        Access::read(
                            vk::PipelineStageFlags::TRANSFER,
                            vk::AccessFlags::TRANSFER_READ,
                        ),
                    );
                    pending.touch(
                        &target.access,
                        Access::write(
                            vk::PipelineStageFlags::TRANSFER,
                            vk::AccessFlags::TRANSFER_WRITE,
                        ),
                    );
                    pending.flush(&self.core, command);
                    let regions = [vk::BufferImageCopy::default()
                        .buffer_offset(*target_offset)
                        .buffer_row_length({
                            let texel = u64::from(texture.format.bytes_per_texel());
                            (u64::from(texture.width) * texel).div_ceil(256) * 256 / texel
                        } as u32)
                        .buffer_image_height(texture.height)
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .mip_level(0)
                                .base_array_layer(0)
                                .layer_count(1),
                        )
                        .image_extent(vk::Extent3D {
                            width: texture.width,
                            height: texture.height,
                            depth: 1,
                        })];
                    unsafe {
                        self.core.cmd_copy_image_to_buffer(
                            command,
                            texture.image,
                            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                            target.handle,
                            &regions,
                        )
                    };
                    frame.resources.push(texture.clone());
                    frame.resources.push(target.clone());
                }
                Command::Present {
                    texture, native, ..
                } => {
                    let present = native.vulkan();
                    let texture = native_texture(texture.native());
                    assert!(
                        texture.present,
                        "a Vulkan submission presents a swapchain image"
                    );
                    pending.transition(
                        texture,
                        vk::ImageLayout::PRESENT_SRC_KHR,
                        Access::read(
                            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                            vk::AccessFlags::empty(),
                        ),
                    );
                    pending.flush(&self.core, command);
                    frame.resources.push(texture.clone());
                    presenting = Some(present);
                }
                Command::Dispatch {
                    pipeline: handle,
                    group: bindings,
                    groups,
                } => {
                    let compiled = pipeline(&handle.slot.native);
                    let group = group_resource(&bindings.native).clone();
                    for texture in &group.textures {
                        pending.transition(
                            texture,
                            texture.sampled_layout(),
                            Access::read(
                                vk::PipelineStageFlags::COMPUTE_SHADER,
                                vk::AccessFlags::SHADER_READ,
                            ),
                        );
                    }
                    for held in &group.buffers {
                        pending.touch(
                            &held.resource.access,
                            held.access(vk::PipelineStageFlags::COMPUTE_SHADER),
                        );
                    }
                    pending.flush(&self.core, command);
                    let pipeline_handle = *compiled
                        .compiled
                        .get()
                        .expect("a compute pipeline is compiled before dispatch");
                    unsafe {
                        self.core.cmd_bind_pipeline(
                            command,
                            vk::PipelineBindPoint::COMPUTE,
                            pipeline_handle,
                        );
                        self.core.cmd_bind_descriptor_sets(
                            command,
                            vk::PipelineBindPoint::COMPUTE,
                            compiled.layout,
                            0,
                            &[group.set],
                            &[],
                        );
                        self.core
                            .cmd_dispatch(command, groups[0], groups[1], groups[2]);
                    }
                    frame.resources.push(compiled.clone());
                    frame.resources.push(group.clone());
                }
            }
        }
        unsafe {
            self.core.cmd_write_timestamp(
                command,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                self.stamps,
                (slot * 2 + 1) as u32,
            );
            self.core.end_command_buffer(command)
        }
        .unwrap_or_else(|error| panic!("closing a Vulkan compute submission: {error:?}"));
        let buffers = [command];
        let signal = presenting.map(|present| [present.semaphore]);
        let wait = presenting.map(|present| [present.acquire]);
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let mut submit = vk::SubmitInfo::default().command_buffers(&buffers);
        if let Some(signal) = &signal {
            submit = submit.signal_semaphores(signal);
        }
        if let Some(wait) = &wait {
            submit = submit
                .wait_semaphores(wait)
                .wait_dst_stage_mask(&wait_stages);
        }
        let submits = [submit];
        let turn = self.gate.take();
        unsafe { self.core.queue_submit(self.queue, &submits, frame.fence) }
            .unwrap_or_else(|error| panic!("submitting Vulkan compute work: {error:?}"));
        drop(state);
        if let Some(present) = presenting {
            present.present(self.queue);
        }
        drop(turn);
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

    fn retire(&self, state: &mut QueueState) {
        let mut completed = state.completed;
        while let Some(slot) = state
            .frames
            .iter()
            .position(|frame| frame.index == completed + 1)
        {
            let fence = state.frames[slot].fence;
            let signaled = unsafe { self.core.get_fence_status(fence) }
                .unwrap_or_else(|error| panic!("checking Vulkan compute completion: {error:?}"));
            if !signaled {
                break;
            }
            completed += 1;
        }
        state.completed = completed;
        self.releases.advance(completed);
        self.record(state);
    }

    fn record(&self, state: &mut QueueState) {
        for slot in 0..state.frames.len() {
            let index = state.frames[slot].index;
            if index == 0 || index > state.completed {
                continue;
            }
            let key = index as usize % TIME_SLOTS;
            if state.times[key].is_some_and(|(kept, _)| kept >= index) {
                continue;
            }
            let mut stamps = [0u64; 2];
            unsafe {
                self.core.get_query_pool_results(
                    self.stamps,
                    (slot * 2) as u32,
                    &mut stamps,
                    vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                )
            }
            .unwrap_or_else(|error| {
                panic!("reading the Vulkan timestamp of submission {index}: {error:?}")
            });
            let mask = if self.bits >= 64 {
                u64::MAX
            } else {
                (1u64 << self.bits) - 1
            };
            let elapsed = stamps[1].wrapping_sub(stamps[0]) & mask;
            state.times[key] = Some((index, elapsed as f64 * self.ticks / 1e9));
        }
    }

    pub(crate) fn complete(&self, index: u64) -> bool {
        let mut state = self
            .state
            .lock()
            .expect("the Vulkan queue is never poisoned");
        self.retire(&mut state);
        index <= state.completed
    }

    pub(crate) fn completed(&self) -> u64 {
        self.state
            .lock()
            .expect("the Vulkan queue is never poisoned")
            .completed
    }

    pub(crate) fn seconds(&self, index: u64) -> f64 {
        let mut state = self
            .state
            .lock()
            .expect("the Vulkan queue is never poisoned");
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
            .expect("the Vulkan queue is never poisoned");
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
            return Err("waiting on an unsubmitted Vulkan command".to_owned());
        }
        self.retire(state);
        if index <= state.completed {
            return Ok(());
        }
        let fence = state
            .frames
            .iter()
            .find(|frame| frame.index == index)
            .expect("an unfinished submission owns a fence")
            .fence;
        match self.gate.advance() {
            None => unsafe {
                self.core.wait_for_fences(
                    &[fence],
                    true,
                    timeout.as_nanos().min(u64::MAX as u128) as u64,
                )
            }
            .map_err(|error| format!("waiting for Vulkan compute work: {error:?}"))?,
            Some(advance) => {
                let started = Instant::now();
                loop {
                    let slice = HOST_SLICE.min(timeout.saturating_sub(started.elapsed()));
                    match unsafe {
                        self.core.wait_for_fences(
                            &[fence],
                            true,
                            slice.as_nanos().min(u64::MAX as u128) as u64,
                        )
                    } {
                        Ok(()) => break,
                        Err(vk::Result::TIMEOUT) if started.elapsed() < timeout => {
                            advance.advance()
                        }
                        Err(vk::Result::TIMEOUT) => {
                            return Err("waiting for Vulkan compute work timed out".to_owned());
                        }
                        Err(error) => {
                            return Err(format!("waiting for Vulkan compute work: {error:?}"));
                        }
                    }
                }
            }
        }
        self.retire(state);
        Ok(())
    }
    fn release(&self) {
        let state = self
            .state
            .lock()
            .expect("the Vulkan queue is never poisoned");
        let fences = state
            .frames
            .iter()
            .filter(|frame| frame.index > state.completed)
            .map(|frame| frame.fence)
            .collect::<Vec<_>>();
        if fences.is_empty() {
            return;
        }
        let result = unsafe {
            self.core.wait_for_fences(
                &fences,
                true,
                RELEASE_TIMEOUT.as_nanos().min(u64::MAX as u128) as u64,
            )
        };
        match result {
            Ok(()) | Err(vk::Result::ERROR_DEVICE_LOST) => {}
            Err(error) => {
                eprintln!("releasing unfinished Vulkan compute work: {error:?}");
                std::process::abort();
            }
        }
    }

    fn discard(&mut self) {
        let mut state = self
            .state
            .lock()
            .expect("the Vulkan queue is never poisoned");
        let frames = std::mem::take(&mut state.frames);
        for frame in frames {
            unsafe { self.core.destroy_fence(frame.fence, None) };
        }
    }

    pub(crate) fn write_host(&self, buffer: &Buffer, offset: u64, bytes: &[u8]) {
        let mapped = buffer
            .resource
            .block
            .mapped()
            .unwrap_or_else(|| panic!("a Vulkan mirror holds no host memory"));
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), mapped.add(offset as usize), bytes.len());
        }
    }

    pub(crate) fn read_host(&self, buffer: &Buffer, offset: u64, bytes: u64) -> Vec<u8> {
        let mapped = buffer
            .resource
            .block
            .mapped()
            .unwrap_or_else(|| panic!("a Vulkan mirror holds no host memory"));
        unsafe { std::slice::from_raw_parts(mapped.add(offset as usize), bytes as usize).to_vec() }
    }

    pub(crate) fn read(&self, buffer: &Buffer, bytes: u64) -> Vec<u8> {
        let mapped = buffer
            .resource
            .block
            .mapped()
            .unwrap_or_else(|| panic!("a Vulkan readback holds no host memory"));
        unsafe { std::slice::from_raw_parts(mapped, bytes as usize) }.to_vec()
    }

    pub(crate) fn assert_alive(&self) {
        let mut state = self
            .state
            .lock()
            .expect("the Vulkan queue is never poisoned");
        self.retire(&mut state);
    }
}

fn allocation(entry: &Entry, instance: &Instance, physical: vk::PhysicalDevice) -> u64 {
    let properties2: Option<vk::PFN_vkGetPhysicalDeviceProperties2> = instance_fn(
        entry,
        instance.handle(),
        c"vkGetPhysicalDeviceProperties2KHR",
    )
    .or_else(|| instance_fn(entry, instance.handle(), c"vkGetPhysicalDeviceProperties2"));
    let Some(properties2) = properties2 else {
        return u64::MAX;
    };
    let mut maintenance = vk::PhysicalDeviceMaintenance3Properties::default();
    let mut properties = vk::PhysicalDeviceProperties2::default();
    properties = properties.push_next(&mut maintenance);
    unsafe { properties2(physical, &mut properties) };
    match maintenance.max_memory_allocation_size {
        0 => u64::MAX,
        bytes => bytes,
    }
}

fn vk_format(format: TextureFormat) -> vk::Format {
    match format {
        TextureFormat::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
        TextureFormat::Rgba8UnormSrgb => vk::Format::R8G8B8A8_SRGB,
        TextureFormat::Bgra8Unorm => vk::Format::B8G8R8A8_UNORM,
        TextureFormat::Rgba16Float => vk::Format::R16G16B16A16_SFLOAT,
        TextureFormat::R32Float => vk::Format::R32_SFLOAT,
        TextureFormat::Depth32Float => DEPTH_FORMAT,
    }
}

fn limits(
    properties: &vk::PhysicalDeviceProperties,
    memory: &vk::PhysicalDeviceMemoryProperties,
    largest_allocation: u64,
) -> Limits {
    let gpu = &properties.limits;
    let local_bytes = memory.memory_heaps[..memory.memory_heap_count as usize]
        .iter()
        .filter(|heap| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|heap| heap.size)
        .max()
        .unwrap_or(0);
    Limits {
        max_storage_buffers_per_shader_stage: gpu.max_per_stage_descriptor_storage_buffers.min(30),
        max_sampled_images_per_shader_stage: gpu.max_per_stage_descriptor_sampled_images,
        max_samplers_per_shader_stage: gpu.max_per_stage_descriptor_samplers,
        max_storage_buffer_binding_size: u64::from(gpu.max_storage_buffer_range)
            .min(MAX_BINDING_BYTES),
        max_buffer_size: local_bytes.min(largest_allocation).min(MAX_BINDING_BYTES),
        max_compute_invocations_per_workgroup: gpu.max_compute_work_group_invocations,
        max_compute_workgroup_size_x: gpu.max_compute_work_group_size[0],
        max_compute_workgroup_storage_size: gpu.max_compute_shared_memory_size,
        max_compute_workgroups_per_dimension: gpu
            .max_compute_work_group_count
            .iter()
            .copied()
            .min()
            .unwrap_or(0),
        min_storage_buffer_offset_alignment: gpu.min_storage_buffer_offset_alignment.max(4),
        max_sampler_anisotropy: gpu.max_sampler_anisotropy as u32,
    }
}

impl Drop for GraphicsPipelineResource {
    fn drop(&mut self) {
        unsafe {
            if let Some(compiled) = self.compiled.get() {
                self.core.destroy_pipeline(*compiled, None);
            }
            self.core.destroy_pipeline_layout(self.layout, None);
            for group in &self.groups {
                self.core.destroy_descriptor_set_layout(*group, None);
            }
        }
    }
}

impl Drop for TextureResource {
    fn drop(&mut self) {
        unsafe {
            for target in &self.targets {
                for frame in target
                    .framebuffers
                    .lock()
                    .expect("the Vulkan framebuffers of a target are never poisoned")
                    .drain(..)
                {
                    self.core.destroy_framebuffer(frame.raw, None);
                }
                if target.owned_view {
                    self.core.destroy_image_view(target.view, None);
                }
            }
            self.core.destroy_image_view(self.view, None);
            if self.block.is_some() {
                self.core.destroy_image(self.image, None);
            }
        }
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
        self.format
    }

    pub(crate) fn outdated(&self) -> bool {
        self.outdated.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn acquire(&mut self) -> Option<NativeFrame> {
        if self.outdated() {
            return None;
        }
        let acquire = self.acquire[self.acquired as usize % ACQUIRE_SLOTS];
        let (image, _) = match unsafe {
            self.loader.acquire_next_image(
                self.raw,
                ACQUIRE_TIMEOUT_NANOS,
                acquire,
                vk::Fence::null(),
            )
        } {
            Ok(image) => image,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.outdated
                    .store(true, std::sync::atomic::Ordering::Release);
                return None;
            }
            Err(vk::Result::TIMEOUT | vk::Result::NOT_READY) => return None,
            Err(error) => panic!("acquiring a Vulkan swapchain image: {error:?}"),
        };
        self.acquired = self.acquired.wrapping_add(1);
        let texture = self.images[image as usize].clone();
        Some(NativeFrame {
            texture: NativeTexture::Vulkan(texture),
            width: self.width,
            height: self.height,
            format: self.format,
            present: NativePresent::Vulkan(Present {
                loader: self.loader.clone(),
                raw: self.raw,
                image,
                acquire,
                semaphore: self.present[image as usize],
                outdated: self.outdated.clone(),
            }),
        })
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        self.device.idle();
        self.images.clear();
        for semaphore in self.present.drain(..) {
            unsafe { self.device.core.destroy_semaphore(semaphore, None) };
        }
        unsafe { self.loader.destroy_swapchain(self.raw, None) };
        let (raw, images, present, width, height, format) =
            self.device
                .build_swapchain(&self.loader, self.surface, width, height);
        self.raw = raw;
        self.images = images;
        self.present = present;
        self.width = width;
        self.height = height;
        self.format = format;
        self.outdated
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

impl Present {
    pub(crate) fn present(&self, queue: vk::Queue) {
        let info = vk::PresentInfoKHR::default()
            .wait_semaphores(std::slice::from_ref(&self.semaphore))
            .swapchains(std::slice::from_ref(&self.raw))
            .image_indices(std::slice::from_ref(&self.image));
        match unsafe { self.loader.queue_present(queue, &info) } {
            Ok(_) => {}
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self
                .outdated
                .store(true, std::sync::atomic::Ordering::Release),
            Err(error) => panic!("presenting a Vulkan swapchain: {error:?}"),
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.device.idle();
        self.images.clear();
        for semaphore in self.present.drain(..) {
            unsafe { self.device.core.destroy_semaphore(semaphore, None) };
        }
        for semaphore in self.acquire.drain(..) {
            unsafe { self.device.core.destroy_semaphore(semaphore, None) };
        }
        unsafe { self.loader.destroy_swapchain(self.raw, None) };
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe { self.core.destroy_sampler(self.raw, None) };
    }
}

impl Drop for PipelineResource {
    fn drop(&mut self) {
        unsafe {
            if let Some(compiled) = self.compiled.get() {
                self.core.destroy_pipeline(*compiled, None);
            }
            self.core.destroy_pipeline_layout(self.layout, None);
            for group in &self.groups {
                self.core.destroy_descriptor_set_layout(*group, None);
            }
        }
    }
}

impl Drop for GroupResource {
    fn drop(&mut self) {
        self.pool.release(self.set);
    }
}

impl Drop for BufferResource {
    fn drop(&mut self) {
        unsafe { self.core.destroy_buffer(self.handle, None) };
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        self.release();
        self.releases.reclaim_all();
        self.discard();
        unsafe {
            for (_, pass) in self
                .render_passes
                .lock()
                .expect("the Vulkan render passes are never poisoned")
                .drain(..)
            {
                self.core.destroy_render_pass(pass, None);
            }
            self.core.destroy_query_pool(self.stamps, None);
            self.core.destroy_command_pool(self.pool, None);
        }
    }
}

fn pipeline_cache(raw: &ash::Device, initial: Option<&[u8]>) -> vk::PipelineCache {
    if let Some(data) = initial {
        let info = vk::PipelineCacheCreateInfo::default().initial_data(data);
        if let Ok(cache) = unsafe { raw.create_pipeline_cache(&info, None) } {
            return cache;
        }
    }
    unsafe { raw.create_pipeline_cache(&vk::PipelineCacheCreateInfo::default(), None) }
        .unwrap_or_else(|error| panic!("creating a Vulkan pipeline cache: {error:?}"))
}

fn uuids(uuid: &[u8; vk::UUID_SIZE]) -> String {
    uuid.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn bytes_of(spirv: &[u32]) -> Vec<u8> {
    spirv.iter().flat_map(|word| word.to_le_bytes()).collect()
}

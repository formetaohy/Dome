use crate::buffer::GpuBuffer;
use crate::cache::Cache;
use crate::failure::{Failure, GpuFailure};
use crate::surface::Surface;
use std::fmt::{self, Display, Formatter};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::Duration;

pub use wgpu::PowerPreference;
pub use wgpu::SubmissionIndex;

pub const REQUIRED_FEATURES: wgpu::Features = wgpu::Features::TEXTURE_BINDING_ARRAY
    .union(wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING)
    .union(wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY);

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct GpuRequest {
    pub power: PowerPreference,
}

impl Default for GpuRequest {
    fn default() -> Self {
        Self {
            power: PowerPreference::HighPerformance,
        }
    }
}

#[derive(Clone, Debug)]
pub enum GpuUnavailable {
    NoAdapter(String),
    NoDevice(String),
}

impl Display for GpuUnavailable {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter(reason) => write!(out, "no graphics adapter: {reason}"),
            Self::NoDevice(reason) => write!(out, "the graphics adapter opens no device: {reason}"),
        }
    }
}

impl std::error::Error for GpuUnavailable {}

struct State {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    limits: wgpu::Limits,
    tables: crate::table::TableCapacity,
    features: wgpu::Features,
    cache: Cache,
    failure: Failure,
}

#[derive(Clone)]
pub struct Device {
    state: Arc<State>,
}

#[derive(Clone)]
pub struct Queue {
    device: Device,
}

pub(crate) type Recreate = Box<dyn Fn() -> wgpu::Surface<'static>>;

pub struct Gpu {
    device: Device,
    surface: Mutex<Option<wgpu::Surface<'static>>>,
    recreate: Mutex<Option<Recreate>>,
}

static INSTANCE: LazyLock<wgpu::Instance> = LazyLock::new(|| {
    wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN | wgpu::Backends::METAL,
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: Default::default(),
        backend_options: Default::default(),
        display: None,
    })
});

type Registration = (GpuRequest, Weak<State>);

static REGISTRY: LazyLock<Mutex<Vec<Registration>>> = LazyLock::new(|| Mutex::new(Vec::new()));

pub(crate) fn instance() -> &'static wgpu::Instance {
    &INSTANCE
}

impl Gpu {
    pub fn open(request: &GpuRequest) -> Result<Self, GpuUnavailable> {
        let mut registry = REGISTRY
            .lock()
            .expect("the shared device registry is never poisoned");
        registry.retain(|(_, state)| state.strong_count() > 0);
        if let Some(state) = registry
            .iter()
            .filter(|(origin, _)| origin == request)
            .find_map(|(_, state)| state.upgrade())
        {
            return Ok(Self {
                device: Device { state },
                surface: Mutex::new(None),
                recreate: Mutex::new(None),
            });
        }
        let device = Device::open(request, None)?;
        registry.push((request.clone(), Arc::downgrade(&device.state)));
        Ok(Self {
            device,
            surface: Mutex::new(None),
            recreate: Mutex::new(None),
        })
    }

    pub fn open_window<W>(request: &GpuRequest, target: W) -> Result<Self, GpuUnavailable>
    where
        W: Into<wgpu::SurfaceTarget<'static>> + Clone + 'static,
    {
        let instance = instance();
        let held = target.clone();
        let surface = crate::surface::create(instance, target);
        let device = Device::open(request, Some(&surface))?;
        Ok(Self {
            device,
            surface: Mutex::new(Some(surface)),
            recreate: Mutex::new(Some(Box::new(move || {
                crate::surface::create(instance, held.clone())
            }))),
        })
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn queue(&self) -> Queue {
        Queue {
            device: self.device.clone(),
        }
    }

    pub fn limits(&self) -> &wgpu::Limits {
        self.device.limits()
    }

    pub fn features(&self) -> wgpu::Features {
        self.device.features()
    }

    pub fn failure(&self) -> Option<GpuFailure> {
        self.device.failure()
    }

    pub fn surface(&self, width: u32, height: u32) -> Surface {
        let native = self
            .surface
            .lock()
            .expect("a present surface is never poisoned")
            .take()
            .expect("a device opens one swapchain for its window");
        let recreate = self
            .recreate
            .lock()
            .expect("a present surface recreation is never poisoned")
            .take()
            .expect("a swapchain device recreates its surface");
        Surface::new(self.device.clone(), native, recreate, width, height)
    }
}

impl Device {
    fn open(
        request: &GpuRequest,
        surface: Option<&wgpu::Surface<'static>>,
    ) -> Result<Self, GpuUnavailable> {
        let instance = instance();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: request.power,
            force_fallback_adapter: false,
            compatible_surface: surface,
            apply_limit_buckets: false,
        }))
        .map_err(|error| GpuUnavailable::NoAdapter(error.to_string()))?;
        let offered = adapter.features();
        if !offered.contains(REQUIRED_FEATURES) {
            return Err(GpuUnavailable::NoAdapter(format!(
                "the adapter offers no resource tables: {offered:?}",
            )));
        }
        let limits = adapter.limits();
        let tables = crate::table::TableCapacity::of(&limits);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("dome"),
            required_features: REQUIRED_FEATURES,
            required_limits: limits.clone(),
            ..Default::default()
        }))
        .map_err(|error| GpuUnavailable::NoDevice(error.to_string()))?;
        let failure = Failure::new();
        let lost = failure.clone();
        device.set_device_lost_callback(move |reason, message| {
            lost.raise(format!("the device is lost: {reason:?}: {message}"));
        });
        let raised = failure.clone();
        device.on_uncaptured_error(Arc::new(move |error| {
            raised.raise(format!("wgpu: {error}"));
        }));
        Ok(Self {
            state: Arc::new(State {
                adapter,
                device,
                queue,
                limits,
                tables,
                features: offered & REQUIRED_FEATURES,
                cache: Cache::default(),
                failure,
            }),
        })
    }

    pub fn limits(&self) -> &wgpu::Limits {
        &self.state.limits
    }

    pub fn tables(&self) -> &crate::table::TableCapacity {
        &self.state.tables
    }

    pub fn cached_bind_groups(&self) -> usize {
        self.state.cache.len()
    }

    pub fn begin_frame(&self) {
        self.state.cache.begin_frame();
    }

    pub fn features(&self) -> wgpu::Features {
        self.state.features
    }

    pub fn failure(&self) -> Option<GpuFailure> {
        self.state.failure.take()
    }

    pub(crate) fn raise(&self, message: impl Into<String>) -> GpuFailure {
        self.state.failure.raise(message)
    }

    pub fn binding_alignment(&self) -> u64 {
        u64::from(self.state.limits.min_storage_buffer_offset_alignment)
    }

    pub(crate) fn native(&self) -> &wgpu::Device {
        &self.state.device
    }

    pub(crate) fn native_queue(&self) -> &wgpu::Queue {
        &self.state.queue
    }

    pub(crate) fn adapter(&self) -> &wgpu::Adapter {
        &self.state.adapter
    }

    pub(crate) fn cache(&self) -> &Cache {
        &self.state.cache
    }

    pub(crate) fn retirements(&self) -> &Arc<crate::identity::Retirements> {
        self.state.cache.retirements()
    }

    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }
}

impl Queue {
    pub fn of(device: &Device) -> Self {
        Self {
            device: device.clone(),
        }
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    fn poll(&self, poll: wgpu::PollType) -> Result<wgpu::PollStatus, wgpu::PollError> {
        self.device.native().poll(poll)
    }

    pub fn wait(&self, submission: SubmissionIndex, timeout: Duration) -> Result<(), GpuFailure> {
        match self.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(timeout),
        }) {
            Ok(_) => Ok(()),
            Err(wgpu::PollError::Timeout) => Err(self
                .device
                .raise(format!("a submission holds the queue for {timeout:?}"))),
            Err(error) => Err(self
                .device
                .raise(format!("a submission completes: {error}"))),
        }
    }

    pub fn complete(&self, submission: SubmissionIndex) -> Result<bool, GpuFailure> {
        match self.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::ZERO),
        }) {
            Ok(_) => Ok(true),
            Err(wgpu::PollError::Timeout) => Ok(false),
            Err(error) => Err(self
                .device
                .raise(format!("a submission completes: {error}"))),
        }
    }

    pub(crate) fn native(&self) -> &wgpu::Queue {
        self.device.native_queue()
    }

    pub(crate) fn read(
        &self,
        buffer: &GpuBuffer,
        submission: SubmissionIndex,
        bytes: u64,
    ) -> Result<Vec<u8>, GpuFailure> {
        self.wait(submission, crate::READBACK_TIMEOUT)?;
        let slice = buffer.native().slice(0..bytes);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(crate::READBACK_TIMEOUT),
        })
        .map_err(|error| self.device.raise(format!("a readback maps: {error}")))?;
        receiver
            .recv()
            .expect("a mapped readback answers its mapping")
            .map_err(|error| self.device.raise(format!("a readback maps: {error}")))?;
        let data = slice
            .get_mapped_range()
            .map_err(|error| {
                self.device
                    .raise(format!("a mapped readback yields its bytes: {error}"))
            })?
            .to_vec();
        buffer.native().unmap();
        Ok(data)
    }
}

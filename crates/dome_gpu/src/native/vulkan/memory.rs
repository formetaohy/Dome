use ash::vk;
use gpu_allocator::MemoryLocation;
use gpu_allocator::vulkan::{
    Allocation, AllocationCreateDesc, AllocationScheme, Allocator, AllocatorCreateDesc,
};
use std::sync::{Arc, Mutex, PoisonError};

pub(crate) struct Memory {
    allocator: Mutex<Allocator>,
}

impl Memory {
    pub(crate) fn new(
        instance: &ash::Instance,
        device: &ash::Device,
        physical: vk::PhysicalDevice,
    ) -> Arc<Self> {
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device: physical,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        })
        .unwrap_or_else(|error| panic!("creating the Vulkan memory pool: {error:?}"));
        Arc::new(Self {
            allocator: Mutex::new(allocator),
        })
    }

    pub(crate) fn allocate(
        self: &Arc<Self>,
        name: &str,
        requirements: vk::MemoryRequirements,
        location: MemoryLocation,
        linear: bool,
    ) -> Block {
        let mut allocator = self
            .allocator
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let allocation = allocator
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location,
                linear,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .unwrap_or_else(|error| panic!("suballocating {name}: {error:?}"));
        Block {
            memory: self.clone(),
            allocation: Some(allocation),
        }
    }
}

pub(crate) struct Block {
    memory: Arc<Memory>,
    allocation: Option<Allocation>,
}

impl Block {
    pub(crate) fn memory(&self) -> vk::DeviceMemory {
        unsafe { self.held().memory() }
    }

    pub(crate) fn offset(&self) -> u64 {
        self.held().offset()
    }

    pub(crate) fn mapped(&self) -> Option<*mut u8> {
        self.held()
            .mapped_ptr()
            .map(|pointer| pointer.cast::<u8>().as_ptr())
    }

    fn held(&self) -> &Allocation {
        self.allocation
            .as_ref()
            .expect("a memory block lives until it is released")
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        let Some(allocation) = self.allocation.take() else {
            return;
        };
        let mut allocator = self
            .memory
            .allocator
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        allocator
            .free(allocation)
            .unwrap_or_else(|error| panic!("releasing a Vulkan memory block: {error:?}"));
    }
}

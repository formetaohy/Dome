use crate::context::Device;
use crate::identity::{Liveness, ResourceId};
pub use wgpu::BufferUsages;

#[derive(Clone)]
pub struct GpuBuffer {
    device: Device,
    native: wgpu::Buffer,
    size: u64,
    usage: BufferUsages,
    liveness: std::sync::Arc<Liveness>,
}

#[derive(Clone, Copy)]
pub struct BufferBinding<'a> {
    pub(crate) buffer: &'a GpuBuffer,
    pub(crate) offset: u64,
    pub(crate) size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferRange {
    pub identity: ResourceId,
    pub offset: u64,
    pub size: u64,
}

impl BufferBinding<'_> {
    pub fn range(&self) -> BufferRange {
        BufferRange {
            identity: self.buffer.identity(),
            offset: self.offset,
            size: self.size,
        }
    }
}

impl GpuBuffer {
    pub fn new(device: &Device, label: &str, size: u64, usage: BufferUsages) -> Self {
        assert!(size > 0, "a buffer of {label} must hold bytes");
        assert!(!usage.is_empty(), "a buffer must have a purpose");
        assert!(
            size <= device.limits().max_buffer_size,
            "a buffer of {size} bytes exceeds the device limit of {}",
            device.limits().max_buffer_size,
        );
        let native = device.native().create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage,
            mapped_at_creation: false,
        });
        Self {
            device: device.clone(),
            native,
            size,
            usage,
            liveness: Liveness::new(device.retirements()),
        }
    }

    pub fn identity(&self) -> ResourceId {
        self.liveness.id()
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn usage(&self) -> BufferUsages {
        self.usage
    }

    pub(crate) fn device(&self) -> &Device {
        &self.device
    }

    pub(crate) fn native(&self) -> &wgpu::Buffer {
        &self.native
    }

    pub fn write(&self, queue: &crate::Queue, bytes: &[u8]) {
        self.write_at(queue, 0, bytes);
    }

    pub fn write_at(&self, queue: &crate::Queue, offset: u64, bytes: &[u8]) {
        assert!(
            self.device.same(queue.device()),
            "a queue cannot write another device's buffer"
        );
        assert!(
            self.usage.contains(BufferUsages::COPY_DST),
            "the buffer does not accept host writes"
        );
        assert!(
            offset.is_multiple_of(4) && bytes.len().is_multiple_of(4),
            "a buffer write must cover whole words"
        );
        assert!(
            offset
                .checked_add(bytes.len() as u64)
                .is_some_and(|end| end <= self.size),
            "writing {} bytes at {offset} exceeds a buffer of {} bytes",
            bytes.len(),
            self.size,
        );
        if !bytes.is_empty() {
            queue.native().write_buffer(&self.native, offset, bytes);
        }
    }

    pub fn binding(&self, offset: u64, size: u64) -> BufferBinding<'_> {
        assert!(offset.is_multiple_of(4) && size > 0 && size.is_multiple_of(4));
        assert!(offset.checked_add(size).is_some_and(|end| end <= self.size));
        BufferBinding {
            buffer: self,
            offset,
            size,
        }
    }

    pub fn read(
        &self,
        queue: &crate::Queue,
        submission: wgpu::SubmissionIndex,
        bytes: u64,
    ) -> Vec<u8> {
        assert!(
            self.device.same(queue.device()),
            "a queue cannot read another device's buffer"
        );
        assert!(
            self.usage.contains(BufferUsages::MAP_READ),
            "the buffer is not host readable"
        );
        assert!(
            bytes > 0 && bytes <= self.size,
            "reading {bytes} bytes exceeds the readback buffer"
        );
        queue.read(self, submission, bytes)
    }
}

use dome_gpu::{BufferUsages, Device, GpuBuffer};

const INITIAL_BYTES: u64 = 256;
const BLOCK_BYTES: u64 = 4096;

pub(crate) struct Growing {
    buffer: GpuBuffer,
    bytes: u64,
    label: &'static str,
    usage: BufferUsages,
}

impl Growing {
    pub(crate) fn new(device: &Device, label: &'static str, usage: BufferUsages) -> Self {
        Self {
            buffer: GpuBuffer::new(device, label, INITIAL_BYTES, usage),
            bytes: INITIAL_BYTES,
            label,
            usage,
        }
    }

    pub(crate) fn buffer(&self) -> &GpuBuffer {
        &self.buffer
    }

    pub(crate) fn fit(&mut self, device: &Device, bytes: u64) -> bool {
        if bytes <= self.bytes {
            return false;
        }
        let grown = bytes
            .next_multiple_of(BLOCK_BYTES)
            .max(self.bytes.saturating_mul(2));
        self.buffer = GpuBuffer::new(device, self.label, grown, self.usage);
        self.bytes = grown;
        true
    }
}

use crate::{Device, GpuBuffer, NativeTexture, Queue, ResourceId, Submission, TextureUsages};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TextureFormat {
    Rgba8Unorm,
    Rgba8UnormSrgb,
    Bgra8Unorm,
    Rgba16Float,
    R32Float,
    Depth32Float,
}

impl TextureFormat {
    pub const fn bytes_per_texel(self) -> u32 {
        match self {
            Self::Rgba8Unorm
            | Self::Rgba8UnormSrgb
            | Self::Bgra8Unorm
            | Self::R32Float
            | Self::Depth32Float => 4,
            Self::Rgba16Float => 8,
        }
    }

    pub const fn is_srgb(self) -> bool {
        matches!(self, Self::Rgba8UnormSrgb)
    }

    pub const fn is_depth(self) -> bool {
        matches!(self, Self::Depth32Float)
    }
}

const MAX_MIP_LEVELS: u32 = 32;

struct TextureState {
    native: NativeTexture,
    device: Device,
    width: u32,
    height: u32,
    mip_levels: u32,
    format: TextureFormat,
    usages: TextureUsages,
    identity: ResourceId,
}

#[derive(Clone)]
pub struct Texture {
    state: Arc<TextureState>,
}

impl Texture {
    pub const READBACK_ALIGNMENT: u64 = 256;

    pub fn new(
        device: &Device,
        label: &str,
        width: u32,
        height: u32,
        mip_levels: u32,
        format: TextureFormat,
        usages: TextureUsages,
    ) -> Self {
        assert!(
            width > 0 && height > 0,
            "a {label} texture spans {width} by {height} texels"
        );
        assert!(
            mip_levels > 0 && mip_levels <= Self::full_mip_levels(width, height),
            "a {label} texture of {width} by {height} texels holds {mip_levels} mip levels",
        );
        assert!(!usages.is_empty(), "a {label} texture names no use");
        assert!(
            !format.is_depth() || !usages.contains(TextureUsages::COPY_DST),
            "a {label} depth texture takes no texel upload",
        );
        assert!(
            u64::from(width)
                <= device.limits().max_buffer_size / u64::from(format.bytes_per_texel()),
            "a texture of {width} texels outruns the device's largest buffer"
        );
        Self {
            state: Arc::new(TextureState {
                native: device
                    .native()
                    .create_texture(label, width, height, mip_levels, format, usages),
                device: device.clone(),
                width,
                height,
                mip_levels,
                format,
                usages,
                identity: ResourceId::next(),
            }),
        }
    }

    pub(crate) fn of(
        device: &Device,
        native: NativeTexture,
        width: u32,
        height: u32,
        format: TextureFormat,
        usages: TextureUsages,
    ) -> Self {
        Self {
            state: Arc::new(TextureState {
                native,
                device: device.clone(),
                width,
                height,
                mip_levels: 1,
                format,
                usages,
                identity: ResourceId::next(),
            }),
        }
    }

    pub const fn full_mip_levels(width: u32, height: u32) -> u32 {
        let extent = if width > height { width } else { height };
        MAX_MIP_LEVELS - extent.leading_zeros()
    }

    pub fn identity(&self) -> ResourceId {
        self.state.identity
    }

    pub fn width(&self) -> u32 {
        self.state.width
    }

    pub fn height(&self) -> u32 {
        self.state.height
    }

    pub fn mip_count(&self) -> u32 {
        self.state.mip_levels
    }

    pub fn format(&self) -> TextureFormat {
        self.state.format
    }

    pub fn usages(&self) -> TextureUsages {
        self.state.usages
    }

    pub fn level_extent(&self, level: u32) -> (u32, u32) {
        assert!(
            level < self.state.mip_levels,
            "a texture of {} mip levels holds no level {level}",
            self.state.mip_levels,
        );
        (
            (self.state.width >> level).max(1),
            (self.state.height >> level).max(1),
        )
    }

    pub fn level_row_bytes(&self, level: u32) -> u64 {
        let (width, _) = self.level_extent(level);
        u64::from(width) * u64::from(self.state.format.bytes_per_texel())
    }

    pub fn level_row_stride(&self, level: u32) -> u64 {
        self.level_row_bytes(level)
            .div_ceil(Self::READBACK_ALIGNMENT)
            * Self::READBACK_ALIGNMENT
    }

    pub fn write(&self, queue: &Queue, pixels: &[u8]) {
        assert_eq!(
            self.state.mip_levels, 1,
            "writing one level of a {} level texture leaves the others undefined",
            self.state.mip_levels,
        );
        self.write_levels(queue, &[pixels]);
    }

    pub fn write_levels(&self, queue: &Queue, levels: &[&[u8]]) {
        assert!(
            self.state.usages.contains(TextureUsages::COPY_DST),
            "writing texels requires a copy destination texture"
        );
        assert_eq!(
            levels.len() as u32,
            self.state.mip_levels,
            "writing {} levels of a {} level texture leaves texels undefined",
            levels.len(),
            self.state.mip_levels,
        );
        let mut submission = Submission::new(self.device(), "dome texture write");
        for (level, pixels) in levels.iter().enumerate() {
            submission.write_texture(self, level as u32, pixels);
        }
        submission.submit(queue);
    }

    pub fn row_bytes(&self) -> u64 {
        self.level_row_bytes(0)
    }

    pub fn row_stride(&self) -> u64 {
        self.level_row_stride(0)
    }

    pub fn readback_bytes(&self) -> u64 {
        self.row_stride() * u64::from(self.state.height)
    }

    pub fn rows<'a>(&self, bytes: &'a [u8]) -> impl Iterator<Item = &'a [u8]> {
        let row_bytes = self.row_bytes() as usize;
        assert_eq!(
            bytes.len() as u64,
            self.readback_bytes(),
            "{} texels of {} need {} readback bytes and {} came back",
            self.state.width,
            self.state.height,
            self.readback_bytes(),
            bytes.len()
        );
        bytes
            .chunks(self.row_stride() as usize)
            .map(move |row| &row[..row_bytes])
    }

    pub fn read(&self, queue: &Queue) -> Vec<u8> {
        assert!(
            self.state.usages.contains(TextureUsages::COPY_SRC),
            "reading texels requires a copy source texture"
        );
        let mut submission = Submission::new(self.device(), "dome texture read");
        let buffer = GpuBuffer::new(
            self.device(),
            "dome texture readback",
            self.readback_bytes(),
            crate::BufferUsages::COPY_DST | crate::BufferUsages::MAP_READ,
        );
        submission.copy_texture(self, &buffer, 0);
        let submission = submission.submit(queue);
        queue.read(&buffer, submission, self.readback_bytes())
    }

    pub fn read_rgba(&self, queue: &Queue) -> Vec<u8> {
        let bytes = self.read(queue);
        let mut rgba = Vec::with_capacity(self.width() as usize * self.height() as usize * 4);
        for row in self.rows(&bytes) {
            match self.state.format {
                TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => {
                    rgba.extend_from_slice(row);
                }
                TextureFormat::Bgra8Unorm => {
                    for pixel in row.as_chunks::<4>().0 {
                        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                    }
                }
                other => panic!("a {other:?} texture holds no eight bit RGBA texels"),
            }
        }
        rgba
    }

    pub fn level(&self, level: u32) -> Self {
        assert!(
            level < self.state.mip_levels,
            "a texture of {} mip levels views no level {level}",
            self.state.mip_levels,
        );
        let (width, height) = self.level_extent(level);
        Self {
            state: Arc::new(TextureState {
                native: self
                    .state
                    .device
                    .native()
                    .create_texture_view(&self.state.native, level),
                device: self.state.device.clone(),
                width,
                height,
                mip_levels: 1,
                format: self.state.format,
                usages: self.state.usages,
                identity: ResourceId::next(),
            }),
        }
    }

    pub(crate) fn device(&self) -> &Device {
        &self.state.device
    }

    pub(crate) fn native(&self) -> &NativeTexture {
        &self.state.native
    }
}

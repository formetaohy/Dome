use crate::context::Device;
use crate::identity::{Liveness, ResourceId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
pub use wgpu::{TextureFormat, TextureUsages};

pub trait TextureFormatExt {
    fn bytes_per_texel(&self) -> u32;
    fn is_srgb(&self) -> bool;
    fn is_depth(&self) -> bool;
}

impl TextureFormatExt for TextureFormat {
    fn bytes_per_texel(&self) -> u32 {
        self.block_copy_size(None)
            .unwrap_or_else(|| panic!("a {self:?} texture holds no texel size"))
    }

    fn is_srgb(&self) -> bool {
        TextureFormat::is_srgb(self)
    }

    fn is_depth(&self) -> bool {
        self.is_depth_stencil_format()
    }
}

const MAX_MIP_LEVELS: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl TextureRegion {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        assert!(
            width > 0 && height > 0,
            "a {width} by {height} texture region holds no texel"
        );
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub(crate) fn assert_within(self, width: u32, height: u32) {
        assert!(
            self.x + self.width <= width && self.y + self.height <= height,
            "the region {} by {} at {} by {} lies outside a {width} by {height} texture",
            self.x,
            self.y,
            self.width,
            self.height,
        );
    }
}

#[derive(Clone)]
pub struct Texture {
    device: Device,
    native: wgpu::Texture,
    width: u32,
    height: u32,
    mip_levels: u32,
    format: TextureFormat,
    usages: TextureUsages,
    liveness: std::sync::Arc<Liveness>,
    base_mip: u32,
    view: wgpu::TextureView,
    views: Arc<Mutex<HashMap<u32, wgpu::TextureView>>>,
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
        let native = device.native().create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mip_levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: usages,
            view_formats: &[],
        });
        let view = native.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            device: device.clone(),
            native,
            width,
            height,
            mip_levels,
            format,
            usages,
            liveness: Liveness::new(device.retirements()),
            base_mip: 0,
            view,
            views: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn of(
        device: &Device,
        native: wgpu::Texture,
        width: u32,
        height: u32,
        format: TextureFormat,
        usages: TextureUsages,
    ) -> Self {
        let view = native.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            device: device.clone(),
            native,
            width,
            height,
            mip_levels: 1,
            format,
            usages,
            liveness: Liveness::new(device.retirements()),
            base_mip: 0,
            view,
            views: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub const fn full_mip_levels(width: u32, height: u32) -> u32 {
        let extent = if width > height { width } else { height };
        MAX_MIP_LEVELS - extent.leading_zeros()
    }

    pub fn identity(&self) -> ResourceId {
        self.liveness.id()
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn mip_count(&self) -> u32 {
        self.mip_levels
    }

    pub fn format(&self) -> TextureFormat {
        self.format
    }

    pub fn usages(&self) -> TextureUsages {
        self.usages
    }

    pub fn level_extent(&self, level: u32) -> (u32, u32) {
        assert!(
            level < self.mip_levels,
            "a texture of {} mip levels holds no level {level}",
            self.mip_levels,
        );
        ((self.width >> level).max(1), (self.height >> level).max(1))
    }

    pub fn level_row_bytes(&self, level: u32) -> u64 {
        let (width, _) = self.level_extent(level);
        u64::from(width) * u64::from(self.format.bytes_per_texel())
    }

    pub fn level_row_stride(&self, level: u32) -> u64 {
        self.level_row_bytes(level)
            .div_ceil(Self::READBACK_ALIGNMENT)
            * Self::READBACK_ALIGNMENT
    }

    pub fn write(&self, queue: &crate::Queue, pixels: &[u8]) {
        assert_eq!(
            self.mip_levels, 1,
            "writing one level of a {} level texture leaves the others undefined",
            self.mip_levels,
        );
        self.write_levels(queue, &[pixels]);
    }

    pub fn write_levels(&self, queue: &crate::Queue, levels: &[&[u8]]) {
        assert!(
            self.usages.contains(TextureUsages::COPY_DST),
            "writing texels requires a copy destination texture"
        );
        assert_eq!(
            levels.len() as u32,
            self.mip_levels,
            "writing {} levels of a {} level texture leaves texels undefined",
            levels.len(),
            self.mip_levels,
        );
        for (level, pixels) in levels.iter().enumerate() {
            let level = level as u32;
            let (width, height) = self.level_extent(level);
            let row = self.level_row_bytes(level);
            assert_eq!(
                pixels.len() as u64,
                row * u64::from(height),
                "writing {level} of a {width} by {height} texture reads {} bytes where {} come in",
                row * u64::from(height),
                pixels.len(),
            );
            queue.native().write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.native,
                    mip_level: level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row as u32),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    pub fn write_region(
        &self,
        queue: &crate::Queue,
        region: TextureRegion,
        row_pitch: u64,
        pixels: &[u8],
    ) {
        let mut submission = crate::Submission::new(&self.device, "dome texture region write");
        submission.write_texture_region(self, 0, region, row_pitch, pixels);
        submission.submit(queue);
    }

    pub fn row_bytes(&self) -> u64 {
        self.level_row_bytes(0)
    }

    pub fn row_stride(&self) -> u64 {
        self.level_row_stride(0)
    }

    pub fn readback_bytes(&self) -> u64 {
        self.row_stride() * u64::from(self.height)
    }

    pub fn rows<'a>(&self, bytes: &'a [u8]) -> impl Iterator<Item = &'a [u8]> {
        let row_bytes = self.row_bytes() as usize;
        assert_eq!(
            bytes.len() as u64,
            self.readback_bytes(),
            "{} texels of {} need {} readback bytes and {} came back",
            self.width,
            self.height,
            self.readback_bytes(),
            bytes.len()
        );
        bytes
            .chunks(self.row_stride() as usize)
            .map(move |row| &row[..row_bytes])
    }

    pub fn read(&self, queue: &crate::Queue) -> Result<Vec<u8>, crate::GpuFailure> {
        assert!(
            self.usages.contains(TextureUsages::COPY_SRC),
            "reading texels requires a copy source texture"
        );
        let mut submission = crate::Submission::new(&self.device, "dome texture read");
        let buffer = crate::GpuBuffer::new(
            &self.device,
            "dome texture readback",
            self.readback_bytes(),
            crate::BufferUsages::COPY_DST | crate::BufferUsages::MAP_READ,
        );
        submission.copy_texture(self, &buffer, 0);
        let submission = submission.submit(queue);
        queue.read(&buffer, submission, self.readback_bytes())
    }

    pub fn read_rgba(&self, queue: &crate::Queue) -> Result<Vec<u8>, crate::GpuFailure> {
        let bytes = self.read(queue)?;
        let mut rgba = Vec::with_capacity(self.width as usize * self.height as usize * 4);
        for row in self.rows(&bytes) {
            match self.format {
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
        Ok(rgba)
    }

    pub fn level(&self, level: u32) -> Self {
        assert!(
            level < self.mip_levels,
            "a texture of {} mip levels views no level {level}",
            self.mip_levels,
        );
        let (width, height) = self.level_extent(level);
        let view = self.level_view(level);
        Self {
            device: self.device.clone(),
            native: self.native.clone(),
            width,
            height,
            mip_levels: 1,
            format: self.format,
            usages: self.usages,
            liveness: Liveness::new(self.device.retirements()),
            base_mip: self.base_mip + level,
            view,
            views: Arc::clone(&self.views),
        }
    }

    pub(crate) fn device(&self) -> &Device {
        &self.device
    }

    pub(crate) fn native(&self) -> &wgpu::Texture {
        &self.native
    }

    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub(crate) fn level_view(&self, level: u32) -> wgpu::TextureView {
        let mip = self.base_mip + level;
        self.views
            .lock()
            .expect("a texture view cache is never poisoned")
            .entry(mip)
            .or_insert_with(|| {
                self.native.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .clone()
    }
}

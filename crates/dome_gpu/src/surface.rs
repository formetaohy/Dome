use crate::context::Device;
use crate::failure::GpuFailure;
use crate::texture::{Texture, TextureFormat, TextureUsages};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) fn create<W: Into<wgpu::SurfaceTarget<'static>>>(
    instance: &wgpu::Instance,
    target: W,
) -> wgpu::Surface<'static> {
    instance
        .create_surface(target)
        .unwrap_or_else(|error| panic!("a window opens its present surface: {error}"))
}

pub struct Surface {
    device: Device,
    native: wgpu::Surface<'static>,
    recreate: crate::context::Recreate,
    width: u32,
    height: u32,
    format: TextureFormat,
    generation: Arc<AtomicU64>,
}

pub struct SurfaceFrame {
    pub(crate) texture: Texture,
    pub(crate) surface: Option<wgpu::SurfaceTexture>,
    pub(crate) token: PresentToken,
}

#[derive(Clone)]
pub(crate) struct PresentToken {
    generation: Arc<AtomicU64>,
    frame: u64,
}

impl PresentToken {
    fn of(generation: &Arc<AtomicU64>) -> Self {
        Self {
            generation: Arc::clone(generation),
            frame: generation.load(Ordering::Acquire),
        }
    }

    pub(crate) fn valid(&self) -> bool {
        self.generation.load(Ordering::Acquire) == self.frame
    }
}

impl Surface {
    pub(crate) fn new(
        device: Device,
        native: wgpu::Surface<'static>,
        recreate: crate::context::Recreate,
        width: u32,
        height: u32,
    ) -> Self {
        assert!(
            width > 0 && height > 0,
            "a surface spans {width} by {height} texels"
        );
        let format = select_format(&native, device.adapter());
        let surface = Self {
            device,
            native,
            recreate,
            width,
            height,
            format,
            generation: Arc::new(AtomicU64::new(0)),
        };
        surface.configure();
        surface
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn format(&self) -> TextureFormat {
        self.format
    }

    fn configure(&self) {
        self.native
            .configure(self.device.native(), &self.configuration());
    }

    fn configuration(&self) -> wgpu::SurfaceConfiguration {
        wgpu::SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: self.format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: self.width,
            height: self.height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: Vec::new(),
        }
    }

    pub fn acquire(&mut self) -> Result<Option<SurfaceFrame>, GpuFailure> {
        let mut attempts = 0;
        loop {
            match self.native.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame)
                | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                    let width = self.width;
                    let height = self.height;
                    let format = self.format;
                    let token = PresentToken::of(&self.generation);
                    let texture = Texture::of(
                        &self.device,
                        frame.texture.clone(),
                        width,
                        height,
                        format,
                        TextureUsages::RENDER_ATTACHMENT,
                    );
                    return Ok(Some(SurfaceFrame {
                        texture,
                        surface: Some(frame),
                        token,
                    }));
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(None);
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    attempts += 1;
                    if attempts > 1 {
                        return Err(self
                            .device
                            .raise("the surface stays outdated after reconfiguring it"));
                    }
                    self.configure();
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    attempts += 1;
                    if attempts > 1 {
                        return Err(self
                            .device
                            .raise("the surface stays lost after recreating it"));
                    }
                    self.rebuild();
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(self.device.raise("the surface fails validation"));
                }
            }
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        assert!(
            width > 0 && height > 0,
            "a surface spans {width} by {height} texels"
        );
        self.width = width;
        self.height = height;
        self.configure();
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    fn rebuild(&mut self) {
        self.native = (self.recreate)();
        self.format = select_format(&self.native, self.device.adapter());
        self.configure();
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

fn select_format(surface: &wgpu::Surface<'_>, adapter: &wgpu::Adapter) -> TextureFormat {
    let capabilities = surface.get_capabilities(adapter);
    for preferred in [TextureFormat::Bgra8Unorm, TextureFormat::Rgba8Unorm] {
        if capabilities.formats.contains(&preferred) {
            return preferred;
        }
    }
    capabilities
        .formats
        .iter()
        .copied()
        .find(|format| !format.is_srgb())
        .unwrap_or_else(|| panic!("a swapchain presents eight bit linear colour"))
}

impl SurfaceFrame {
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    pub fn width(&self) -> u32 {
        self.texture.width()
    }

    pub fn height(&self) -> u32 {
        self.texture.height()
    }

    pub fn format(&self) -> TextureFormat {
        self.texture.format()
    }

    pub(crate) fn device(&self) -> &Device {
        self.texture.device()
    }
}

impl Drop for SurfaceFrame {
    fn drop(&mut self) {
        assert!(
            self.surface.is_none(),
            "a surface frame presents before it is dropped"
        );
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

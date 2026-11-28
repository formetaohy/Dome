use crate::context::Device;
use crate::event::GpuEvent;
use crate::failure::GpuFailure;
use crate::texture::{Texture, TextureFormat, TextureUsages};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) fn create<W: Into<wgpu::SurfaceTarget<'static>>>(
    instance: &wgpu::Instance,
    target: W,
) -> Result<wgpu::Surface<'static>, wgpu::CreateSurfaceError> {
    instance.create_surface(target)
}

pub struct Surface {
    device: Device,
    native: Option<wgpu::Surface<'static>>,
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
        let format = select_format(&native, device.adapter());
        let surface = Self {
            device,
            native: Some(native),
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
        let Some(native) = &self.native else {
            return;
        };
        if self.width == 0 || self.height == 0 {
            return;
        }
        native.configure(self.device.native(), &self.configuration());
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
        if self.width == 0 || self.height == 0 {
            return Ok(None);
        }
        let mut reconfigured = false;
        let mut rebuilt = false;
        loop {
            let Some(native) = &self.native else {
                return Err(GpuFailure::SurfaceLost);
            };
            match native.get_current_texture() {
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
                    self.device.emit(GpuEvent::SurfaceOutdated);
                    if reconfigured {
                        return Err(GpuFailure::SurfaceOutdated);
                    }
                    reconfigured = true;
                    self.configure();
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    self.device.emit(GpuEvent::SurfaceLost);
                    if rebuilt {
                        return Err(GpuFailure::SurfaceLost);
                    }
                    rebuilt = true;
                    self.rebuild()?;
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(self.device.raise("the surface fails validation"));
                }
            }
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.configure();
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    fn rebuild(&mut self) -> Result<(), GpuFailure> {
        self.native = None;
        let native = (self.recreate)().map_err(|error| {
            self.device
                .raise(format!("the present surface recreates itself: {error}"))
        })?;
        self.format = select_format(&native, self.device.adapter());
        self.native = Some(native);
        self.configure();
        self.generation.fetch_add(1, Ordering::AcqRel);
        Ok(())
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
    pub fn discard(mut self) {
        drop(self.surface.take());
    }

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
        if std::thread::panicking() {
            return;
        }
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

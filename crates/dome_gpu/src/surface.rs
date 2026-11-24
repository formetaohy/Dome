use crate::context::Device;
use crate::texture::{Texture, TextureFormat, TextureUsages};
use core::ffi::{c_ulong, c_void};
use raw_window_handle::{
    AndroidDisplayHandle, AndroidNdkWindowHandle, AppKitDisplayHandle, AppKitWindowHandle,
    RawDisplayHandle, RawWindowHandle, UiKitDisplayHandle, UiKitWindowHandle, WaylandDisplayHandle,
    WaylandWindowHandle, Win32WindowHandle, WindowsDisplayHandle, XlibDisplayHandle,
    XlibWindowHandle,
};
use std::num::NonZeroIsize;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowHandle {
    Windows { window: usize, instance: usize },
    AppKit { view: usize },
    UiKit { view: usize },
    Xlib { display: usize, window: usize },
    Wayland { display: usize, surface: usize },
    Android { window: usize },
}

impl WindowHandle {
    pub fn of(window: RawWindowHandle, display: RawDisplayHandle) -> Self {
        match (window, display) {
            (RawWindowHandle::Win32(window), _) => Self::Windows {
                window: window.hwnd.get() as usize,
                instance: window.hinstance.map_or(0, |held| held.get() as usize),
            },
            (RawWindowHandle::AppKit(window), _) => Self::AppKit {
                view: window.ns_view.as_ptr() as usize,
            },
            (RawWindowHandle::UiKit(window), _) => Self::UiKit {
                view: window.ui_view.as_ptr() as usize,
            },
            (RawWindowHandle::Xlib(window), RawDisplayHandle::Xlib(display)) => Self::Xlib {
                display: display
                    .display
                    .expect("an Xlib display handle names its display")
                    .as_ptr() as usize,
                window: window.window as usize,
            },
            (RawWindowHandle::Wayland(window), RawDisplayHandle::Wayland(display)) => {
                Self::Wayland {
                    display: display.display.as_ptr() as usize,
                    surface: window.surface.as_ptr() as usize,
                }
            }
            (RawWindowHandle::AndroidNdk(window), _) => Self::Android {
                window: window.a_native_window.as_ptr() as usize,
            },
            _ => panic!(
                "a window surface needs a Win32, AppKit, UIKit, Xlib, Wayland or Android handle"
            ),
        }
    }

    pub(crate) fn raw(self) -> (RawWindowHandle, RawDisplayHandle) {
        match self {
            Self::Windows { window, instance } => {
                let mut handle = Win32WindowHandle::new(
                    NonZeroIsize::new(window as isize).expect("a window handle names a window"),
                );
                handle.hinstance = NonZeroIsize::new(instance as isize);
                (
                    RawWindowHandle::Win32(handle),
                    RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
                )
            }
            Self::AppKit { view } => (
                RawWindowHandle::AppKit(AppKitWindowHandle::new(
                    NonNull::new(view as *mut c_void).expect("a view handle names a view"),
                )),
                RawDisplayHandle::AppKit(AppKitDisplayHandle::new()),
            ),
            Self::UiKit { view } => (
                RawWindowHandle::UiKit(UiKitWindowHandle::new(
                    NonNull::new(view as *mut c_void).expect("a view handle names a view"),
                )),
                RawDisplayHandle::UiKit(UiKitDisplayHandle::new()),
            ),
            Self::Xlib { display, window } => (
                RawWindowHandle::Xlib(XlibWindowHandle::new(window as c_ulong)),
                RawDisplayHandle::Xlib(XlibDisplayHandle::new(
                    Some(
                        NonNull::new(display as *mut c_void)
                            .expect("an Xlib handle names its display"),
                    ),
                    0,
                )),
            ),
            Self::Wayland { display, surface } => (
                RawWindowHandle::Wayland(WaylandWindowHandle::new(
                    NonNull::new(surface as *mut c_void)
                        .expect("a Wayland handle names its surface"),
                )),
                RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                    NonNull::new(display as *mut c_void)
                        .expect("a Wayland handle names its display"),
                )),
            ),
            Self::Android { window } => (
                RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(
                    NonNull::new(window as *mut c_void)
                        .expect("an Android handle names its native window"),
                )),
                RawDisplayHandle::Android(AndroidDisplayHandle::new()),
            ),
        }
    }
}

pub(crate) fn create(instance: &wgpu::Instance, handle: WindowHandle) -> wgpu::Surface<'static> {
    let (window, display) = handle.raw();
    unsafe {
        instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(display),
                raw_window_handle: window,
            })
            .unwrap_or_else(|error| panic!("a window opens its present surface: {error}"))
    }
}

pub struct Surface {
    device: Device,
    native: wgpu::Surface<'static>,
    window: WindowHandle,
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
        device: &Device,
        native: wgpu::Surface<'static>,
        window: WindowHandle,
        width: u32,
        height: u32,
    ) -> Self {
        assert!(
            width > 0 && height > 0,
            "a surface spans {width} by {height} texels"
        );
        let format = select_format(&native, device.adapter());
        let device = device.clone();
        let surface = Self {
            device,
            native,
            window,
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

    pub fn acquire(&mut self) -> Option<SurfaceFrame> {
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
                    return Some(SurfaceFrame {
                        texture,
                        surface: Some(frame),
                        token,
                    });
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return None;
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    attempts += 1;
                    assert!(attempts < 2, "a surface stays outdated after reconfiguring");
                    self.configure();
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    attempts += 1;
                    assert!(attempts < 2, "a surface stays lost after recreating it");
                    self.rebuild();
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    panic!("a surface acquisition fails validation")
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
        self.native = create(crate::context::instance(), self.window);
        let format = select_format(&self.native, self.device.adapter());
        assert_eq!(
            format, self.format,
            "a lost surface presents a {:?} format where its renderer paints {:?}",
            format, self.format,
        );
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

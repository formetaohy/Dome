use crate::capability::TextureUsages;
use crate::context::Device;
use crate::native::NativeSurface;
use crate::texture::{Texture, TextureFormat};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
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
}

pub struct Surface {
    device: Device,
    native: NativeSurface,
    width: u32,
    height: u32,
    format: TextureFormat,
    generation: Arc<AtomicU64>,
}

pub struct SurfaceFrame {
    pub(crate) texture: Texture,
    pub(crate) native: Option<crate::native::NativePresent>,
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
    pub(crate) fn new(device: &Device, width: u32, height: u32) -> Self {
        assert!(
            width > 0 && height > 0,
            "a surface spans {width} by {height} texels"
        );
        let native = device.native().create_surface(width, height);
        Self {
            device: device.clone(),
            width: native.width(),
            height: native.height(),
            format: native.format(),
            native,
            generation: Arc::new(AtomicU64::new(0)),
        }
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

    pub fn acquire(&mut self) -> Option<SurfaceFrame> {
        if self.native.outdated() {
            self.rebuild();
        }
        let mut frame = self.native.acquire();
        if frame.is_none() && self.native.outdated() {
            self.rebuild();
            frame = self.native.acquire();
        }
        let frame = frame?;
        self.width = frame.width;
        self.height = frame.height;
        let texture = Texture::of(
            &self.device,
            frame.texture,
            frame.width,
            frame.height,
            frame.format,
            TextureUsages::RENDER_ATTACHMENT,
        );
        Some(SurfaceFrame {
            texture,
            native: Some(frame.present),
            token: PresentToken::of(&self.generation),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        assert!(
            width > 0 && height > 0,
            "a surface spans {width} by {height} texels"
        );
        self.native.resize(width, height);
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.width = width;
        self.height = height;
        self.format = self.native.format();
    }

    fn rebuild(&mut self) {
        let width = self.native.width();
        let height = self.native.height();
        self.native.resize(width, height);
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.width = self.native.width();
        self.height = self.native.height();
        self.format = self.native.format();
    }
}

impl Drop for SurfaceFrame {
    fn drop(&mut self) {
        assert!(
            self.native.is_none(),
            "a surface frame presents before it is dropped"
        );
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
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
}

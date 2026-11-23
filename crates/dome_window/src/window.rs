use winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub minimum_size: Option<(u32, u32)>,
}

impl WindowConfig {
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        assert!(
            width > 0 && height > 0,
            "a window of {width} by {height} spans no surface"
        );
        Self {
            title: title.into(),
            width,
            height,
            minimum_size: None,
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self::new("Dome", 1280, 720)
    }
}

pub struct Window {
    raw: winit::window::Window,
}

impl Window {
    pub(crate) fn new(raw: winit::window::Window) -> Self {
        Self { raw }
    }

    pub fn size(&self) -> (u32, u32) {
        let size = self.raw.inner_size();
        (size.width, size.height)
    }

    pub fn scale_factor(&self) -> f64 {
        self.raw.scale_factor()
    }

    pub fn is_minimized(&self) -> bool {
        self.raw.is_minimized().unwrap_or(false)
    }

    pub fn has_focus(&self) -> bool {
        self.raw.has_focus()
    }

    pub fn set_theme(&self, theme: Option<winit::window::Theme>) {
        self.raw.set_theme(theme);
    }

    pub fn set_ime_allowed(&self, allowed: bool) {
        self.raw.set_ime_allowed(allowed);
    }

    pub fn set_ime_cursor_area(&self, x: f64, y: f64, width: f64, height: f64) {
        self.raw.set_ime_cursor_area(
            winit::dpi::LogicalPosition::new(x, y),
            winit::dpi::LogicalSize::new(width, height),
        );
    }

    pub fn set_title(&self, title: &str) {
        self.raw.set_title(title);
    }

    pub fn request_redraw(&self) {
        self.raw.request_redraw();
    }

    pub fn window_handle(&self) -> RawWindowHandle {
        self.raw
            .window_handle()
            .expect("a native window exposes a raw window handle")
            .as_raw()
    }

    pub fn display_handle(&self) -> RawDisplayHandle {
        self.raw
            .display_handle()
            .expect("a native window exposes a raw display handle")
            .as_raw()
    }
}

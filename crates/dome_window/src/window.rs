use std::sync::Arc;

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
    native: Arc<winit::window::Window>,
}

impl Window {
    pub(crate) fn new(native: winit::window::Window) -> Self {
        Self {
            native: Arc::new(native),
        }
    }

    pub fn target(&self) -> Arc<winit::window::Window> {
        Arc::clone(&self.native)
    }

    pub fn size(&self) -> (u32, u32) {
        let size = self.native.inner_size();
        (size.width, size.height)
    }

    pub fn scale_factor(&self) -> f64 {
        self.native.scale_factor()
    }

    pub fn is_minimized(&self) -> bool {
        self.native.is_minimized().unwrap_or(false)
    }

    pub fn has_focus(&self) -> bool {
        self.native.has_focus()
    }

    pub fn set_theme(&self, theme: Option<winit::window::Theme>) {
        self.native.set_theme(theme);
    }

    pub fn set_ime_allowed(&self, allowed: bool) {
        self.native.set_ime_allowed(allowed);
    }

    pub fn set_ime_cursor_area(&self, x: f64, y: f64, width: f64, height: f64) {
        self.native.set_ime_cursor_area(
            winit::dpi::LogicalPosition::new(x, y),
            winit::dpi::LogicalSize::new(width, height),
        );
    }

    pub fn set_title(&self, title: &str) {
        self.native.set_title(title);
    }

    pub fn hide(&self) {
        self.native.set_visible(false);
    }

    pub fn request_redraw(&self) {
        self.native.request_redraw();
    }
}

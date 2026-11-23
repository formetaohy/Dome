pub use winit::event::MouseButton;
pub use winit::keyboard::{KeyCode, ModifiersState};

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Resized {
        width: u32,
        height: u32,
    },
    ScaleFactorChanged {
        scale_factor: f64,
    },
    Focused(bool),
    Key {
        code: KeyCode,
        character: Option<char>,
        pressed: bool,
        repeat: bool,
        text: Option<String>,
    },
    Button {
        button: MouseButton,
        pressed: bool,
    },
    CursorMoved {
        x: f32,
        y: f32,
    },
    CursorLeft,
    Wheel {
        lines: [f32; 2],
        pixels: [f32; 2],
    },
    Modifiers(ModifiersState),
}

use crate::event::{Event, KeyCode, ModifiersState, MouseButton};
use std::collections::HashSet;

#[derive(Debug)]
pub struct Input {
    keys: HashSet<KeyCode>,
    keys_pressed: HashSet<KeyCode>,
    keys_released: HashSet<KeyCode>,
    buttons: HashSet<MouseButton>,
    buttons_pressed: HashSet<MouseButton>,
    buttons_released: HashSet<MouseButton>,
    cursor: Option<[f32; 2]>,
    cursor_delta: [f32; 2],
    scroll_lines: [f32; 2],
    scroll_pixels: [f32; 2],
    modifiers: ModifiersState,
    focused: bool,
}

impl Input {
    pub fn handle(&mut self, event: Event) {
        match event {
            Event::Key {
                code,
                pressed,
                repeat,
                ..
            } => {
                if pressed {
                    let fresh = self.keys.insert(code);
                    if fresh || !repeat {
                        self.keys_pressed.insert(code);
                    }
                } else if self.keys.remove(&code) {
                    self.keys_released.insert(code);
                }
            }
            Event::Button { button, pressed } => {
                if pressed {
                    if self.buttons.insert(button) {
                        self.buttons_pressed.insert(button);
                    }
                } else if self.buttons.remove(&button) {
                    self.buttons_released.insert(button);
                }
            }
            Event::CursorMoved { x, y } => {
                if let Some(previous) = self.cursor {
                    self.cursor_delta[0] += x - previous[0];
                    self.cursor_delta[1] += y - previous[1];
                }
                self.cursor = Some([x, y]);
            }
            Event::CursorLeft => self.cursor = None,
            Event::Wheel { lines, pixels } => {
                self.scroll_lines[0] += lines[0];
                self.scroll_lines[1] += lines[1];
                self.scroll_pixels[0] += pixels[0];
                self.scroll_pixels[1] += pixels[1];
            }
            Event::Modifiers(modifiers) => self.modifiers = modifiers,
            Event::Focused(focused) => {
                self.focused = focused;
                if !focused {
                    self.keys.clear();
                    self.buttons.clear();
                }
            }
            Event::Resized { .. } | Event::ScaleFactorChanged { .. } => {}
        }
    }

    pub fn hold_pointer(&mut self) {
        self.cursor_delta = [0.0; 2];
        self.scroll_lines = [0.0; 2];
        self.scroll_pixels = [0.0; 2];
    }

    pub fn begin_frame(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.buttons_pressed.clear();
        self.buttons_released.clear();
        self.cursor_delta = [0.0; 2];
        self.scroll_lines = [0.0; 2];
        self.scroll_pixels = [0.0; 2];
    }

    pub fn keys(&self) -> impl Iterator<Item = KeyCode> + '_ {
        self.keys.iter().copied()
    }

    pub fn buttons(&self) -> impl Iterator<Item = MouseButton> + '_ {
        self.buttons.iter().copied()
    }

    pub fn key_down(&self, key: KeyCode) -> bool {
        self.keys.contains(&key)
    }

    pub fn key_pressed(&self, key: KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    pub fn key_released(&self, key: KeyCode) -> bool {
        self.keys_released.contains(&key)
    }

    pub fn button_down(&self, button: MouseButton) -> bool {
        self.buttons.contains(&button)
    }

    pub fn button_pressed(&self, button: MouseButton) -> bool {
        self.buttons_pressed.contains(&button)
    }

    pub fn button_released(&self, button: MouseButton) -> bool {
        self.buttons_released.contains(&button)
    }

    pub fn cursor(&self) -> Option<[f32; 2]> {
        self.cursor
    }

    pub fn cursor_delta(&self) -> [f32; 2] {
        self.cursor_delta
    }

    pub fn scroll_lines(&self) -> [f32; 2] {
        self.scroll_lines
    }

    pub fn scroll_pixels(&self) -> [f32; 2] {
        self.scroll_pixels
    }

    pub fn modifiers(&self) -> ModifiersState {
        self.modifiers
    }

    pub fn focused(&self) -> bool {
        self.focused
    }
}

impl Default for Input {
    fn default() -> Self {
        Self {
            keys: HashSet::new(),
            keys_pressed: HashSet::new(),
            keys_released: HashSet::new(),
            buttons: HashSet::new(),
            buttons_pressed: HashSet::new(),
            buttons_released: HashSet::new(),
            cursor: None,
            cursor_delta: [0.0; 2],
            scroll_lines: [0.0; 2],
            scroll_pixels: [0.0; 2],
            modifiers: ModifiersState::empty(),
            focused: true,
        }
    }
}

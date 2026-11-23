use crate::input::{Button, Event, Input, Key, Modifiers, ScrollDelta};
use dome_math::Vec2;
use dome_window::{Event as WindowEvent, Ime, KeyCode, MouseButton};

const LINE_STEP: f32 = 40.0;

impl Input {
    pub fn window(&mut self, event: &WindowEvent, scale: f32) {
        let scale = scale.max(f32::EPSILON);
        match event {
            WindowEvent::CursorMoved { x, y } => {
                self.push(Event::Moved(Vec2::new(x / scale, y / scale)));
            }
            WindowEvent::CursorLeft => self.push(Event::Leave),
            WindowEvent::Button { button, pressed } => {
                if let Some(button) = button_of(*button) {
                    self.push(if *pressed {
                        Event::Down(button)
                    } else {
                        Event::Up(button)
                    });
                }
            }
            WindowEvent::Wheel { lines, pixels } => {
                let (right, down) = if *pixels != [0.0; 2] {
                    (pixels[0] / scale, -pixels[1] / scale)
                } else {
                    (lines[0] * LINE_STEP, -lines[1] * LINE_STEP)
                };
                self.push(Event::Scroll(ScrollDelta { down, right }));
            }
            WindowEvent::Key {
                code,
                character,
                pressed,
                repeat,
                text,
            } => {
                let named = key_of(*code);
                match (named, character) {
                    (Some(key), _) => self.push(Event::Key {
                        key,
                        pressed: *pressed,
                        repeat: *repeat,
                    }),
                    (None, Some(character)) => self.push(Event::Key {
                        key: Key::Character(*character),
                        pressed: *pressed,
                        repeat: *repeat,
                    }),
                    (None, None) => {}
                }
                if *pressed && let Some(text) = text {
                    let modifiers = self.modifiers();
                    if !modifiers.control
                        && !modifiers.alt
                        && !modifiers.logo
                        && text.chars().any(|held| !held.is_control())
                    {
                        self.push(Event::Text(text.clone()));
                    }
                }
            }
            WindowEvent::Ime(ime) => match ime {
                Ime::Preedit(text) => {
                    self.push(Event::Preedit((!text.is_empty()).then(|| text.clone())));
                }
                Ime::Commit(text) => {
                    self.push(Event::Preedit(None));
                    self.push(Event::Text(text.clone()));
                }
                Ime::Enabled => {}
                Ime::Disabled => self.push(Event::Preedit(None)),
            },
            WindowEvent::Modifiers(state) => self.push(Event::Modifiers(Modifiers {
                shift: state.shift_key(),
                control: state.control_key(),
                alt: state.alt_key(),
                logo: state.super_key(),
            })),
            WindowEvent::Focused(focused) => {
                if !*focused {
                    self.push(Event::Cancel);
                }
            }
            WindowEvent::Resized { .. } | WindowEvent::ScaleFactorChanged { .. } => {}
        }
    }
}

fn button_of(button: MouseButton) -> Option<Button> {
    Some(match button {
        MouseButton::Left => Button::Primary,
        MouseButton::Right => Button::Secondary,
        MouseButton::Middle => Button::Middle,
        _ => return None,
    })
}

fn key_of(code: KeyCode) -> Option<Key> {
    Some(match code {
        KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,
        KeyCode::Space => Key::Space,
        KeyCode::Escape => Key::Escape,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::ArrowLeft => Key::Left,
        KeyCode::ArrowRight => Key::Right,
        KeyCode::ArrowUp => Key::Up,
        KeyCode::ArrowDown => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Tab => Key::Tab,
        KeyCode::F1 => Key::Function(1),
        KeyCode::F2 => Key::Function(2),
        KeyCode::F3 => Key::Function(3),
        KeyCode::F4 => Key::Function(4),
        KeyCode::F5 => Key::Function(5),
        KeyCode::F6 => Key::Function(6),
        KeyCode::F7 => Key::Function(7),
        KeyCode::F8 => Key::Function(8),
        KeyCode::F9 => Key::Function(9),
        KeyCode::F10 => Key::Function(10),
        KeyCode::F11 => Key::Function(11),
        KeyCode::F12 => Key::Function(12),
        _ => return None,
    })
}

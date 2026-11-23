use crate::event::Event;
use crate::window::{Window, WindowConfig};
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Exit,
}

pub trait Handler: 'static {
    fn opened(&mut self, window: &Window);

    fn frame(&mut self, window: &Window, real: Duration) -> Flow;

    fn event(&mut self, _window: &Window, _event: Event) {}

    fn suspended(&mut self, _window: &Window) -> Flow {
        Flow::Exit
    }

    fn closed(&mut self, _window: &Window) {}
}

pub fn run<H: Handler>(config: WindowConfig, handler: H) {
    let event_loop = EventLoop::new().expect("the platform opens a window event loop");
    let mut host = Host {
        config,
        handler,
        window: None,
        last: None,
    };
    event_loop
        .run_app(&mut host)
        .expect("the platform runs a window event loop");
}

struct Host<H: Handler> {
    config: WindowConfig,
    handler: H,
    window: Option<Window>,
    last: Option<Instant>,
}

impl<H: Handler> Host<H> {
    fn close(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.take() {
            self.handler.closed(&window);
        }
        event_loop.exit();
    }
}

impl<H: Handler> ApplicationHandler for Host<H> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        assert!(
            self.window.is_none(),
            "a resumed window host holds the window it resumes"
        );
        let mut attributes = winit::window::Window::default_attributes()
            .with_title(&self.config.title)
            .with_inner_size(LogicalSize::new(
                f64::from(self.config.width),
                f64::from(self.config.height),
            ));
        if let Some((width, height)) = self.config.minimum_size {
            assert!(
                width > 0 && height > 0,
                "a minimum window size spans a surface"
            );
            attributes = attributes
                .with_min_inner_size(LogicalSize::new(f64::from(width), f64::from(height)));
        }
        let window = Window::new(
            event_loop
                .create_window(attributes)
                .expect("the platform creates a window"),
        );
        self.handler.opened(&window);
        self.last = None;
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window: winit::window::WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            self.close(event_loop);
            return;
        }
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if let WindowEvent::RedrawRequested = event {
            if window.is_minimized() || window.size() == (0, 0) {
                return;
            }
            let real = self.last.map_or(Duration::ZERO, |last| last.elapsed());
            self.last = Some(Instant::now());
            if self.handler.frame(window, real) == Flow::Exit {
                self.close(event_loop);
            } else {
                window.request_redraw();
            }
            return;
        }
        if let Some(event) = translate(&event) {
            let restarts = match event {
                Event::Resized { width, height } => width > 0 && height > 0,
                Event::Focused(true) | Event::ScaleFactorChanged { .. } => {
                    !window.is_minimized() && window.size() != (0, 0)
                }
                _ => false,
            };
            self.handler.event(window, event);
            if restarts {
                window.request_redraw();
            }
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = self.window.take() else {
            return;
        };
        if self.handler.suspended(&window) == Flow::Exit {
            self.handler.closed(&window);
            event_loop.exit();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.take() {
            self.handler.closed(&window);
        }
    }
}

fn translate(event: &WindowEvent) -> Option<Event> {
    match event {
        WindowEvent::Resized(size) => Some(Event::Resized {
            width: size.width,
            height: size.height,
        }),
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => Some(Event::ScaleFactorChanged {
            scale_factor: *scale_factor,
        }),
        WindowEvent::Focused(focused) => Some(Event::Focused(*focused)),
        WindowEvent::KeyboardInput { event, .. } => match event.physical_key {
            PhysicalKey::Code(code) => Some(Event::Key {
                code,
                character: match &event.logical_key {
                    winit::keyboard::Key::Character(text) => text.chars().next(),
                    _ => None,
                },
                pressed: event.state == ElementState::Pressed,
                repeat: event.repeat,
                text: event.text.as_ref().map(ToString::to_string),
            }),
            PhysicalKey::Unidentified(_) => None,
        },
        WindowEvent::MouseInput { state, button, .. } => Some(Event::Button {
            button: *button,
            pressed: *state == ElementState::Pressed,
        }),
        WindowEvent::CursorMoved { position, .. } => Some(Event::CursorMoved {
            x: position.x as f32,
            y: position.y as f32,
        }),
        WindowEvent::CursorLeft { .. } => Some(Event::CursorLeft),
        WindowEvent::MouseWheel { delta, .. } => match delta {
            MouseScrollDelta::LineDelta(x, y) => Some(Event::Wheel {
                lines: [*x, *y],
                pixels: [0.0; 2],
            }),
            MouseScrollDelta::PixelDelta(position) => Some(Event::Wheel {
                lines: [0.0; 2],
                pixels: [position.x as f32, position.y as f32],
            }),
        },
        WindowEvent::ModifiersChanged(modifiers) => Some(Event::Modifiers(modifiers.state())),
        _ => None,
    }
}

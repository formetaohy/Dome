use crate::action::Action;
use dome_gpu::{Gpu, GpuEventCursor, Surface};
use dome_ui::{MemoryClipboard, Ui};
use dome_window::Window;

pub(crate) struct Session {
    pub(crate) context: Gpu,
    pub(crate) events: GpuEventCursor,
    pub(crate) surface: Surface,
    pub(crate) ui: Ui<Action>,
}

impl Session {
    pub(crate) fn new(context: Gpu, surface: Surface, held: Option<Ui<Action>>) -> Self {
        let events = context.events();
        let ui = match held {
            Some(mut ui) => {
                ui.rebind(context.device(), &context.queue(), surface.format());
                ui
            }
            None => ui_of(&context, &surface),
        };
        Self {
            context,
            events,
            surface,
            ui,
        }
    }

    pub(crate) fn resurface(&mut self, window: &Window) -> bool {
        let (width, height) = window.size();
        let surface = self.context.surface(width, height);
        let format = surface.format();
        self.surface = surface;
        if format == self.ui.format() {
            return false;
        }
        self.ui
            .rebind(self.context.device(), &self.context.queue(), format);
        true
    }
}

fn ui_of(context: &Gpu, surface: &Surface) -> Ui<Action> {
    let mut ui = Ui::new(context.device(), &context.queue(), surface.format());
    ui.set_clipboard(Box::new(MemoryClipboard::default()));
    ui
}

use crate::action::Action;
use dome_gpu::{Gpu, GpuEventCursor, Surface};
use dome_ui::{MemoryClipboard, Ui};

pub(crate) struct Session {
    pub(crate) context: Gpu,
    pub(crate) events: GpuEventCursor,
    pub(crate) surface: Surface,
    pub(crate) ui: Ui<Action>,
}

impl Session {
    pub(crate) fn new(context: Gpu, surface: Surface) -> Self {
        let events = context.events();
        let ui = ui_of(&context, &surface);
        Self {
            context,
            events,
            surface,
            ui,
        }
    }

    pub(crate) fn renew(&mut self) {
        self.ui = ui_of(&self.context, &self.surface);
    }

    pub(crate) fn adopts(&self) -> bool {
        self.surface.format() == self.ui.format()
    }
}

fn ui_of(context: &Gpu, surface: &Surface) -> Ui<Action> {
    let mut ui = Ui::new(context.device(), &context.queue(), surface.format());
    ui.set_clipboard(Box::new(MemoryClipboard::default()));
    ui
}

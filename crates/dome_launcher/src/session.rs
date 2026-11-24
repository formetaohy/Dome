use crate::action::Action;
use dome_gpu::{Gpu, Surface};
use dome_ui::{MemoryClipboard, Ui};

pub(crate) struct Session {
    pub(crate) context: Gpu,
    pub(crate) surface: Surface,
    pub(crate) ui: Ui<Action>,
}

impl Session {
    pub(crate) fn new(context: Gpu, surface: Surface) -> Self {
        let mut ui = Ui::new(context.device(), &context.queue(), surface.format());
        ui.set_clipboard(Box::new(MemoryClipboard::default()));
        Self {
            context,
            surface,
            ui,
        }
    }
}

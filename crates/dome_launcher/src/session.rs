use crate::action::Action;
use dome_gpu::{GpuContext, Surface};
use dome_ui::{MemoryClipboard, Ui};

pub(crate) struct Session {
    pub(crate) context: GpuContext,
    pub(crate) surface: Surface,
    pub(crate) ui: Ui<Action>,
}

impl Session {
    pub(crate) fn new(context: GpuContext, surface: Surface) -> Self {
        let mut ui = Ui::new(context.device(), context.queue(), surface.format());
        ui.set_clipboard(Box::new(MemoryClipboard::default()));
        Self {
            context,
            surface,
            ui,
        }
    }
}

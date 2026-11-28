use crate::action::Action;
use crate::preview::Preview;
use dome_gpu::{Gpu, GpuEventCursor, Surface};
use dome_scene::Scene;
use dome_ui::{MemoryClipboard, Ui};

pub(crate) struct Session {
    pub(crate) context: Gpu,
    pub(crate) events: GpuEventCursor,
    pub(crate) surface: Surface,
    pub(crate) preview: Preview,
    pub(crate) ui: Ui<Action>,
}

impl Session {
    pub(crate) fn new(context: Gpu, surface: Surface, preview: Preview, scene: &Scene) -> Self {
        let events = context.events();
        let mut ui = ui_of(&context, &surface);
        preview.register_textures(&mut ui, scene);
        Self {
            context,
            events,
            surface,
            preview,
            ui,
        }
    }

    pub(crate) fn renew(&mut self, scene: &Scene) {
        let mut ui = ui_of(&self.context, &self.surface);
        self.preview.register_textures(&mut ui, scene);
        self.ui = ui;
    }

    pub(crate) fn adopts(&self) -> bool {
        self.surface.format() == self.ui.format()
    }

    pub(crate) fn register_textures(&mut self, scene: &Scene) {
        self.preview.register_textures(&mut self.ui, scene);
    }
}

fn ui_of(context: &Gpu, surface: &Surface) -> Ui<Action> {
    let mut ui = Ui::new(context.device(), &context.queue(), surface.format());
    ui.set_clipboard(Box::new(MemoryClipboard::default()));
    ui
}

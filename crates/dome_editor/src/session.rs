use crate::action::Action;
use crate::preview::Preview;
use dome_gpu::{GpuContext, Surface};
use dome_scene::Scene;
use dome_ui::{MemoryClipboard, Ui};

pub(crate) struct Session {
    pub(crate) context: GpuContext,
    pub(crate) surface: Surface,
    pub(crate) preview: Preview,
    pub(crate) ui: Ui<Action>,
}

impl Session {
    pub(crate) fn new(
        context: GpuContext,
        surface: Surface,
        preview: Preview,
        scene: &Scene,
    ) -> Self {
        let mut ui = Ui::new(context.device(), context.queue(), surface.format());
        ui.set_clipboard(Box::new(MemoryClipboard::default()));
        preview.register_textures(&mut ui, scene);
        Self {
            context,
            surface,
            preview,
            ui,
        }
    }

    pub(crate) fn register_textures(&mut self, scene: &Scene) {
        self.preview.register_textures(&mut self.ui, scene);
    }
}

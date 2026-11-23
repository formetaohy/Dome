use crate::message::Message;
use crate::preview::Preview;
use dome_gpu_device::{GpuContext, Surface};
use dome_scene::Scene;
use dome_ui::Ui;

pub(crate) struct Session {
    pub(crate) context: GpuContext,
    pub(crate) surface: Surface,
    pub(crate) preview: Preview,
    pub(crate) ui: Ui<Message>,
}

impl Session {
    pub(crate) fn new(
        context: GpuContext,
        surface: Surface,
        preview: Preview,
        scene: &Scene,
    ) -> Self {
        let mut ui = Ui::new(context.device(), context.queue(), surface.format());
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

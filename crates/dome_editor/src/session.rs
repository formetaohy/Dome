use crate::action::Action;
use crate::preview::Preview;
use dome_gpu::{Gpu, GpuEventCursor, Surface};
use dome_scene::Scene;
use dome_ui::{MemoryClipboard, Ui};
use dome_window::Window;

pub(crate) struct Session {
    pub(crate) context: Gpu,
    pub(crate) events: GpuEventCursor,
    pub(crate) surface: Surface,
    pub(crate) preview: Preview,
    pub(crate) ui: Ui<Action>,
}

impl Session {
    pub(crate) fn new(
        context: Gpu,
        surface: Surface,
        preview: Preview,
        scene: &Scene,
        held: Option<Ui<Action>>,
    ) -> Self {
        let events = context.events();
        let mut ui = match held {
            Some(mut ui) => {
                ui.rebind(context.device(), &context.queue(), surface.format());
                ui
            }
            None => ui_of(&context, &surface),
        };
        preview.register_textures(&mut ui, scene);
        Self {
            context,
            events,
            surface,
            preview,
            ui,
        }
    }

    pub(crate) fn resurface(&mut self, window: &Window, scene: &Scene) -> bool {
        let (width, height) = window.size();
        let surface = self.context.surface(width, height);
        let format = surface.format();
        self.surface = surface;
        if format == self.ui.format() {
            return false;
        }
        self.ui
            .rebind(self.context.device(), &self.context.queue(), format);
        self.preview.register_textures(&mut self.ui, scene);
        true
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

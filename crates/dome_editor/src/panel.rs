#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    Scene,
    Library,
    Inspector,
    Settings,
}

impl Pane {
    pub fn name(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Library => "library",
            Self::Inspector => "inspector",
            Self::Settings => "settings",
        }
    }

    pub fn on_left(self) -> bool {
        matches!(self, Self::Scene | Self::Library)
    }

    pub fn width(self) -> f32 {
        match self {
            Self::Scene | Self::Library => 260.0,
            Self::Inspector | Self::Settings => 320.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Panels {
    pub left: Option<Pane>,
    pub right: Option<Pane>,
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            left: Some(Pane::Scene),
            right: Some(Pane::Inspector),
        }
    }
}

impl Panels {
    pub fn toggle(&mut self, pane: Pane) {
        if pane.on_left() {
            self.left = (self.left != Some(pane)).then_some(pane);
        } else {
            self.right = (self.right != Some(pane)).then_some(pane);
        }
    }

    pub fn open(&self, pane: Pane) -> bool {
        if pane.on_left() {
            self.left == Some(pane)
        } else {
            self.right == Some(pane)
        }
    }
}

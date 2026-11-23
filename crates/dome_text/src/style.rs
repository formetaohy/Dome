use crate::font::GlyphStyle;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Align {
    Start,
    Center,
    End,
    Baseline,
}

impl Align {
    pub(crate) fn factor(self) -> f32 {
        match self {
            Self::Start | Self::Baseline => 0.0,
            Self::Center => 0.5,
            Self::End => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeStyle {
    pub size: f32,
    pub weight: f32,
    pub width: f32,
    pub grade: f32,
    pub tracking: f32,
    pub leading: f32,
    pub monospace: bool,
}

impl TypeStyle {
    pub const fn new(size: f32, weight: f32, leading: f32) -> Self {
        Self {
            size,
            weight,
            width: 100.0,
            grade: 0.0,
            tracking: 0.0,
            leading,
            monospace: false,
        }
    }

    pub const fn with_tracking(mut self, tracking: f32) -> Self {
        self.tracking = tracking;
        self
    }

    pub const fn glyph(self, hint: bool) -> GlyphStyle {
        GlyphStyle::new(self.size, self.weight)
            .with_axes(self.width, self.grade)
            .hinted(hint)
    }

    pub const fn medium(self) -> Self {
        Self {
            weight: 500.0,
            ..self
        }
    }

    pub const fn semibold(self) -> Self {
        Self {
            weight: 600.0,
            ..self
        }
    }

    pub const fn bold(self) -> Self {
        Self {
            weight: 700.0,
            ..self
        }
    }
}

impl std::hash::Hash for TypeStyle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.size.to_bits().hash(state);
        self.weight.to_bits().hash(state);
        self.width.to_bits().hash(state);
        self.grade.to_bits().hash(state);
        self.tracking.to_bits().hash(state);
        self.leading.to_bits().hash(state);
        self.monospace.hash(state);
    }
}

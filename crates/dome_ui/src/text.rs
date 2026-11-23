use std::sync::Arc;

use dome_math::Vec2;

use crate::color::Color;

pub use dome_text::{Align, Text, TextLayout, TypeStyle};

#[derive(Clone, Copy, Debug, PartialEq, Hash)]
pub struct TextStyle {
    pub font: TypeStyle,
    pub color: Color,
    pub align: Align,
    pub wrap: bool,
    pub truncate: bool,
}

impl TextStyle {
    pub fn new(font: TypeStyle, color: Color) -> Self {
        Self {
            font,
            color,
            align: Align::Start,
            wrap: false,
            truncate: false,
        }
    }

    pub fn aligned(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn wrapping(mut self) -> Self {
        self.wrap = true;
        self
    }

    pub fn truncating(mut self) -> Self {
        self.truncate = true;
        self
    }

    pub(crate) fn bounded(self) -> bool {
        self.wrap || self.truncate || !matches!(self.align, Align::Start | Align::Baseline)
    }
}

pub(crate) fn layout(
    text: &mut Text,
    value: &str,
    style: TextStyle,
    max_width: f32,
) -> Arc<TextLayout> {
    text.layout(
        value,
        style.font,
        max_width,
        style.wrap,
        style.truncate,
        style.align,
    )
}

pub(crate) fn measure(text: &mut Text, value: &str, style: TextStyle) -> Vec2 {
    text.layout(value, style.font, 0.0, false, false, style.align)
        .size
}

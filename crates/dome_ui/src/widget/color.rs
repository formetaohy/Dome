use dome_math::Vec2;

use crate::color::Color;
use crate::content::{ColorPickerContent, Content};
use crate::element::{Element, Layout, Length, Size};
use crate::geometry::{Corners, Rect};
use crate::motion::Motion;
use crate::node::{Node, Runtime};

use crate::style::{Border, Fill};

use super::PaintInfo;

const SQUARE: f32 = 184.0;
const GAP: f32 = 10.0;
const HUE_HEIGHT: f32 = 16.0;
const CELLS: usize = 20;

pub fn color_picker<'a, A>(color: [f32; 3]) -> Element<'a, A> {
    Element::new(Layout {
        size: Size::new(
            Length::Fixed(SQUARE),
            Length::Fixed(SQUARE + GAP + HUE_HEIGHT),
        ),
        ..Layout::row()
    })
    .with_content(Content::ColorPicker(ColorPickerContent { color }))
}

pub(super) fn measure() -> Vec2 {
    Vec2::new(SQUARE, SQUARE + GAP + HUE_HEIGHT)
}

pub(crate) fn value<A>(node: &Node<A>) -> Option<(Motion, (f32, f32))> {
    let Content::ColorPicker(_) = &node.content else {
        return None;
    };
    Some((Motion::INSTANT, (node.runtime.value.value, 0.0)))
}

pub(super) fn paint(
    content: &ColorPickerContent,
    runtime: &Runtime,
    painter: &mut crate::paint::Painter,
    info: &PaintInfo<'_>,
) {
    let rect = info.rect;
    let square = Rect::from_min_size(rect.min, Vec2::splat(SQUARE));
    let (derived_hue, saturation, value) = rgb_to_hsv(content.color);
    let hue = if runtime.seeded {
        runtime.value.value
    } else {
        derived_hue
    };
    let cell = SQUARE / CELLS as f32;
    let square_corners = Corners::all(info.theme.metrics.corner_md);
    let held_clip = painter.clip();
    painter.push_clip(square, square_corners);
    for row in 0..CELLS {
        for column in 0..CELLS {
            let saturation = (column as f32 + 0.5) / CELLS as f32;
            let value = 1.0 - (row as f32 + 0.5) / CELLS as f32;
            painter.rect(
                Rect::from_min_size(
                    square.min + Vec2::new(column as f32 * cell, row as f32 * cell),
                    Vec2::splat(cell + 0.6),
                ),
                Corners::ZERO,
                Color::from_rgb_f32(hsv_to_rgb(hue, saturation, value)),
            );
        }
    }
    painter.pop_clip(held_clip);
    painter.outlined(
        square,
        square_corners,
        Fill::None,
        Border {
            width: 1.0,
            color: info.theme.palette.outline_variant.with_alpha(0.55),
        },
    );
    let marker = Vec2::new(
        (square.min.x + SQUARE * saturation).clamp(square.min.x + 6.0, square.max.x - 6.0),
        (square.min.y + SQUARE * (1.0 - value)).clamp(square.min.y + 6.0, square.max.y - 6.0),
    );
    painter.ring(
        marker,
        7.8,
        1.2,
        0.0,
        std::f32::consts::TAU,
        Color::black_alpha(0.45),
    );
    painter.ring(marker, 6.0, 2.0, 0.0, std::f32::consts::TAU, Color::WHITE);
    let strip = hue_rect(rect);
    let stops = 32;
    let step = strip.width() / stops as f32;
    let held_clip = painter.clip();
    painter.push_clip(strip, Corners::capsule());
    for index in 0..stops {
        painter.rect(
            Rect::from_min_size(
                Vec2::new(strip.min.x + index as f32 * step, strip.min.y),
                Vec2::new(step + 0.6, strip.height()),
            ),
            Corners::ZERO,
            Color::from_rgb_f32(hsv_to_rgb(index as f32 / (stops - 1) as f32, 1.0, 1.0)),
        );
    }
    painter.pop_clip(held_clip);
    let hue_x = (strip.min.x + strip.width() * hue).clamp(strip.min.x + 5.0, strip.max.x - 5.0);
    let thumb = Rect::from_center_size(Vec2::new(hue_x, strip.center().y), Vec2::new(5.0, 22.0));
    painter.rect(
        thumb.expand(1.0),
        Corners::capsule(),
        Color::black_alpha(0.45),
    );
    painter.rect(thumb, Corners::capsule(), Color::WHITE);
}

fn hue_rect(rect: Rect) -> Rect {
    Rect::from_min_size(
        Vec2::new(rect.min.x, rect.min.y + SQUARE + GAP),
        Vec2::new(SQUARE, HUE_HEIGHT),
    )
}

pub(crate) fn pick(
    rect: Rect,
    color: [f32; 3],
    stored: f32,
    seeded: bool,
    pointer: Vec2,
) -> Option<([f32; 3], f32)> {
    let (derived, saturation, value) = rgb_to_hsv(color);
    let hue = if seeded { stored } else { derived };
    let square = Rect::from_min_size(rect.min, Vec2::splat(SQUARE));
    if square.contains(pointer) {
        let s = ((pointer.x - square.min.x) / SQUARE).clamp(0.0, 1.0);
        let v = (1.0 - (pointer.y - square.min.y) / SQUARE).clamp(0.0, 1.0);
        return Some((hsv_to_rgb(hue, s, v), hue));
    }
    let strip = hue_rect(rect);
    if strip.contains(pointer) {
        let hue = ((pointer.x - strip.min.x) / strip.width()).clamp(0.0, 1.0);
        return Some((hsv_to_rgb(hue, saturation, value), hue));
    }
    None
}

pub(crate) fn rgb_to_hsv(color: [f32; 3]) -> (f32, f32, f32) {
    let maximum = color[0].max(color[1]).max(color[2]);
    let minimum = color[0].min(color[1]).min(color[2]);
    let delta = maximum - minimum;
    let value = maximum;
    let saturation = if maximum <= 0.0 { 0.0 } else { delta / maximum };
    let hue = if delta <= 0.0 {
        0.0
    } else if maximum == color[0] {
        ((color[1] - color[2]) / delta).rem_euclid(6.0) / 6.0
    } else if maximum == color[1] {
        ((color[2] - color[0]) / delta + 2.0) / 6.0
    } else {
        ((color[0] - color[1]) / delta + 4.0) / 6.0
    };
    (hue, saturation, value)
}

pub(crate) fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> [f32; 3] {
    let hue = (hue.rem_euclid(1.0)) * 6.0;
    let index = hue.floor() as i32;
    let fraction = hue - index as f32;
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * fraction);
    let t = value * (1.0 - saturation * (1.0 - fraction));
    match index {
        0 => [value, t, p],
        1 => [q, value, p],
        2 => [p, value, t],
        3 => [p, q, value],
        4 => [t, p, value],
        _ => [value, p, q],
    }
}

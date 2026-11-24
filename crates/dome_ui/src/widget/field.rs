use dome_math::Vec2;

use crate::content::{Content, FieldContent};
use crate::edit::Edit;
use crate::element::{Element, Layout, Length, Size};
use crate::geometry::{Corners, Rect};
use crate::grapheme::{self, Boundary};
use crate::node::Runtime;
use crate::paint::Painter;
use crate::style::{Border, State};
use crate::text::TextStyle;
use crate::theme::Role;

use super::PaintInfo;

const FIELD_PAD: f32 = 10.0;

pub fn text_field<'a, A>(
    value: &str,
    style: TextStyle,
    width: f32,
    multiline: bool,
) -> Element<'a, A> {
    Element::new(Layout {
        size: Size::new(
            Length::Fixed(width),
            Length::Fixed(if multiline { 96.0 } else { 30.0 }),
        ),
        ..Layout::row()
    })
    .with_content(Content::Field(FieldContent {
        value: value.to_string(),
        style,
        multiline,
        radius: 10.0,
        editing: false,
        select_all: false,
        error: false,
    }))
}

pub(super) fn measure_field(field: &FieldContent, layout: Layout, available: Vec2) -> Vec2 {
    let width = match layout.size.width {
        Length::Auto | Length::Fill => available.x,
        Length::Fixed(value) => value,
        Length::Minimum(minimum) => minimum.max(available.x),
        Length::Maximum(maximum) => available.x.min(maximum),
    };
    let height = match layout.size.height {
        Length::Auto | Length::Fill => available.y,
        other => other.resolve(available.y),
    };
    let _ = field;
    Vec2::new(width, height)
}

pub(crate) fn caret_at(
    edit: &Edit,
    preedit: Option<&str>,
    style: TextStyle,
    multiline: bool,
    rect: Rect,
    pointer: Vec2,
    layout: &mut crate::text::Text,
) -> Boundary {
    let text = edit.text();
    let caret = edit.caret();
    let inner = inner(rect);
    let preedit_width = preedit.map_or(0.0, |composed| width(composed, style, layout));
    if !multiline {
        let caret_x = width(&text[..caret.byte()], style, layout);
        let shift = shift(caret_x, preedit_width, inner.width());
        return column(text, pointer.x - inner.min.x + shift, style, layout);
    }
    let line_height = style.font.leading.max(style.font.size * 1.28);
    let lines = lines(text);
    let caret_line = line_at(&lines, caret);
    let row = ((pointer.y - inner.min.y) / line_height).floor().max(0.0) as usize
        + scroll_rows(caret_line, inner.height(), line_height);
    let (_, line_x) = caret_x_of(text, &lines, caret, style, layout);
    let shift = shift(line_x, preedit_width, inner.width());
    let x = pointer.x - inner.min.x + shift;
    let Some(line) = lines.get(row) else {
        return Boundary::end(text);
    };
    let relative = column(&text[line.start..line.end], x, style, layout);
    Boundary::nearest(text, line.start + relative.byte())
}

fn inner(rect: Rect) -> Rect {
    rect.shrink2(Vec2::new(FIELD_PAD, 4.0))
}

fn scroll_rows(line: usize, height: f32, line_height: f32) -> usize {
    let visible = (height / line_height).floor().max(1.0);
    ((line as f32 + 1.0) - visible).max(0.0).ceil() as usize
}

fn shift(caret_x: f32, preedit_width: f32, width: f32) -> f32 {
    (caret_x + preedit_width + 2.0 - width).max(0.0)
}

#[derive(Clone, Copy, Debug)]
struct Line {
    start: usize,
    end: usize,
}

fn lines(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (offset, character) in text.char_indices() {
        if character == '\n' {
            lines.push(Line { start, end: offset });
            start = offset + 1;
        }
    }
    lines.push(Line {
        start,
        end: text.len(),
    });
    lines
}

fn line_at(lines: &[Line], caret: Boundary) -> usize {
    lines
        .iter()
        .position(|line| caret.byte() <= line.end)
        .expect("a caret sits inside a line")
}

fn line_prefix<'a>(text: &'a str, lines: &[Line], line: usize, caret: Boundary) -> &'a str {
    let start = lines[line].start;
    &text[start..caret.byte().clamp(start, lines[line].end)]
}

fn caret_x_of(
    text: &str,
    lines: &[Line],
    caret: Boundary,
    style: TextStyle,
    layout: &mut crate::text::Text,
) -> (usize, f32) {
    let line = line_at(lines, caret);
    (
        line,
        width(line_prefix(text, lines, line, caret), style, layout),
    )
}

fn caret_xy(
    painter: &mut Painter,
    style: TextStyle,
    text: &str,
    lines: &[Line],
    caret: Boundary,
) -> (usize, f32) {
    let line = line_at(lines, caret);
    (
        line,
        text_width(painter, style, line_prefix(text, lines, line, caret)),
    )
}

fn column(text: &str, x: f32, style: TextStyle, layout: &mut crate::text::Text) -> Boundary {
    let boundaries = grapheme::boundaries(text);
    let (mut low, mut high) = (0usize, boundaries.len() - 1);
    while low < high {
        let middle = (low + high).div_ceil(2);
        if width(&text[..boundaries[middle].byte()], style, layout) <= x {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let before = width(&text[..boundaries[low].byte()], style, layout);
    let Some(after) = boundaries.get(low + 1) else {
        return boundaries[low];
    };
    if (width(&text[..after.byte()], style, layout) - x).abs() < (before - x).abs() {
        *after
    } else {
        boundaries[low]
    }
}

fn width(text: &str, style: TextStyle, layout: &mut crate::text::Text) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    layout
        .layout(
            text,
            style.font,
            0.0,
            false,
            false,
            crate::text::Align::Start,
        )
        .size
        .x
}

fn text_width(painter: &mut Painter, style: TextStyle, text: &str) -> f32 {
    if text.is_empty() {
        0.0
    } else {
        painter.measure(text, style).x
    }
}

pub(super) fn paint_field(
    field: &FieldContent,
    runtime: &Runtime,
    painter: &mut Painter,
    info: &PaintInfo<'_>,
) -> Option<Rect> {
    let rect = info.rect;
    let radius = field.radius;
    let state = if !info.enabled {
        State::Disabled
    } else if runtime.edit.is_some() {
        State::Selected
    } else if info.press > 0.5 {
        State::Pressed
    } else if info.hover > 0.5 {
        State::Hover
    } else {
        State::Rest
    };
    let look = info.theme.surface(Role::Field, state);
    let palette = info.theme.palette;
    let inner = rect.shrink2(Vec2::new(FIELD_PAD, 4.0));
    let border = if field.error {
        Border {
            width: look.border.width.max(1.5),
            color: palette.error,
        }
    } else {
        look.border
    };
    painter.outlined(rect, Corners::all(radius), look.fill, border);
    let clip = painter.clip();
    painter.push_clip(inner, Corners::ZERO);
    let style = field.style;
    if let Some(edit) = &runtime.edit {
        if field.multiline {
            let text = edit.text();
            let caret = edit.caret();
            let anchor = edit.anchor();
            let preedit = info.preedit.filter(|composed| !composed.is_empty());
            let preedit_width = preedit.map_or(0.0, |composed| painter.measure(composed, style).x);
            let line_height = style.font.leading.max(style.font.size * 1.28);
            let lines = lines(text);
            let (caret_line, caret_x) = caret_xy(painter, style, text, &lines, caret);
            let offset = shift(caret_x, preedit_width, inner.width());
            let rows = scroll_rows(caret_line, inner.height(), line_height);
            let top = |index: usize| inner.min.y + (index as f32 - rows as f32) * line_height;
            let selection = anchor.min(caret).byte()..anchor.max(caret).byte();
            if selection.start != selection.end {
                for (index, line) in lines.iter().enumerate() {
                    let low = selection.start.max(line.start);
                    let high = selection.end.min(line.end);
                    if low < high {
                        let left = text_width(painter, style, &text[line.start..low]);
                        let right = text_width(painter, style, &text[line.start..high]);
                        painter.rect(
                            Rect::from_min_size(
                                Vec2::new(inner.min.x + left - offset, top(index)),
                                Vec2::new(right - left, line_height),
                            ),
                            Corners::ZERO,
                            palette.primary.with_alpha(0.28),
                        );
                    }
                }
            }
            for (index, line) in lines.iter().enumerate() {
                let layout = painter.layout(&text[line.start..line.end], style, 0.0);
                painter.text(
                    &layout,
                    Vec2::new(inner.min.x - offset, top(index)),
                    style.color,
                );
            }
            if let Some(composed) = preedit {
                let layout = painter.layout(composed, style, 0.0);
                let origin = Vec2::new(inner.min.x + caret_x - offset, top(caret_line));
                painter.text(&layout, origin, style.color);
                painter.rect(
                    Rect::from_min_size(
                        Vec2::new(origin.x, origin.y + layout.size.y - 1.0),
                        Vec2::new(layout.size.x, 1.0),
                    ),
                    Corners::ZERO,
                    palette.primary.with_alpha(0.8),
                );
            }
            let caret_rect = Rect::from_min_size(
                Vec2::new(
                    inner.min.x + caret_x + preedit_width - offset,
                    top(caret_line) + 1.0,
                ),
                Vec2::new(1.6, (line_height - 2.0).max(1.0)),
            );
            if edit.caret_visible(info.time) {
                painter.rect(caret_rect, Corners::ZERO, palette.primary);
            }
            painter.pop_clip(clip);
            return Some(caret_rect);
        }
        let caret_rect = paint_single_line_edit(
            edit,
            inner,
            style,
            palette.primary,
            info.preedit,
            info.time,
            painter,
        );
        painter.pop_clip(clip);
        Some(caret_rect)
    } else {
        let text = &field.value;
        let layout = painter.layout(text, style, inner.width());
        let origin = Vec2::new(
            inner.min.x,
            inner.min.y + (inner.height() - layout.size.y) * 0.5,
        );
        painter.text(&layout, origin, style.color);
        painter.pop_clip(clip);
        None
    }
}

pub(crate) fn paint_single_line_edit(
    edit: &Edit,
    inner: Rect,
    style: TextStyle,
    accent: crate::color::Color,
    preedit: Option<&str>,
    time: f32,
    painter: &mut Painter,
) -> Rect {
    let text = edit.text();
    let caret = edit.caret().byte();
    let anchor = edit.anchor().byte();
    let caret_x = text_width(painter, style, &text[..caret]);
    let preedit = preedit.filter(|composed| !composed.is_empty());
    let preedit_width = preedit.map_or(0.0, |composed| painter.measure(composed, style).x);
    let offset = shift(caret_x, preedit_width, inner.width());
    let selection = anchor.min(caret)..anchor.max(caret);
    if selection.start != selection.end {
        let left = text_width(painter, style, &text[..selection.start]);
        let right = text_width(painter, style, &text[..selection.end]);
        painter.rect(
            Rect::from_min_size(
                Vec2::new(inner.min.x + left - offset, inner.min.y + 2.0),
                Vec2::new(right - left, inner.height() - 4.0),
            ),
            Corners::ZERO,
            accent.with_alpha(0.28),
        );
    }
    let layout = painter.layout(text, style, 0.0);
    let origin = Vec2::new(
        inner.min.x - offset,
        inner.min.y + (inner.height() - layout.size.y) * 0.5,
    );
    painter.text(&layout, origin, style.color);
    if let Some(composed) = preedit {
        let composed = painter.layout(composed, style, 0.0);
        let origin = Vec2::new(
            inner.min.x + caret_x - offset,
            inner.min.y + (inner.height() - composed.size.y) * 0.5,
        );
        painter.text(&composed, origin, style.color);
        painter.rect(
            Rect::from_min_size(
                Vec2::new(origin.x, origin.y + composed.size.y - 1.0),
                Vec2::new(composed.size.x, 1.0),
            ),
            Corners::ZERO,
            accent.with_alpha(0.8),
        );
    }
    let caret_rect = Rect::from_min_size(
        Vec2::new(
            inner.min.x + caret_x + preedit_width - offset,
            inner.min.y + 2.0,
        ),
        Vec2::new(1.6, inner.height() - 4.0),
    );
    if edit.caret_visible(time) {
        painter.rect(caret_rect, Corners::ZERO, accent);
    }
    caret_rect
}

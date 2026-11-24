use std::collections::{BTreeMap, HashMap};
use std::mem::size_of;
use std::sync::Arc;

use dome_math::Vec2;

use crate::atlas::{Atlas, GlyphRequest, GlyphSlot, Slot};
use crate::font::{Fonts, IconGlyph};
use crate::style::{Align, TypeStyle};

const BUDGET: usize = 8 << 20;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub face: u16,
    pub id: u16,
    pub x: f32,
    pub baseline: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub byte: u32,
    pub x: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedLine {
    pub start: u32,
    pub end: u32,
    pub x: f32,
}

#[derive(Clone, Debug)]
pub struct TextLayout {
    pub text: Arc<str>,
    pub style: TypeStyle,
    pub glyphs: Vec<PlacedGlyph>,
    pub lines: Vec<PlacedLine>,
    pub stops: Vec<Stop>,
    pub size: Vec2,
    pub baseline: f32,
    pub truncated: bool,
}

impl TextLayout {
    pub fn line(&self, byte: usize) -> usize {
        self.lines
            .iter()
            .rposition(|line| line.start as usize <= byte)
            .unwrap_or(0)
    }

    pub fn caret(&self, line: usize, byte: usize) -> f32 {
        let held = self.lines[line];
        let at = self
            .stops
            .partition_point(|stop| (stop.byte as usize) <= byte);
        self.stops[..at]
            .iter()
            .rev()
            .find(|stop| stop.byte >= held.start)
            .map_or(held.x, |stop| stop.x)
    }

    pub fn column(&self, line: usize, x: f32) -> usize {
        let held = self.lines[line];
        let low = self.stops.partition_point(|stop| stop.byte < held.start);
        let high = self.stops.partition_point(|stop| stop.byte <= held.end);
        let stops = &self.stops[low..high];
        let at = stops.partition_point(|stop| stop.x < x);
        match (at.checked_sub(1).map(|index| &stops[index]), stops.get(at)) {
            (Some(before), Some(after)) => {
                if (after.x - x).abs() < (before.x - x).abs() {
                    after.byte as usize
                } else {
                    before.byte as usize
                }
            }
            (Some(before), None) => before.byte as usize,
            (None, Some(after)) => after.byte as usize,
            (None, None) => held.start as usize,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Spec {
    size: u32,
    weight: u32,
    stretch: u16,
    grade: i16,
    tracking: i16,
    leading: u32,
    width: u32,
    pixels_per_point: u32,
    align: Align,
    monospace: bool,
    wrap: bool,
    truncate: bool,
}

struct Entry {
    spec: Spec,
    stamp: u64,
    bytes: usize,
    layout: Arc<TextLayout>,
}

struct Row {
    glyphs: Vec<PlacedGlyph>,
    stops: Vec<Stop>,
    width: f32,
    start: u32,
    end: u32,
}

pub struct Text {
    fonts: Fonts,
    atlas: Atlas,
    entries: HashMap<Arc<str>, Vec<Entry>>,
    order: BTreeMap<u64, (Arc<str>, Spec)>,
    bytes: usize,
    stamp: u64,
    pixels_per_point: f32,
}

impl Text {
    pub fn new() -> Self {
        Self {
            fonts: Fonts::system(),
            atlas: Atlas::new(),
            entries: HashMap::new(),
            order: BTreeMap::new(),
            bytes: 0,
            stamp: 0,
            pixels_per_point: 1.0,
        }
    }

    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    pub fn atlas_mut(&mut self) -> &mut Atlas {
        &mut self.atlas
    }

    pub fn begin(&mut self, pixels_per_point: f32) -> bool {
        self.pixels_per_point = pixels_per_point;
        self.atlas.advance();
        self.atlas.take_recycled()
    }

    pub fn set_pixels_per_point(&mut self, pixels_per_point: f32) {
        self.pixels_per_point = pixels_per_point;
    }

    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }

    pub fn description(&self) -> &str {
        self.fonts.description()
    }

    pub fn layout(
        &mut self,
        text: &str,
        style: TypeStyle,
        max_width: f32,
        wrap: bool,
        truncate: bool,
        align: Align,
    ) -> Arc<TextLayout> {
        let spec = Spec {
            size: (style.size * 4.0).round() as u32,
            weight: (style.weight * 4.0).round() as u32,
            stretch: (style.width * 4.0).round() as u16,
            grade: (style.grade * 4.0).round() as i16,
            tracking: (style.tracking * 64.0).round() as i16,
            leading: (style.leading * 4.0).round() as u32,
            width: (max_width.max(0.0) * 4.0).round() as u32,
            pixels_per_point: (self.pixels_per_point * 100.0).round() as u32,
            align,
            monospace: style.monospace,
            wrap,
            truncate,
        };
        let stamp = self.stamp + 1;
        self.stamp = stamp;
        if let Some(entries) = self.entries.get_mut(text)
            && let Some(entry) = entries.iter_mut().find(|entry| entry.spec == spec)
        {
            let held = entry.stamp;
            entry.stamp = stamp;
            if let Some(key) = self.order.remove(&held) {
                self.order.insert(stamp, key);
            }
            return Arc::clone(&entry.layout);
        }
        let layout = Arc::new(self.build(text, style, max_width, wrap, truncate, align));
        let bytes = size_of::<TextLayout>()
            + size_of::<Entry>()
            + layout.text.len()
            + layout.glyphs.len() * size_of::<PlacedGlyph>()
            + layout.lines.len() * size_of::<PlacedLine>()
            + layout.stops.len() * size_of::<Stop>();
        let shared = self
            .entries
            .get_key_value(text)
            .map_or_else(|| Arc::from(text), |(held, _)| Arc::clone(held));
        self.entries.entry(shared.clone()).or_default().push(Entry {
            spec,
            stamp,
            bytes,
            layout: Arc::clone(&layout),
        });
        self.order.insert(stamp, (shared, spec));
        self.bytes += bytes;
        self.trim();
        layout
    }

    fn trim(&mut self) {
        while self.bytes > BUDGET {
            let Some((stamp, (text, _))) = self.order.pop_first() else {
                return;
            };
            let mut dropped = 0;
            if let Some(entries) = self.entries.get_mut(&*text) {
                if let Some(index) = entries.iter().position(|entry| entry.stamp == stamp) {
                    dropped = entries.remove(index).bytes;
                }
                if entries.is_empty() {
                    self.entries.remove(&*text);
                }
            }
            self.bytes = self.bytes.saturating_sub(dropped);
        }
    }

    fn build(
        &mut self,
        text: &str,
        style: TypeStyle,
        max_width: f32,
        wrap: bool,
        truncate: bool,
        align: Align,
    ) -> TextLayout {
        let cjk = text.chars().any(is_cjk);
        let leading = if cjk {
            style.leading.max(style.size * 1.28)
        } else {
            style.leading.max(style.size * 1.05)
        };
        let ascent = style.size * if cjk { 0.88 } else { 0.78 };
        let descent = style.size * if cjk { 0.18 } else { 0.22 };
        let baseline_offset = 0.5 * (leading - (ascent + descent)) + ascent;
        let bounded = max_width > 0.0;
        let glyph_style = style.glyph(false);
        let mut rows: Vec<Row> = Vec::new();
        let mut glyphs: Vec<PlacedGlyph> = Vec::new();
        let mut stops: Vec<Stop> = Vec::new();
        let mut width = 0.0f32;
        let mut start = 0usize;
        let mut truncated = false;

        let flush = |rows: &mut Vec<Row>,
                     glyphs: &mut Vec<PlacedGlyph>,
                     stops: &mut Vec<Stop>,
                     width: &mut f32,
                     start: usize,
                     end: usize| {
            stops.push(Stop {
                byte: end as u32,
                x: *width,
            });
            rows.push(Row {
                glyphs: std::mem::take(glyphs),
                stops: std::mem::take(stops),
                width: *width,
                start: start as u32,
                end: end as u32,
            });
            *width = 0.0;
        };

        let mut offset = 0usize;
        'paragraphs: for paragraph in text.split('\n') {
            let mut cursor = offset;
            for token in tokenize(paragraph) {
                let token_start = cursor;
                cursor += token.len();
                let whitespace = token.starts_with(' ');
                let runs = if style.monospace {
                    self.fonts.monospace_runs(token)
                } else {
                    self.fonts.runs(token)
                };
                let mut shaped = Vec::with_capacity(runs.len());
                let mut token_width = 0.0f32;
                let mut run_start = 0usize;
                for run in runs {
                    let held = self.fonts.shape(run.face, run.text, glyph_style, 1.0);
                    token_width += held.width + style.tracking * held.glyphs.len() as f32;
                    shaped.push((run.face as u16, token_start + run_start, held));
                    run_start += run.text.len();
                }
                if wrap
                    && bounded
                    && !whitespace
                    && width > 0.0
                    && width + token_width > max_width + 0.01
                {
                    flush(
                        &mut rows,
                        &mut glyphs,
                        &mut stops,
                        &mut width,
                        start,
                        token_start,
                    );
                    start = token_start;
                }
                if truncate && !wrap && bounded && width + token_width > max_width {
                    let face = if style.monospace {
                        self.fonts.face_for_mono('\u{2026}')
                    } else {
                        self.fonts.face_for('\u{2026}')
                    };
                    let ellipsis = self.fonts.shape(face, "\u{2026}", glyph_style, 1.0);
                    let room = (max_width - width - ellipsis.width).max(0.0);
                    let mut base = 0.0f32;
                    let mut used = 0.0f32;
                    'prefix: for (held, held_start, run) in &shaped {
                        for (index, glyph) in run.glyphs.iter().enumerate() {
                            let advance = run
                                .glyphs
                                .get(index + 1)
                                .map_or(run.width - glyph.x, |next| next.x - glyph.x);
                            let x = base + glyph.x + style.tracking * index as f32;
                            if x + advance > room {
                                break 'prefix;
                            }
                            glyphs.push(PlacedGlyph {
                                face: *held,
                                id: glyph.id,
                                x: width + x,
                                baseline: 0.0,
                            });
                            stops.push(Stop {
                                byte: (*held_start + glyph.byte as usize) as u32,
                                x: width + x,
                            });
                            used = x + advance;
                        }
                        base += run.width + style.tracking * run.glyphs.len() as f32;
                    }
                    width += used;
                    if width + ellipsis.width <= max_width {
                        for glyph in &ellipsis.glyphs {
                            glyphs.push(PlacedGlyph {
                                face: face as u16,
                                id: glyph.id,
                                x: width + glyph.x,
                                baseline: 0.0,
                            });
                        }
                        width += ellipsis.width;
                    }
                    truncated = true;
                    flush(
                        &mut rows,
                        &mut glyphs,
                        &mut stops,
                        &mut width,
                        start,
                        cursor,
                    );
                    break 'paragraphs;
                }
                for (held, held_start, run) in &shaped {
                    let base = width;
                    for (index, glyph) in run.glyphs.iter().enumerate() {
                        let x = base + glyph.x + style.tracking * index as f32;
                        glyphs.push(PlacedGlyph {
                            face: *held,
                            id: glyph.id,
                            x,
                            baseline: 0.0,
                        });
                        stops.push(Stop {
                            byte: (*held_start + glyph.byte as usize) as u32,
                            x,
                        });
                    }
                    width += run.width + style.tracking * run.glyphs.len() as f32;
                }
            }
            if !glyphs.is_empty() {
                flush(
                    &mut rows,
                    &mut glyphs,
                    &mut stops,
                    &mut width,
                    start,
                    cursor,
                );
            } else {
                rows.push(Row {
                    glyphs: Vec::new(),
                    stops: Vec::new(),
                    width: 0.0,
                    start: cursor as u32,
                    end: cursor as u32,
                });
            }
            offset = cursor + 1;
            start = offset;
        }
        if rows.is_empty() {
            rows.push(Row {
                glyphs: Vec::new(),
                stops: Vec::new(),
                width: 0.0,
                start: text.len() as u32,
                end: text.len() as u32,
            });
        }

        let widest = rows.iter().map(|row| row.width).fold(0.0f32, f32::max);
        let reference = if bounded { max_width } else { widest };
        let mut placed = Vec::new();
        let mut lines = Vec::with_capacity(rows.len());
        let mut held_stops = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            let shift = (reference - row.width) * align.factor();
            let baseline = baseline_offset + index as f32 * leading;
            for glyph in &row.glyphs {
                placed.push(PlacedGlyph {
                    x: glyph.x + shift,
                    baseline,
                    ..*glyph
                });
            }
            for stop in &row.stops {
                held_stops.push(Stop {
                    byte: stop.byte,
                    x: stop.x + shift,
                });
            }
            lines.push(PlacedLine {
                start: row.start,
                end: row.end,
                x: shift,
            });
        }
        let size = Vec2::new(widest, leading * rows.len() as f32);
        TextLayout {
            text: Arc::from(text),
            style,
            glyphs: placed,
            lines,
            stops: held_stops,
            size,
            baseline: baseline_offset,
            truncated,
        }
    }

    pub fn emit(
        &mut self,
        layout: &TextLayout,
        origin: Vec2,
        color: [f32; 4],
        mut plot: impl FnMut(GlyphPlot),
    ) {
        let style = layout.style.glyph(false);
        for glyph in &layout.glyphs {
            let position = origin + Vec2::new(glyph.x, glyph.baseline);
            self.emit_glyph(
                glyph.face as usize,
                glyph.id,
                style,
                position,
                color,
                &mut plot,
            );
        }
    }

    pub fn icon(
        &mut self,
        character: char,
        style: crate::font::GlyphStyle,
        center: Vec2,
        color: [f32; 4],
        mut plot: impl FnMut(GlyphPlot),
    ) {
        let icon = self.fonts.icon(character);
        self.emit_glyph(
            icon.face,
            icon.id,
            style,
            icon_origin(icon, style, center),
            color,
            &mut plot,
        );
    }

    fn emit_glyph(
        &mut self,
        face: usize,
        id: u16,
        style: crate::font::GlyphStyle,
        origin: Vec2,
        color: [f32; 4],
        plot: &mut impl FnMut(GlyphPlot),
    ) {
        let pixels_per_point = self.pixels_per_point;
        let pen_x = origin.x * pixels_per_point;
        let baseline = origin.y * pixels_per_point;
        let slot = self.atlas.slot(
            &mut self.fonts,
            GlyphRequest {
                face,
                id,
                style,
                pixels_per_point,
            },
        );
        let Slot::Ready(slot) = slot else {
            return;
        };
        plot(GlyphPlot {
            slot,
            left: (pen_x + slot.left as f32) / pixels_per_point,
            top: (baseline - slot.top as f32) / pixels_per_point,
            width: slot.width as f32 / pixels_per_point,
            height: slot.height as f32 / pixels_per_point,
            color,
        });
    }
}

impl Default for Text {
    fn default() -> Self {
        Self::new()
    }
}

fn icon_origin(icon: IconGlyph, style: crate::font::GlyphStyle, center: Vec2) -> Vec2 {
    let half = 0.5 * style.size;
    center + Vec2::new(-icon.advance * half, half)
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphPlot {
    pub slot: GlyphSlot,
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
}

fn tokenize(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = 0usize;
    let mut previous = None;
    let mut whitespace = false;
    for (index, character) in text.char_indices() {
        let space = character == ' ';
        if let Some(before) = previous {
            let boundary =
                space != whitespace || (!space && !whitespace && allows_break(before, character));
            if boundary {
                tokens.push(&text[start..index]);
                start = index;
            }
        }
        previous = Some(character);
        whitespace = space;
    }
    if start < text.len() {
        tokens.push(&text[start..]);
    }
    tokens
}

fn allows_break(before: char, after: char) -> bool {
    if is_opening(before) || is_closing(after) {
        return false;
    }
    is_cjk(before) || is_cjk(after) || before == '-'
}

fn is_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x2E80..=0x2EFF
            | 0x3000..=0x303F
            | 0x3040..=0x30FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xAC00..=0xD7AF
            | 0xF900..=0xFAFF
            | 0xFF00..=0xFFEF
            | 0x20000..=0x2FA1F
    )
}

fn is_opening(character: char) -> bool {
    matches!(
        character,
        '（' | '《' | '「' | '『' | '【' | '〔' | '〈' | '(' | '[' | '{'
    )
}

fn is_closing(character: char) -> bool {
    matches!(
        character,
        '，' | '。'
            | '！'
            | '？'
            | '；'
            | '：'
            | '、'
            | '）'
            | '》'
            | '」'
            | '』'
            | '】'
            | '〕'
            | '〉'
            | '…'
            | '—'
            | '·'
            | ','
            | '.'
            | ';'
            | ':'
            | '!'
            | '?'
            | ')'
            | ']'
            | '}'
    )
}

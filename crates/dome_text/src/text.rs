use std::collections::HashMap;
use std::sync::Arc;

use dome_math::Vec2;

use crate::atlas::{Atlas, GlyphRequest, GlyphSlot, Slot};
use crate::font::{Fonts, IconGlyph};
use crate::style::{Align, TypeStyle};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub face: u16,
    pub id: u16,
    pub x: f32,
    pub baseline: f32,
}

#[derive(Clone, Debug)]
pub struct TextLayout {
    pub text: String,
    pub style: TypeStyle,
    pub glyphs: Vec<PlacedGlyph>,
    pub size: Vec2,
    pub baseline: f32,
    pub truncated: bool,
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
    used: u64,
    layout: Arc<TextLayout>,
}

pub struct Text {
    fonts: Fonts,
    atlas: Atlas,
    entries: HashMap<String, Vec<Entry>>,
    frame: u64,
    pixels_per_point: f32,
}

impl Text {
    pub fn new() -> Self {
        Self {
            fonts: Fonts::system(),
            atlas: Atlas::new(),
            entries: HashMap::new(),
            frame: 0,
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
        self.frame += 1;
        if self.frame.is_multiple_of(120) {
            self.evict();
        }
        if !self.atlas.exhausted() {
            return false;
        }
        self.atlas.reset();
        self.entries.clear();
        true
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

    fn evict(&mut self) {
        let floor = self.frame.saturating_sub(240);
        self.entries.retain(|_, entries| {
            entries.retain(|entry| entry.used >= floor);
            !entries.is_empty()
        });
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
        if let Some(entries) = self.entries.get_mut(text)
            && let Some(entry) = entries.iter_mut().find(|entry| entry.spec == spec)
        {
            entry.used = self.frame;
            return Arc::clone(&entry.layout);
        }
        let layout = Arc::new(self.build(text, style, max_width, wrap, truncate, align));
        self.entries
            .entry(text.to_string())
            .or_default()
            .push(Entry {
                spec,
                used: self.frame,
                layout: Arc::clone(&layout),
            });
        layout
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
        let mut lines: Vec<(Vec<PlacedGlyph>, f32)> = Vec::new();
        let mut glyphs: Vec<PlacedGlyph> = Vec::new();
        let mut pen = 0.0f32;
        let mut truncated = false;

        let flush = |lines: &mut Vec<(Vec<PlacedGlyph>, f32)>,
                     glyphs: &mut Vec<PlacedGlyph>,
                     pen: &mut f32| {
            if !glyphs.is_empty() || !lines.is_empty() {
                lines.push((std::mem::take(glyphs), *pen));
                *pen = 0.0;
            }
        };

        let glyph_style = style.glyph(false);
        'paragraphs: for paragraph in text.split('\n') {
            if !glyphs.is_empty() {
                flush(&mut lines, &mut glyphs, &mut pen);
            }
            for token in tokenize(paragraph) {
                let whitespace = token.starts_with(' ');
                let runs = if style.monospace {
                    self.fonts.monospace_runs(token)
                } else {
                    self.fonts.runs(token)
                };
                let mut shaped = Vec::with_capacity(runs.len());
                let mut width = 0.0f32;
                for run in runs {
                    let glyphs = self.fonts.shape(run.face, run.text, glyph_style, 1.0);
                    width += glyphs.width + style.tracking * glyphs.glyphs.len() as f32;
                    shaped.push((run.face as u16, glyphs));
                }
                if wrap && bounded && !whitespace && pen > 0.0 && pen + width > max_width + 0.01 {
                    flush(&mut lines, &mut glyphs, &mut pen);
                }
                if truncate && !wrap && bounded && pen + width > max_width {
                    let face = if style.monospace {
                        self.fonts.face_for_mono('\u{2026}')
                    } else {
                        self.fonts.face_for('\u{2026}')
                    };
                    let ellipsis = self.fonts.shape(face, "\u{2026}", glyph_style, 1.0);
                    let room = (max_width - pen - ellipsis.width).max(0.0);
                    let mut base = 0.0f32;
                    let mut used = 0.0f32;
                    'prefix: for (face, run) in &shaped {
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
                                face: *face,
                                id: glyph.id,
                                x: pen + x,
                                baseline: 0.0,
                            });
                            used = x + advance;
                        }
                        base += run.width + style.tracking * run.glyphs.len() as f32;
                    }
                    pen += used;
                    if pen + ellipsis.width <= max_width {
                        for glyph in &ellipsis.glyphs {
                            glyphs.push(PlacedGlyph {
                                face: face as u16,
                                id: glyph.id,
                                x: pen + glyph.x,
                                baseline: 0.0,
                            });
                        }
                        pen += ellipsis.width;
                    }
                    truncated = true;
                    flush(&mut lines, &mut glyphs, &mut pen);
                    break 'paragraphs;
                }
                for (face, run) in &shaped {
                    for (index, glyph) in run.glyphs.iter().enumerate() {
                        glyphs.push(PlacedGlyph {
                            face: *face,
                            id: glyph.id,
                            x: pen + glyph.x + style.tracking * index as f32,
                            baseline: 0.0,
                        });
                    }
                    pen += run.width + style.tracking * run.glyphs.len() as f32;
                }
            }
            if !glyphs.is_empty() {
                flush(&mut lines, &mut glyphs, &mut pen);
            } else {
                lines.push((Vec::new(), 0.0));
            }
        }
        if lines.is_empty() {
            lines.push((Vec::new(), 0.0));
        }

        let widest = lines.iter().map(|(_, width)| *width).fold(0.0f32, f32::max);
        let reference = if bounded { max_width } else { widest };
        let mut placed = Vec::new();
        for (index, (line, width)) in lines.iter().enumerate() {
            let shift = (reference - width) * align.factor();
            let baseline = baseline_offset + index as f32 * leading;
            for glyph in line {
                placed.push(PlacedGlyph {
                    x: glyph.x + shift,
                    baseline,
                    ..*glyph
                });
            }
        }
        let size = Vec2::new(widest, leading * lines.len() as f32);
        TextLayout {
            text: text.to_string(),
            style,
            glyphs: placed,
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

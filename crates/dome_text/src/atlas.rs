use std::collections::HashMap;

use crate::font::{Fonts, GlyphStyle};
use crate::glyph::GlyphRaster;

const PADDING: u32 = 1;
const SIZE: u32 = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    face: u16,
    id: u16,
    pixels: u16,
    weight: u16,
    width: u16,
    grade: i16,
    fill: u16,
    hint: bool,
    subpixel_x: u8,
    subpixel_y: u8,
}

impl GlyphKey {
    fn new(face: usize, id: u16, style: GlyphStyle, pixels_per_point: f32) -> Self {
        let pixels = (style.size * pixels_per_point * 4.0)
            .round()
            .clamp(1.0, 65535.0) as u16;
        let weight = (style.weight * 4.0).round().clamp(0.0, 65535.0) as u16;
        let width = (style.width * 4.0).round().clamp(0.0, 65535.0) as u16;
        let grade = (style.grade * 4.0).round().clamp(-32768.0, 32767.0) as i16;
        let fill = (style.fill * 1000.0).round().clamp(0.0, 65535.0) as u16;
        Self {
            face: face as u16,
            id,
            pixels,
            weight,
            width,
            grade,
            fill,
            hint: style.hint,
            subpixel_x: 0,
            subpixel_y: 0,
        }
    }

    fn with_subpixel(mut self, x: u8, y: u8) -> Self {
        self.subpixel_x = x;
        self.subpixel_y = y;
        self
    }
}

pub struct GlyphRequest {
    pub face: usize,
    pub id: u16,
    pub style: GlyphStyle,
    pub pixels_per_point: f32,
    pub position: (f32, f32),
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphSlot {
    pub uv: [f32; 4],
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug)]
struct Shelf {
    top: u32,
    height: u32,
    cursor: u32,
}

pub struct Atlas {
    pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    slots: HashMap<GlyphKey, GlyphSlot>,
    used: u64,
    revision: u64,
}

impl Default for Atlas {
    fn default() -> Self {
        Self::new()
    }
}

impl Atlas {
    pub fn new() -> Self {
        Self {
            pixels: vec![0u8; (SIZE * SIZE) as usize],
            shelves: Vec::new(),
            slots: HashMap::new(),
            used: 0,
            revision: 0,
        }
    }

    pub fn size(&self) -> u32 {
        SIZE
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn saturated(&self) -> bool {
        self.used * 8 >= u64::from(SIZE) * u64::from(SIZE) * 7
    }

    pub fn reset(&mut self) {
        self.pixels.fill(0);
        self.shelves.clear();
        self.slots.clear();
        self.used = 0;
        self.touch();
    }

    pub fn slot(&mut self, fonts: &mut Fonts, request: GlyphRequest) -> Option<GlyphSlot> {
        let fraction_x = request.position.0.rem_euclid(1.0);
        let fraction_y = request.position.1.rem_euclid(1.0);
        let quantized_x = ((fraction_x * 4.0).round() as u8).min(3);
        let quantized_y = ((fraction_y * 4.0).round() as u8).min(3);
        let key = GlyphKey::new(
            request.face,
            request.id,
            request.style,
            request.pixels_per_point,
        )
        .with_subpixel(quantized_x, quantized_y);
        if let Some(slot) = self.slots.get(&key) {
            return Some(*slot);
        }
        let raster = fonts.rasterize(
            request.face,
            request.id,
            request.style,
            request.pixels_per_point,
            [quantized_x as f32 / 4.0, quantized_y as f32 / 4.0],
        )?;
        let slot = self.insert(&raster)?;
        self.slots.insert(key, slot);
        Some(slot)
    }

    fn insert(&mut self, raster: &GlyphRaster) -> Option<GlyphSlot> {
        let width = raster.width + PADDING * 2;
        let height = raster.height + PADDING * 2;
        if width > SIZE || height > SIZE {
            return None;
        }
        let (x, y) = self.allocate(width, height)?;
        self.used += u64::from(width) * u64::from(height);
        for row in 0..raster.height {
            let source = (row * raster.width) as usize;
            let destination = ((y + PADDING + row) * SIZE + x + PADDING) as usize;
            self.pixels[destination..destination + raster.width as usize]
                .copy_from_slice(&raster.data[source..source + raster.width as usize]);
        }
        self.touch();
        Some(GlyphSlot {
            uv: [
                (x + PADDING) as f32 / SIZE as f32,
                (y + PADDING) as f32 / SIZE as f32,
                (x + PADDING + raster.width) as f32 / SIZE as f32,
                (y + PADDING + raster.height) as f32 / SIZE as f32,
            ],
            left: raster.left,
            top: raster.top,
            width: raster.width,
            height: raster.height,
        })
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        for shelf in self.shelves.iter_mut() {
            if shelf.height >= height && shelf.cursor + width + PADDING <= SIZE {
                let x = shelf.cursor;
                shelf.cursor += width + PADDING;
                return Some((x, shelf.top));
            }
        }
        let top = self
            .shelves
            .last()
            .map_or(0, |shelf| shelf.top + shelf.height);
        if top + height > SIZE {
            return None;
        }
        self.shelves.push(Shelf {
            top,
            height,
            cursor: width + PADDING,
        });
        Some((0, top))
    }

    fn touch(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("an atlas revision fits its counter");
    }
}

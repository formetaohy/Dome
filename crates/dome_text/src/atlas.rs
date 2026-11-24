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
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphRequest {
    pub face: usize,
    pub id: u16,
    pub style: GlyphStyle,
    pub pixels_per_point: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphSlot {
    pub uv: [f32; 4],
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub shelf: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            width: (self.x + self.width).max(other.x + other.width) - x,
            height: (self.y + self.height).max(other.y + other.height) - y,
        }
    }

    fn joins(self, other: Self) -> bool {
        let paired = self.y < other.y + other.height && other.y < self.y + self.height;
        paired && self.x <= other.x + other.width && other.x <= self.x + self.width
    }
}

#[derive(Clone, Debug)]
struct Shelf {
    top: u32,
    height: u32,
    cursor: u32,
    used: u64,
    keys: Vec<GlyphKey>,
}

pub struct Atlas {
    pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    slots: HashMap<GlyphKey, Option<GlyphSlot>>,
    dirty: Vec<Region>,
    revision: u64,
    frame: u64,
    recycled: bool,
    exhausted: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum Slot {
    Ready(GlyphSlot),
    Missing,
    Full,
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
            dirty: vec![Region {
                x: 0,
                y: 0,
                width: SIZE,
                height: SIZE,
            }],
            revision: 0,
            frame: 0,
            recycled: false,
            exhausted: false,
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

    pub fn exhausted(&self) -> bool {
        self.exhausted
    }

    pub fn advance(&mut self) {
        self.frame = self
            .frame
            .checked_add(1)
            .expect("an atlas frame fits its counter");
    }

    pub fn take_recycled(&mut self) -> bool {
        std::mem::take(&mut self.recycled)
    }

    pub fn drain(&mut self) -> Vec<Region> {
        std::mem::take(&mut self.dirty)
    }

    pub fn reset(&mut self) {
        self.pixels.fill(0);
        self.shelves.clear();
        self.slots.clear();
        self.exhausted = false;
        self.dirty.clear();
        self.dirty.push(Region {
            x: 0,
            y: 0,
            width: SIZE,
            height: SIZE,
        });
        self.touch();
    }

    pub fn slot(&mut self, fonts: &mut Fonts, request: GlyphRequest) -> Slot {
        let key = GlyphKey::new(
            request.face,
            request.id,
            request.style,
            request.pixels_per_point,
        );
        match self.slots.get(&key).copied() {
            Some(Some(slot)) => {
                self.touch_shelf(slot.shelf);
                return Slot::Ready(slot);
            }
            Some(None) => return Slot::Missing,
            None => {}
        }
        if self.exhausted {
            return Slot::Full;
        }
        let Some(raster) = fonts.rasterize(
            request.face,
            request.id,
            request.style,
            request.pixels_per_point,
            [0.0, 0.0],
        ) else {
            self.slots.insert(key, None);
            return Slot::Missing;
        };
        let Some(slot) = self.insert(key, &raster) else {
            self.exhausted = true;
            return Slot::Full;
        };
        self.slots.insert(key, Some(slot));
        Slot::Ready(slot)
    }

    fn insert(&mut self, key: GlyphKey, raster: &GlyphRaster) -> Option<GlyphSlot> {
        let width = raster.width + PADDING * 2;
        let height = raster.height + PADDING * 2;
        if width > SIZE || height > SIZE {
            return None;
        }
        let (x, y, shelf) = loop {
            if let Some(at) = self.allocate(width, height) {
                break at;
            }
            if !self.recycle() {
                return None;
            }
        };
        for row in 0..raster.height {
            let source = (row * raster.width) as usize;
            let destination = ((y + PADDING + row) * SIZE + x + PADDING) as usize;
            self.pixels[destination..destination + raster.width as usize]
                .copy_from_slice(&raster.data[source..source + raster.width as usize]);
        }
        self.mark(Region {
            x,
            y,
            width,
            height,
        });
        self.shelves[shelf as usize].keys.push(key);
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
            shelf,
        })
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32, u16)> {
        for (index, shelf) in self.shelves.iter_mut().enumerate() {
            if shelf.height >= height && shelf.cursor + width + PADDING <= SIZE {
                let x = shelf.cursor;
                shelf.cursor += width + PADDING;
                shelf.used = self.frame;
                return Some((x, shelf.top, index as u16));
            }
        }
        let top = self
            .shelves
            .last()
            .map_or(0, |shelf| shelf.top + shelf.height);
        if top + height > SIZE {
            return None;
        }
        let index = self.shelves.len() as u16;
        self.shelves.push(Shelf {
            top,
            height,
            cursor: width + PADDING,
            used: self.frame,
            keys: Vec::new(),
        });
        Some((0, top, index))
    }

    fn recycle(&mut self) -> bool {
        let frame = self.frame;
        let Some(index) = self
            .shelves
            .iter()
            .enumerate()
            .filter(|(_, shelf)| shelf.used < frame)
            .min_by_key(|(_, shelf)| shelf.used)
            .map(|(index, _)| index)
        else {
            return false;
        };
        let (top, height, keys) = {
            let shelf = &mut self.shelves[index];
            shelf.cursor = 0;
            shelf.used = frame;
            (shelf.top, shelf.height, std::mem::take(&mut shelf.keys))
        };
        for key in keys {
            self.slots.remove(&key);
        }
        for row in top..top + height {
            let start = (row * SIZE) as usize;
            self.pixels[start..start + SIZE as usize].fill(0);
        }
        self.mark(Region {
            x: 0,
            y: top,
            width: SIZE,
            height,
        });
        self.recycled = true;
        self.touch();
        true
    }

    fn touch_shelf(&mut self, shelf: u16) {
        if let Some(held) = self.shelves.get_mut(shelf as usize) {
            held.used = self.frame;
        }
    }

    fn mark(&mut self, region: Region) {
        let mut held = region;
        let mut at = 0;
        while at < self.dirty.len() {
            if self.dirty[at].joins(held) {
                held = held.union(self.dirty.swap_remove(at));
                at = 0;
            } else {
                at += 1;
            }
        }
        self.dirty.push(held);
    }

    fn touch(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("an atlas revision fits its counter");
    }
}

const _: () = assert!(
    SIZE.is_power_of_two(),
    "an atlas row of a power of two texels keeps every glyph upload aligned",
);

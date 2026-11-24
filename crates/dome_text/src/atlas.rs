use std::collections::HashMap;

use crate::font::{Fonts, GlyphStyle};
use crate::glyph::GlyphRaster;

const PADDING: u32 = 1;
const SIZE: u32 = 2048;
const PAGES: usize = 4;
pub const PAGE_LIMIT: u16 = PAGES as u16;

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
    pub page: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub page: u16,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    fn union(self, other: Self) -> Self {
        assert_eq!(
            self.page, other.page,
            "a dirty region joins the page it belongs to",
        );
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            page: self.page,
            x,
            y,
            width: (self.x + self.width).max(other.x + other.width) - x,
            height: (self.y + self.height).max(other.y + other.height) - y,
        }
    }

    fn joins(self, other: Self) -> bool {
        let paired = self.y < other.y + other.height && other.y < self.y + self.height;
        self.page == other.page
            && paired
            && self.x <= other.x + other.width
            && other.x <= self.x + self.width
    }
}

#[derive(Clone, Debug)]
struct Shelf {
    top: u32,
    height: u32,
    cursor: u32,
    keys: Vec<GlyphKey>,
}

struct Page {
    pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    used: u64,
}

impl Page {
    fn new(size: u32) -> Self {
        Self {
            pixels: vec![0u8; (size * size) as usize],
            shelves: Vec::new(),
            used: 0,
        }
    }

    fn allocate(&mut self, size: u32, width: u32, height: u32) -> Option<(u32, u32, u16)> {
        for (index, shelf) in self.shelves.iter_mut().enumerate() {
            if shelf.height >= height && shelf.cursor + width + PADDING <= size {
                let x = shelf.cursor;
                shelf.cursor += width + PADDING;
                return Some((x, shelf.top, index as u16));
            }
        }
        let top = self
            .shelves
            .last()
            .map_or(0, |shelf| shelf.top + shelf.height);
        if top + height > size {
            return None;
        }
        let index = self.shelves.len() as u16;
        self.shelves.push(Shelf {
            top,
            height,
            cursor: width + PADDING,
            keys: Vec::new(),
        });
        Some((0, top, index))
    }
}

pub struct Atlas {
    size: u32,
    capacity: usize,
    pages: Vec<Page>,
    slots: HashMap<GlyphKey, Option<GlyphSlot>>,
    dirty: Vec<Region>,
    revision: u64,
    frame: u64,
    recycled: bool,
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
        Self::with_capacity(SIZE, PAGES)
    }

    pub fn with_capacity(size: u32, pages: usize) -> Self {
        assert!(
            size.is_power_of_two(),
            "an atlas of {size} texels a side keeps every glyph upload aligned",
        );
        assert!(pages > 0, "an atlas holds at least one page");
        Self {
            size,
            capacity: pages,
            pages: vec![Page::new(size)],
            slots: HashMap::new(),
            dirty: vec![Region {
                page: 0,
                x: 0,
                y: 0,
                width: size,
                height: size,
            }],
            revision: 0,
            frame: 0,
            recycled: false,
        }
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn page_count(&self) -> u16 {
        self.pages.len() as u16
    }

    pub fn pixels(&self, page: u16) -> &[u8] {
        &self.pages[page as usize].pixels
    }

    pub fn revision(&self) -> u64 {
        self.revision
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

    pub fn slot(&mut self, fonts: &mut Fonts, request: GlyphRequest) -> Slot {
        let key = GlyphKey::new(
            request.face,
            request.id,
            request.style,
            request.pixels_per_point,
        );
        match self.slots.get(&key).copied() {
            Some(Some(slot)) => {
                self.pages[slot.page as usize].used = self.frame;
                return Slot::Ready(slot);
            }
            Some(None) => return Slot::Missing,
            None => {}
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
        match self.insert(key, &raster) {
            Some(slot) => Slot::Ready(slot),
            None => Slot::Full,
        }
    }

    fn insert(&mut self, key: GlyphKey, raster: &GlyphRaster) -> Option<GlyphSlot> {
        let size = self.size;
        let width = raster.width + PADDING * 2;
        let height = raster.height + PADDING * 2;
        if width > size || height > size {
            return None;
        }
        let mut page = 0usize;
        let (x, y, shelf) = loop {
            if page < self.pages.len()
                && let Some(held) = self.pages[page].allocate(size, width, height)
            {
                break held;
            }
            if page + 1 < self.pages.len() {
                page += 1;
                continue;
            }
            if self.pages.len() < self.capacity {
                self.pages.push(Page::new(size));
                page = self.pages.len() - 1;
                continue;
            }
            page = usize::from(self.recycle()?);
        };
        let held = &mut self.pages[page];
        let stride = size as usize;
        for row in 0..raster.height {
            let source = (row * raster.width) as usize;
            let destination = (y + PADDING + row) as usize * stride + (x + PADDING) as usize;
            held.pixels[destination..destination + raster.width as usize]
                .copy_from_slice(&raster.data[source..source + raster.width as usize]);
        }
        held.shelves[shelf as usize].keys.push(key);
        held.used = self.frame;
        self.mark(Region {
            page: page as u16,
            x,
            y,
            width,
            height,
        });
        let scale = size as f32;
        let slot = GlyphSlot {
            uv: [
                (x + PADDING) as f32 / scale,
                (y + PADDING) as f32 / scale,
                (x + PADDING + raster.width) as f32 / scale,
                (y + PADDING + raster.height) as f32 / scale,
            ],
            left: raster.left,
            top: raster.top,
            width: raster.width,
            height: raster.height,
            page: page as u16,
        };
        self.slots.insert(key, Some(slot));
        self.bump_revision();
        Some(slot)
    }

    fn recycle(&mut self) -> Option<u16> {
        let frame = self.frame;
        let index = self
            .pages
            .iter()
            .enumerate()
            .filter(|(_, page)| page.used < frame)
            .min_by_key(|(_, page)| page.used)
            .map(|(index, _)| index)?;
        let keys = {
            let page = &mut self.pages[index];
            page.used = frame;
            page.pixels.fill(0);
            page.shelves
                .drain(..)
                .flat_map(|shelf| shelf.keys)
                .collect::<Vec<_>>()
        };
        for key in keys {
            self.slots.remove(&key);
        }
        self.mark(Region {
            page: index as u16,
            x: 0,
            y: 0,
            width: self.size,
            height: self.size,
        });
        self.recycled = true;
        self.bump_revision();
        Some(index as u16)
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

    fn bump_revision(&mut self) {
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

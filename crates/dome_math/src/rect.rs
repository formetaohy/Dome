use crate::Vec2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub const ZERO: Self = Self {
        min: Vec2::ZERO,
        max: Vec2::ZERO,
    };

    pub const fn from_min_max(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    pub fn from_min_size(min: Vec2, size: Vec2) -> Self {
        Self {
            min,
            max: min + size,
        }
    }

    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        Self {
            min: center - 0.5 * size,
            max: center + 0.5 * size,
        }
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    pub fn left(&self) -> f32 {
        self.min.x
    }

    pub fn top(&self) -> f32 {
        self.min.y
    }

    pub fn right(&self) -> f32 {
        self.max.x
    }

    pub fn bottom(&self) -> f32 {
        self.max.y
    }

    pub fn is_empty(&self) -> bool {
        self.max.x <= self.min.x || self.max.y <= self.min.y
    }

    pub fn contains(&self, point: Vec2) -> bool {
        point.x >= self.min.x
            && point.y >= self.min.y
            && point.x < self.max.x
            && point.y < self.max.y
    }

    pub fn contains_rect(&self, other: Self) -> bool {
        other.min.x >= self.min.x
            && other.min.y >= self.min.y
            && other.max.x <= self.max.x
            && other.max.y <= self.max.y
    }

    pub fn intersects(&self, other: Self) -> bool {
        self.min.x < other.max.x
            && other.min.x < self.max.x
            && self.min.y < other.max.y
            && other.min.y < self.max.y
    }

    pub fn intersect(&self, other: Self) -> Self {
        Self {
            min: self.min.max(other.min),
            max: self.max.min(other.max),
        }
    }

    pub fn union(&self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn translate(&self, offset: Vec2) -> Self {
        Self {
            min: self.min + offset,
            max: self.max + offset,
        }
    }

    pub fn shrink(&self, amount: f32) -> Self {
        self.shrink2(Vec2::splat(amount))
    }

    pub fn shrink2(&self, amount: Vec2) -> Self {
        Self {
            min: self.min + amount,
            max: self.max - amount,
        }
    }

    pub fn expand(&self, amount: f32) -> Self {
        self.shrink(-amount)
    }

    pub fn expand2(&self, amount: Vec2) -> Self {
        Self {
            min: self.min - amount,
            max: self.max + amount,
        }
    }
}

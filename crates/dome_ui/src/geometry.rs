use dome_math::Vec2;

pub use dome_math::Rect;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Edges {
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    pub fn all(value: f32) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }

    pub fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        }
    }

    pub fn horizontal(self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(self) -> f32 {
        self.top + self.bottom
    }

    pub fn size(self) -> Vec2 {
        Vec2::new(self.horizontal(), self.vertical())
    }

    pub fn shrink(self, rect: Rect) -> Rect {
        Rect {
            min: rect.min + Vec2::new(self.left, self.top),
            max: rect.max - Vec2::new(self.right, self.bottom),
        }
    }

    pub fn expand(self, rect: Rect) -> Rect {
        Rect {
            min: rect.min - Vec2::new(self.left, self.top),
            max: rect.max + Vec2::new(self.right, self.bottom),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Band {
    pub edges: Edges,
    pub corners: Corners,
}

impl Band {
    pub const NONE: Self = Self {
        edges: Edges::ZERO,
        corners: Corners::ZERO,
    };

    pub const fn new(edges: Edges, corners: Corners) -> Self {
        Self { edges, corners }
    }

    pub fn expand(self, rect: Rect) -> Rect {
        self.edges.expand(rect)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl std::hash::Hash for Corners {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.top_left.to_bits().hash(state);
        self.top_right.to_bits().hash(state);
        self.bottom_right.to_bits().hash(state);
        self.bottom_left.to_bits().hash(state);
    }
}

impl Corners {
    pub const ZERO: Self = Self::all(0.0);

    pub const CAPSULE: f32 = 4096.0;

    pub const fn new(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    pub const fn all(value: f32) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }

    pub const fn top(value: f32) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: 0.0,
            bottom_left: 0.0,
        }
    }

    pub const fn bottom(value: f32) -> Self {
        Self {
            top_left: 0.0,
            top_right: 0.0,
            bottom_right: value,
            bottom_left: value,
        }
    }

    pub const fn left(value: f32, inner: f32) -> Self {
        Self {
            top_left: value,
            top_right: inner,
            bottom_right: inner,
            bottom_left: value,
        }
    }

    pub const fn right(value: f32, inner: f32) -> Self {
        Self {
            top_left: inner,
            top_right: value,
            bottom_right: value,
            bottom_left: inner,
        }
    }

    pub const fn capsule() -> Self {
        Self::all(Self::CAPSULE)
    }

    pub const fn split_leading(inner: f32) -> Self {
        Self::left(Self::CAPSULE, inner)
    }

    pub const fn split_trailing(inner: f32) -> Self {
        Self::right(Self::CAPSULE, inner)
    }

    pub fn group_vertical(
        index: usize,
        count: usize,
        outer: f32,
        inner: f32,
        active: bool,
    ) -> Self {
        if active || count <= 1 {
            return Self::all(outer);
        }
        if index == 0 {
            Self::new(outer, outer, inner, inner)
        } else if index + 1 >= count {
            Self::new(inner, inner, outer, outer)
        } else {
            Self::all(inner)
        }
    }

    pub fn group_horizontal(
        index: usize,
        count: usize,
        outer: f32,
        inner: f32,
        active: bool,
    ) -> Self {
        if active || count <= 1 {
            return Self::all(outer);
        }
        if index == 0 {
            Self::left(outer, inner)
        } else if index + 1 >= count {
            Self::right(outer, inner)
        } else {
            Self::all(inner)
        }
    }

    pub fn resolved(self, size: Vec2) -> Self {
        let limit = 0.5 * size.min_element();
        Self {
            top_left: self.top_left.min(limit),
            top_right: self.top_right.min(limit),
            bottom_right: self.bottom_right.min(limit),
            bottom_left: self.bottom_left.min(limit),
        }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            top_left: self.top_left + (other.top_left - self.top_left) * t,
            top_right: self.top_right + (other.top_right - self.top_right) * t,
            bottom_right: self.bottom_right + (other.bottom_right - self.bottom_right) * t,
            bottom_left: self.bottom_left + (other.bottom_left - self.bottom_left) * t,
        }
    }
}

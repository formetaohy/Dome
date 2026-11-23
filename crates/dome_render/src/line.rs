use dome_math::{Vec3, Vec4};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub start: Vec3,
    pub end: Vec3,
    pub color: Vec4,
}

#[derive(Clone, Debug, Default)]
pub struct Lines {
    lines: Vec<Line>,
}

impl Lines {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, start: Vec3, end: Vec3, color: Vec4) {
        self.lines.push(Line { start, end, color });
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }
}

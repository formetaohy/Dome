use crate::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        assert!(
            min.x <= max.x && min.y <= max.y && min.z <= max.z,
            "an oriented box orders its corners"
        );
        Self { min, max }
    }

    pub fn from_center_half_extents(center: Vec3, half_extents: Vec3) -> Self {
        assert!(
            half_extents.x >= 0.0 && half_extents.y >= 0.0 && half_extents.z >= 0.0,
            "a box of negative extent has no volume"
        );
        Self::new(center - half_extents, center + half_extents)
    }

    pub fn from_points(points: &[Vec3]) -> Self {
        let mut points = points.iter().copied();
        let first = points.next().expect("a box around no point has no corner");
        let mut min = first;
        let mut max = first;
        for point in points {
            min = min.min(point);
            max = max.max(point);
        }
        Self { min, max }
    }

    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn half_extents(self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    pub fn size(self) -> Vec3 {
        self.max - self.min
    }

    pub fn contains(self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    pub fn intersects(self, other: Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}

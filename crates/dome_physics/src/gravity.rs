use dome_math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gravity(pub Vec3);

impl Gravity {
    pub const EARTH: Self = Self(Vec3::new(0.0, -9.81, 0.0));
}

impl Default for Gravity {
    fn default() -> Self {
        Self::EARTH
    }
}

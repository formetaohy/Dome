use crate::Vec3;
use std::ops::{Mul, Sub};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat3 {
    pub cols: [Vec3; 3],
}

impl Mat3 {
    pub const IDENTITY: Self = Self::from_cols(Vec3::X, Vec3::Y, Vec3::Z);

    pub const fn from_cols(x: Vec3, y: Vec3, z: Vec3) -> Self {
        Self { cols: [x, y, z] }
    }

    pub const fn from_diagonal(diagonal: Vec3) -> Self {
        Self::from_cols(
            Vec3::new(diagonal.x, 0.0, 0.0),
            Vec3::new(0.0, diagonal.y, 0.0),
            Vec3::new(0.0, 0.0, diagonal.z),
        )
    }

    pub fn col(&self, index: usize) -> Vec3 {
        self.cols[index]
    }

    pub fn row(&self, index: usize) -> Vec3 {
        Vec3::new(
            self.cols[0].to_array()[index],
            self.cols[1].to_array()[index],
            self.cols[2].to_array()[index],
        )
    }

    pub fn transpose(&self) -> Self {
        Self::from_cols(self.row(0), self.row(1), self.row(2))
    }

    pub fn determinant(&self) -> f32 {
        let first = self.row(0);
        let second = self.row(1);
        let third = self.row(2);
        first.x * (second.y * third.z - second.z * third.y)
            - first.y * (second.x * third.z - second.z * third.x)
            + first.z * (second.x * third.y - second.y * third.x)
    }

    pub fn inverse(&self) -> Self {
        let first = self.row(0);
        let second = self.row(1);
        let third = self.row(2);
        let determinant = self.determinant();
        assert!(determinant != 0.0, "a singular matrix has no inverse");
        let inverse = 1.0 / determinant;
        Self::from_cols(
            Vec3::new(
                second.y * third.z - second.z * third.y,
                second.z * third.x - second.x * third.z,
                second.x * third.y - second.y * third.x,
            ) * inverse,
            Vec3::new(
                first.z * third.y - first.y * third.z,
                first.x * third.z - first.z * third.x,
                first.y * third.x - first.x * third.y,
            ) * inverse,
            Vec3::new(
                first.y * second.z - first.z * second.y,
                first.z * second.x - first.x * second.z,
                first.x * second.y - first.y * second.x,
            ) * inverse,
        )
    }

    pub fn mul_vec3(self, vector: Vec3) -> Vec3 {
        self.cols[0] * vector.x + self.cols[1] * vector.y + self.cols[2] * vector.z
    }
}

impl Default for Mat3 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mul for Mat3 {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Self::from_cols(
            self.mul_vec3(other.cols[0]),
            self.mul_vec3(other.cols[1]),
            self.mul_vec3(other.cols[2]),
        )
    }
}

impl Mul<Vec3> for Mat3 {
    type Output = Vec3;

    fn mul(self, vector: Vec3) -> Vec3 {
        self.mul_vec3(vector)
    }
}

impl Mul<f32> for Mat3 {
    type Output = Self;

    fn mul(self, scalar: f32) -> Self {
        Self::from_cols(
            self.cols[0] * scalar,
            self.cols[1] * scalar,
            self.cols[2] * scalar,
        )
    }
}

impl Sub for Mat3 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::from_cols(
            self.cols[0] - other.cols[0],
            self.cols[1] - other.cols[1],
            self.cols[2] - other.cols[2],
        )
    }
}

use crate::{Mat3, Quat, Vec3, Vec4};
use std::ops::Mul;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub cols: [Vec4; 4],
}

impl Mat4 {
    pub const IDENTITY: Self = Self::from_cols(Vec4::X, Vec4::Y, Vec4::Z, Vec4::W);

    pub const fn from_cols(x: Vec4, y: Vec4, z: Vec4, w: Vec4) -> Self {
        Self { cols: [x, y, z, w] }
    }

    pub const fn from_translation(translation: Vec3) -> Self {
        Self::from_cols(
            Vec4::X,
            Vec4::Y,
            Vec4::Z,
            Vec4::new(translation.x, translation.y, translation.z, 1.0),
        )
    }

    pub const fn from_scale(scale: Vec3) -> Self {
        Self::from_cols(
            Vec4::new(scale.x, 0.0, 0.0, 0.0),
            Vec4::new(0.0, scale.y, 0.0, 0.0),
            Vec4::new(0.0, 0.0, scale.z, 0.0),
            Vec4::W,
        )
    }

    pub fn from_quat(rotation: Quat) -> Self {
        let rotation = rotation.to_mat3();
        Self::from_cols(
            rotation.col(0).extend(0.0),
            rotation.col(1).extend(0.0),
            rotation.col(2).extend(0.0),
            Vec4::W,
        )
    }

    pub fn from_scale_rotation_translation(scale: Vec3, rotation: Quat, translation: Vec3) -> Self {
        let rotation = rotation.to_mat3();
        Self::from_cols(
            (rotation.col(0) * scale.x).extend(0.0),
            (rotation.col(1) * scale.y).extend(0.0),
            (rotation.col(2) * scale.z).extend(0.0),
            translation.extend(1.0),
        )
    }

    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let forward = (target - eye).normalize();
        let right = forward.cross(up).normalize();
        let held = right.cross(forward);
        Self::from_cols(
            Vec4::new(right.x, held.x, -forward.x, 0.0),
            Vec4::new(right.y, held.y, -forward.y, 0.0),
            Vec4::new(right.z, held.z, -forward.z, 0.0),
            Vec4::new(-right.dot(eye), -held.dot(eye), forward.dot(eye), 1.0),
        )
    }

    pub fn orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Self {
        assert!(
            right > left && top > bottom,
            "an orthographic view of {left} to {right} by {bottom} to {top} spans no extent"
        );
        assert!(far > near, "an orthographic view clips {near} to {far}");
        Self::from_cols(
            Vec4::new(2.0 / (right - left), 0.0, 0.0, 0.0),
            Vec4::new(0.0, 2.0 / (top - bottom), 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0 / (near - far), 0.0),
            Vec4::new(
                (right + left) / (left - right),
                (top + bottom) / (bottom - top),
                near / (near - far),
                1.0,
            ),
        )
    }

    pub fn perspective(vertical_fov: f32, aspect: f32, near: f32, far: f32) -> Self {
        assert!(
            vertical_fov > 0.0 && vertical_fov < std::f32::consts::PI,
            "a perspective of {vertical_fov} radians spans no view"
        );
        assert!(
            aspect > 0.0,
            "a viewport {aspect} times as wide as tall has no aspect"
        );
        assert!(
            near > 0.0 && far > near,
            "a perspective clips {near} to {far}"
        );
        let focal = 1.0 / (vertical_fov * 0.5).tan();
        let depth = far / (near - far);
        Self::from_cols(
            Vec4::new(focal / aspect, 0.0, 0.0, 0.0),
            Vec4::new(0.0, focal, 0.0, 0.0),
            Vec4::new(0.0, 0.0, depth, -1.0),
            Vec4::new(0.0, 0.0, depth * near, 0.0),
        )
    }

    pub fn col(&self, index: usize) -> Vec4 {
        self.cols[index]
    }

    pub fn flip_clip_y(self) -> Self {
        let [x, y, z, w] = self.cols;
        Self::from_cols(
            Vec4::new(x.x, -x.y, x.z, x.w),
            Vec4::new(y.x, -y.y, y.z, y.w),
            Vec4::new(z.x, -z.y, z.z, z.w),
            Vec4::new(w.x, -w.y, w.z, w.w),
        )
    }

    pub fn row(&self, index: usize) -> Vec4 {
        let [x, y, z, w] = self.cols;
        Vec4::new(
            x.to_array()[index],
            y.to_array()[index],
            z.to_array()[index],
            w.to_array()[index],
        )
    }

    pub fn to_mat3(&self) -> Mat3 {
        Mat3::from_cols(
            self.cols[0].truncate(),
            self.cols[1].truncate(),
            self.cols[2].truncate(),
        )
    }

    pub fn transpose(&self) -> Self {
        Self::from_cols(self.row(0), self.row(1), self.row(2), self.row(3))
    }

    pub fn determinant(&self) -> f32 {
        let rows = self.rows();
        let mut determinant = 0.0;
        for (column, value) in rows[0].into_iter().enumerate() {
            let sign = if column % 2 == 0 { 1.0 } else { -1.0 };
            determinant += sign * value * minor(&rows, 0, column);
        }
        determinant
    }

    pub fn inverse(&self) -> Self {
        let rows = self.rows();
        let determinant = self.determinant();
        assert!(determinant != 0.0, "a singular matrix has no inverse");
        let scale = 1.0 / determinant;
        let mut entries = [[0.0; 4]; 4];
        for (row, values) in entries.iter_mut().enumerate() {
            for (column, entry) in values.iter_mut().enumerate() {
                let sign = if (row + column) % 2 == 0 { 1.0 } else { -1.0 };
                *entry = sign * minor(&rows, column, row) * scale;
            }
        }
        Self::from_cols(
            Vec4::new(entries[0][0], entries[1][0], entries[2][0], entries[3][0]),
            Vec4::new(entries[0][1], entries[1][1], entries[2][1], entries[3][1]),
            Vec4::new(entries[0][2], entries[1][2], entries[2][2], entries[3][2]),
            Vec4::new(entries[0][3], entries[1][3], entries[2][3], entries[3][3]),
        )
    }

    pub fn mul_vec4(self, vector: Vec4) -> Vec4 {
        self.cols[0] * vector.x
            + self.cols[1] * vector.y
            + self.cols[2] * vector.z
            + self.cols[3] * vector.w
    }

    pub fn transform_point(self, point: Vec3) -> Vec3 {
        let homogeneous = self.mul_vec4(point.extend(1.0));
        assert!(
            homogeneous.w != 0.0,
            "a point projected onto the horizon has no position"
        );
        homogeneous.truncate() / homogeneous.w
    }

    pub fn transform_vector(self, vector: Vec3) -> Vec3 {
        self.mul_vec4(vector.extend(0.0)).truncate()
    }

    fn rows(&self) -> [[f32; 4]; 4] {
        let [x, y, z, w] = self.cols;
        [
            [x.x, y.x, z.x, w.x],
            [x.y, y.y, z.y, w.y],
            [x.z, y.z, z.z, w.z],
            [x.w, y.w, z.w, w.w],
        ]
    }
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mul for Mat4 {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Self::from_cols(
            self.mul_vec4(other.cols[0]),
            self.mul_vec4(other.cols[1]),
            self.mul_vec4(other.cols[2]),
            self.mul_vec4(other.cols[3]),
        )
    }
}

impl Mul<Vec4> for Mat4 {
    type Output = Vec4;

    fn mul(self, vector: Vec4) -> Vec4 {
        self.mul_vec4(vector)
    }
}

fn minor(rows: &[[f32; 4]; 4], skip_row: usize, skip_column: usize) -> f32 {
    let mut kept = [[0.0; 3]; 3];
    let mut target_row = 0;
    for (source_row, values) in rows.iter().enumerate() {
        if source_row == skip_row {
            continue;
        }
        let mut target_column = 0;
        for (source_column, value) in values.iter().enumerate() {
            if source_column == skip_column {
                continue;
            }
            kept[target_row][target_column] = *value;
            target_column += 1;
        }
        target_row += 1;
    }
    determinant(&kept)
}

fn determinant(rows: &[[f32; 3]; 3]) -> f32 {
    rows[0][0] * (rows[1][1] * rows[2][2] - rows[1][2] * rows[2][1])
        - rows[0][1] * (rows[1][0] * rows[2][2] - rows[1][2] * rows[2][0])
        + rows[0][2] * (rows[1][0] * rows[2][1] - rows[1][1] * rows[2][0])
}

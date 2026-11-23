use crate::{Mat3, Vec3};
use std::ops::Mul;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const IDENTITY: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    pub fn from_axis_angle(axis: Vec3, radians: f32) -> Self {
        let axis = axis.normalize();
        let (sin, cos) = (radians * 0.5).sin_cos();
        Self::new(axis.x * sin, axis.y * sin, axis.z * sin, cos)
    }

    pub fn from_rotation_x(radians: f32) -> Self {
        Self::from_axis_angle(Vec3::X, radians)
    }

    pub fn from_rotation_y(radians: f32) -> Self {
        Self::from_axis_angle(Vec3::Y, radians)
    }

    pub fn from_rotation_z(radians: f32) -> Self {
        Self::from_axis_angle(Vec3::Z, radians)
    }

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z + self.w * other.w
    }

    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    pub fn normalize(self) -> Self {
        let length = self.length();
        assert!(length > 0.0, "a quaternion of no length has no rotation");
        Self::new(
            self.x / length,
            self.y / length,
            self.z / length,
            self.w / length,
        )
    }

    pub fn conjugate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }

    pub fn inverse(self) -> Self {
        let length_squared = self.length_squared();
        assert!(
            length_squared > 0.0,
            "a quaternion of no length has no inverse"
        );
        let conjugate = self.conjugate();
        Self::new(
            conjugate.x / length_squared,
            conjugate.y / length_squared,
            conjugate.z / length_squared,
            conjugate.w / length_squared,
        )
    }

    pub fn slerp(self, other: Self, phase: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&phase),
            "a rotation blends at {phase} outside its unit span",
        );
        let mut other = other;
        let mut dot = self.dot(other);
        if dot < 0.0 {
            other = Self::new(-other.x, -other.y, -other.z, -other.w);
            dot = -dot;
        }
        if dot > 0.9995 {
            return Self::new(
                self.x + (other.x - self.x) * phase,
                self.y + (other.y - self.y) * phase,
                self.z + (other.z - self.z) * phase,
                self.w + (other.w - self.w) * phase,
            )
            .normalize();
        }
        let angle = dot.clamp(-1.0, 1.0).acos();
        let sine = angle.sin();
        let first = ((1.0 - phase) * angle).sin() / sine;
        let second = (phase * angle).sin() / sine;
        Self::new(
            self.x * first + other.x * second,
            self.y * first + other.y * second,
            self.z * first + other.z * second,
            self.w * first + other.w * second,
        )
    }

    pub fn from_yaw_pitch_roll(yaw: f32, pitch: f32, roll: f32) -> Self {
        Self::from_rotation_y(yaw) * Self::from_rotation_x(pitch) * Self::from_rotation_z(roll)
    }

    pub fn to_yaw_pitch_roll(self) -> Vec3 {
        let matrix = self.to_mat3();
        let [m00, _, m02] = matrix.row(0).to_array();
        let [m10, m11, m12] = matrix.row(1).to_array();
        let [m20, _, m22] = matrix.row(2).to_array();
        let pitch = (-m12).clamp(-1.0, 1.0).asin();
        if m11.abs() > 1e-6 {
            let yaw = m02.atan2(m22);
            let roll = m10.atan2(m11);
            Vec3::new(yaw, pitch, roll)
        } else {
            let yaw = (m20 * -m12.signum()).atan2(m00);
            Vec3::new(yaw, pitch, 0.0)
        }
    }

    pub fn to_axis_angle(self) -> (Vec3, f32) {
        let held = if self.w < 0.0 {
            Self::new(-self.x, -self.y, -self.z, -self.w)
        } else {
            self
        };
        let axis = Vec3::new(held.x, held.y, held.z);
        let sine = axis.length();
        if sine <= 1e-9 {
            return (Vec3::Y, 0.0);
        }
        (axis / sine, 2.0 * sine.atan2(held.w))
    }

    pub fn rotate_vec3(self, vector: Vec3) -> Vec3 {
        let axis = Vec3::new(self.x, self.y, self.z);
        let doubled = axis.cross(vector) * 2.0;
        vector + doubled * self.w + axis.cross(doubled)
    }

    pub fn to_mat3(self) -> Mat3 {
        let Self { x, y, z, w } = self;
        let x2 = x + x;
        let y2 = y + y;
        let z2 = z + z;
        let xx = x * x2;
        let xy = x * y2;
        let xz = x * z2;
        let yy = y * y2;
        let yz = y * z2;
        let zz = z * z2;
        let wx = w * x2;
        let wy = w * y2;
        let wz = w * z2;
        Mat3::from_cols(
            Vec3::new(1.0 - (yy + zz), xy + wz, xz - wy),
            Vec3::new(xy - wz, 1.0 - (xx + zz), yz + wx),
            Vec3::new(xz + wy, yz - wx, 1.0 - (xx + yy)),
        )
    }
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mul for Quat {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Self::new(
            self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
            self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
        )
    }
}

impl Mul<Vec3> for Quat {
    type Output = Vec3;

    fn mul(self, vector: Vec3) -> Vec3 {
        self.rotate_vec3(vector)
    }
}

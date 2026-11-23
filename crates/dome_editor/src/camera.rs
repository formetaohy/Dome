use dome_math::{Quat, Transform, Vec2, Vec3};
use dome_render::Camera;

pub(crate) fn editor() -> Camera {
    Camera::new(1.0, 0.1, 1000.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pose {
    pub(crate) target: Vec3,
    pub(crate) distance: f32,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
}

pub(crate) struct Orbit {
    pose: Pose,
}

impl Orbit {
    pub(crate) fn rotate(&mut self, delta: Vec2) {
        self.pose.yaw -= delta.x * 0.01;
        self.pose.pitch = (self.pose.pitch + delta.y * 0.01).clamp(-1.5, 1.5);
    }

    pub(crate) fn pan(&mut self, delta: Vec2) {
        let rotation = self.rotation();
        let right = rotation.rotate_vec3(Vec3::X);
        let up = rotation.rotate_vec3(Vec3::Y);
        let scale = self.pose.distance * 0.002;
        self.pose.target += right * (-delta.x * scale) + up * (delta.y * scale);
    }

    pub(crate) fn zoom(&mut self, scroll: f32) {
        self.pose.distance =
            (self.pose.distance * (1.0 - scroll * 0.001).clamp(0.1, 10.0)).clamp(0.1, 1000.0);
    }

    pub(crate) fn view(&self) -> Transform {
        let rotation = self.rotation();
        let framing = rotation.rotate_vec3(Vec3::new(0.0, 0.0, self.pose.distance));
        Transform::from_scale_rotation_translation(Vec3::ONE, rotation, self.pose.target + framing)
    }

    pub(crate) fn focus(&mut self, target: Vec3) {
        assert!(
            target.to_array().iter().all(|held| held.is_finite()),
            "an orbit focuses {target:?}",
        );
        self.pose.target = target;
    }

    fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.pose.yaw) * Quat::from_rotation_x(self.pose.pitch)
    }
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            pose: Pose {
                target: Vec3::ZERO,
                distance: 5.0,
                yaw: 0.6,
                pitch: -0.42,
            },
        }
    }
}

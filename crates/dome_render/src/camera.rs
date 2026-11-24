use crate::view::View;
use dome_math::{Mat4, Transform, Vec2, Vec3, Vec4};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub vertical_fov: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn new(vertical_fov: f32, near: f32, far: f32) -> Self {
        let camera = Self {
            vertical_fov,
            near,
            far,
        };
        camera.check();
        camera
    }

    fn check(self) {
        assert!(
            self.near > 0.0 && self.far > self.near && self.far.is_finite(),
            "a camera of near {} and far {} spans no depth",
            self.near,
            self.far,
        );
        assert!(
            self.vertical_fov > 0.0 && self.vertical_fov < std::f32::consts::PI,
            "a camera of {0} radians sees no cone",
            self.vertical_fov,
        );
    }

    pub fn view(self, transform: Transform, aspect: f32) -> View {
        self.view_jittered(transform, aspect, Vec2::ZERO, 1, 1)
    }

    pub fn view_jittered(
        self,
        transform: Transform,
        aspect: f32,
        jitter: Vec2,
        width: u32,
        height: u32,
    ) -> View {
        self.check();
        assert!(
            width > 0 && height > 0,
            "a {width} by {height} viewport holds no subpixel offset",
        );
        let eye = transform.translation;
        let forward = transform.rotation.rotate_vec3(Vec3::new(0.0, 0.0, -1.0));
        let up = transform.rotation.rotate_vec3(Vec3::Y);
        let projection = Mat4::perspective(self.vertical_fov, aspect, self.near, self.far);
        let vertical = -0.5;
        let offset = Vec4::new(
            jitter.x * 2.0 / width as f32,
            jitter.y / height as f32 / vertical,
            0.0,
            0.0,
        );
        let projection = Mat4::from_cols(
            projection.col(0),
            projection.col(1),
            projection.col(2) + Vec4::new(-offset.x, -offset.y, 0.0, 0.0),
            projection.col(3),
        );
        View::of(projection * Mat4::look_at(eye, eye + forward, up))
    }
}

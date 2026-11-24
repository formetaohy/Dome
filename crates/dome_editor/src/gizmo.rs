use dome_math::{Mat4, Quat, Rect, Transform, Vec2, Vec3, Vec4};
use dome_render::Camera;
use dome_ui::{Color, Corners, Painter};

const LENGTH: f32 = 88.0;
const HALF: f32 = 1.6;
const TIP: f32 = 4.0;
const UNIFORM: f32 = 5.0;
const REACH: f32 = 9.0;
const MINIMUM: f32 = 4.0;
const RING_STEPS: usize = 64;
const RING_HALF: f32 = 1.2;
const PLANE_FACING: f32 = 0.2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Translate,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Space {
    World,
    Local,
}

impl Space {
    pub fn toggled(self) -> Self {
        match self {
            Self::World => Self::Local,
            Self::Local => Self::World,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }

    pub fn color(self) -> Color {
        match self {
            Self::X => Color::bytes(255, 110, 118),
            Self::Y => Color::bytes(109, 224, 142),
            Self::Z => Color::bytes(116, 178, 255),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Grip {
    Axis(Axis),
    Uniform,
}

impl Grip {
    fn color(self) -> Color {
        match self {
            Self::Axis(axis) => axis.color(),
            Self::Uniform => Color::new(0.86, 0.87, 0.90, 1.0),
        }
    }
}

enum Shape {
    Axis {
        start: Vec2,
        tip: Vec2,
        tip_half: f32,
    },
    Square {
        center: Vec2,
        half: f32,
    },
    Ring {
        points: Vec<Vec2>,
    },
}

pub struct Handle {
    pub grip: Grip,
    axis: Vec3,
    origin: Vec3,
    shape: Shape,
    screen: Vec2,
    pixels: f32,
    center: Vec2,
}

#[derive(Clone, Copy)]
pub struct Target {
    pub camera: Camera,
    pub view: Transform,
    pub viewport: Rect,
}

impl Target {
    pub fn handles(&self, origin: Vec3, basis: [Vec3; 3], mode: Mode) -> Vec<Handle> {
        let mut handles = Vec::new();
        if self.viewport.is_empty() {
            return handles;
        }
        let clip = self.clip();
        let Some(center) = self.project(clip, origin) else {
            return handles;
        };
        match mode {
            Mode::Translate | Mode::Scale => {
                if mode == Mode::Scale {
                    handles.push(Handle {
                        grip: Grip::Uniform,
                        axis: Vec3::ZERO,
                        origin,
                        shape: Shape::Square {
                            center,
                            half: UNIFORM,
                        },
                        screen: Vec2::ZERO,
                        pixels: 0.0,
                        center,
                    });
                }
                for axis in [Axis::X, Axis::Y, Axis::Z] {
                    let world = basis[axis.index()];
                    let Some(tip) = self.project(clip, origin + world) else {
                        continue;
                    };
                    let length = tip.distance(center);
                    if length < MINIMUM {
                        continue;
                    }
                    let reach = LENGTH / length;
                    let Some(tip) = self.project(clip, origin + world * reach) else {
                        continue;
                    };
                    let screen = tip - center;
                    let length = screen.length();
                    if length < MINIMUM {
                        continue;
                    }
                    handles.push(Handle {
                        grip: Grip::Axis(axis),
                        axis: world,
                        origin,
                        shape: Shape::Axis {
                            start: center,
                            tip,
                            tip_half: TIP,
                        },
                        screen: screen.normalize(),
                        pixels: reach / length,
                        center,
                    });
                }
            }
            Mode::Rotate => {
                let right = self.view.rotation.rotate_vec3(Vec3::X);
                let up = self.view.rotation.rotate_vec3(Vec3::Y);
                let mut pixels = 0.0;
                let mut held = 0;
                for direction in [right, up] {
                    if let Some(point) = self.project(clip, origin + direction) {
                        pixels += point.distance(center);
                        held += 1;
                    }
                }
                if held == 0 || pixels <= f32::EPSILON {
                    return handles;
                }
                let radius = LENGTH * held as f32 / pixels;
                for axis in [Axis::X, Axis::Y, Axis::Z] {
                    let world = basis[axis.index()];
                    let (u, v) = tangents(world);
                    let mut points = Vec::with_capacity(RING_STEPS + 1);
                    for step in 0..=RING_STEPS {
                        let angle = step as f32 / RING_STEPS as f32 * std::f32::consts::TAU;
                        let world = origin + (u * angle.cos() + v * angle.sin()) * radius;
                        match self.project(clip, world) {
                            Some(point) => points.push(point),
                            None => break,
                        }
                    }
                    if points.len() <= RING_STEPS {
                        continue;
                    }
                    handles.push(Handle {
                        grip: Grip::Axis(axis),
                        axis: world,
                        origin,
                        shape: Shape::Ring { points },
                        screen: Vec2::ZERO,
                        pixels: 0.0,
                        center,
                    });
                }
            }
        }
        handles
    }

    fn clip(&self) -> Mat4 {
        let aspect = self.viewport.width() / self.viewport.height();
        self.camera.view(self.view, aspect).clip
    }

    fn project(&self, matrix: Mat4, point: Vec3) -> Option<Vec2> {
        let clip = matrix.mul_vec4(point.extend(1.0));
        if clip.w <= 0.0 {
            return None;
        }
        let depth = clip.z / clip.w;
        if !(0.0..=1.0).contains(&depth) {
            return None;
        }
        let x = clip.x / clip.w;
        let y = clip.y / clip.w;
        Some(Vec2::new(
            self.viewport.min.x + (x * 0.5 + 0.5) * self.viewport.width(),
            self.viewport.min.y + (0.5 - y * 0.5) * self.viewport.height(),
        ))
    }

    fn ray(&self, point: Vec2) -> Ray {
        let inverse = self.clip().inverse();
        let x = (point.x - self.viewport.min.x) / self.viewport.width() * 2.0 - 1.0;
        let y = 1.0 - (point.y - self.viewport.min.y) / self.viewport.height() * 2.0;
        let place = |depth: f32| {
            let held = inverse.mul_vec4(Vec4::new(x, y, depth, 1.0));
            Vec3::new(held.x / held.w, held.y / held.w, held.z / held.w)
        };
        let near = place(0.0);
        Ray {
            origin: self.view.translation,
            direction: (place(1.0) - near).normalize(),
        }
    }
}

struct Ray {
    origin: Vec3,
    direction: Vec3,
}

impl Shape {
    fn distance(&self, point: Vec2) -> f32 {
        match self {
            Self::Axis { start, tip, .. } => offset(point, *start, *tip),
            Self::Square { center, half } => (point.distance(*center) - half).max(0.0),
            Self::Ring { points } => points
                .windows(2)
                .map(|pair| offset(point, pair[0], pair[1]))
                .fold(f32::INFINITY, f32::min),
        }
    }
}

pub fn hit(handles: &[Handle], point: Vec2) -> Option<Grip> {
    handles
        .iter()
        .map(|handle| (handle.grip, handle.shape.distance(point)))
        .filter(|(_, distance)| *distance <= REACH)
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(grip, _)| grip)
}

pub fn paint(handles: &[Handle], hover: Option<Grip>, active: Option<Grip>, painter: &mut Painter) {
    let mut origin = None;
    for handle in handles {
        origin = Some(handle.center);
        let color = if Some(handle.grip) == active {
            Color::new(1.0, 0.94, 0.62, 1.0)
        } else if Some(handle.grip) == hover {
            lit(handle.grip.color())
        } else {
            handle.grip.color()
        };
        match &handle.shape {
            Shape::Axis {
                start,
                tip,
                tip_half,
            } => {
                painter.segment(*start, *tip, HALF * 2.1, color);
                painter.rect(
                    Rect::from_center_size(*tip, Vec2::splat(*tip_half * 2.2)),
                    Corners::all(3.0),
                    color,
                );
            }
            Shape::Square { center, half } => {
                painter.rect(
                    Rect::from_center_size(*center, Vec2::splat(*half * 2.2)),
                    Corners::all(3.5),
                    color,
                );
            }
            Shape::Ring { points } => {
                for pair in points.windows(2) {
                    painter.segment(pair[0], pair[1], RING_HALF * 2.1, color);
                }
            }
        }
    }
    if let Some(center) = origin
        && !handles
            .iter()
            .any(|handle| matches!(handle.grip, Grip::Uniform))
    {
        painter.rect(
            Rect::from_center_size(center, Vec2::splat(7.0)),
            Corners::capsule(),
            Color::bytes(236, 240, 248),
        );
    }
}

pub fn begin(
    target: Target,
    handle: &Handle,
    mode: Mode,
    local: Transform,
    parent: Transform,
    press: Vec2,
) -> Drag {
    let axis = handle.axis;
    let (u, v) = if mode == Mode::Rotate {
        tangents(axis)
    } else {
        (Vec3::ZERO, Vec3::ZERO)
    };
    let mut drag = Drag {
        target,
        mode,
        grip: handle.grip,
        axis,
        origin: handle.origin,
        local,
        parent,
        press,
        screen: handle.screen,
        pixels: handle.pixels,
        u,
        v,
        center: handle.center,
        screen_angle: 0.0,
        plane_start: None,
        uniform: 1.0,
    };
    match mode {
        Mode::Translate => {}
        Mode::Scale => {
            drag.uniform = press.distance(handle.center).max(1.0);
        }
        Mode::Rotate => {
            drag.screen_angle = angle_of(press - handle.center);
            drag.plane_start = drag.plane_point(press);
        }
    }
    drag
}

pub struct Drag {
    target: Target,
    mode: Mode,
    grip: Grip,
    axis: Vec3,
    origin: Vec3,
    local: Transform,
    parent: Transform,
    press: Vec2,
    screen: Vec2,
    pixels: f32,
    u: Vec3,
    v: Vec3,
    center: Vec2,
    screen_angle: f32,
    plane_start: Option<Vec3>,
    uniform: f32,
}

impl Drag {
    pub fn grip(&self) -> Grip {
        self.grip
    }

    pub fn transform(&self, point: Vec2) -> Transform {
        let mut held = self.local;
        match self.mode {
            Mode::Translate => {
                let travel = (point - self.press).dot(self.screen) * self.pixels;
                held.translation = self
                    .parent
                    .inverse_transform_point(self.origin + self.axis * travel);
            }
            Mode::Rotate => {
                let delta = self.rotation(point);
                let turn = Quat::from_axis_angle(self.axis, delta);
                held.rotation = (self.parent.rotation.inverse()
                    * turn
                    * (self.parent.rotation * self.local.rotation))
                    .normalize();
            }
            Mode::Scale => {
                let factor = match self.grip {
                    Grip::Uniform => {
                        (point.distance(self.center) / self.uniform).clamp(0.01, 100.0)
                    }
                    Grip::Axis(_) => {
                        (1.0 + (point - self.press).dot(self.screen) / LENGTH).clamp(0.01, 100.0)
                    }
                };
                match self.grip {
                    Grip::Uniform => held.scale = self.local.scale * factor,
                    Grip::Axis(axis) => match axis {
                        Axis::X => held.scale.x = self.local.scale.x * factor,
                        Axis::Y => held.scale.y = self.local.scale.y * factor,
                        Axis::Z => held.scale.z = self.local.scale.z * factor,
                    },
                }
            }
        }
        held
    }

    fn rotation(&self, point: Vec2) -> f32 {
        if let Some(start) = self.plane_start
            && let Some(current) = self.plane_point(point)
        {
            let from = start.dot(self.v).atan2(start.dot(self.u));
            let to = current.dot(self.v).atan2(current.dot(self.u));
            return wrapped(to - from);
        }
        let facing = if self
            .axis
            .dot(self.target.view.rotation.rotate_vec3(-Vec3::Z))
            >= 0.0
        {
            1.0
        } else {
            -1.0
        };
        wrapped(angle_of(point - self.center) - self.screen_angle) * facing
    }

    fn plane_point(&self, point: Vec2) -> Option<Vec3> {
        let ray = self.target.ray(point);
        let facing = ray.direction.dot(self.axis);
        if facing.abs() < PLANE_FACING {
            return None;
        }
        let distance = (self.origin - ray.origin).dot(self.axis) / facing;
        if distance <= 0.0 {
            return None;
        }
        Some(ray.origin + ray.direction * distance - self.origin)
    }
}

fn tangents(axis: Vec3) -> (Vec3, Vec3) {
    let helper = if axis.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let u = axis.cross(helper).normalize();
    let v = axis.cross(u);
    (u, v)
}

fn angle_of(direction: Vec2) -> f32 {
    direction.y.atan2(direction.x)
}

fn wrapped(angle: f32) -> f32 {
    angle.sin().atan2(angle.cos())
}

fn lit(color: Color) -> Color {
    Color::new(
        color.red + (1.0 - color.red) * 0.45,
        color.green + (1.0 - color.green) * 0.45,
        color.blue + (1.0 - color.blue) * 0.45,
        1.0,
    )
}

fn offset(point: Vec2, start: Vec2, tip: Vec2) -> f32 {
    let span = tip - start;
    let length = span.length_squared();
    if length == 0.0 {
        return point.distance(start);
    }
    let along = ((point - start).dot(span) / length).clamp(0.0, 1.0);
    point.distance(start + span * along)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target {
            camera: Camera::new(std::f32::consts::FRAC_PI_2, 0.1, 100.0),
            view: Transform::from_translation(Vec3::new(0.0, 0.0, 5.0)),
            viewport: Rect::from_min_size(Vec2::ZERO, Vec2::new(800.0, 600.0)),
        }
    }

    fn basis() -> [Vec3; 3] {
        [Vec3::X, Vec3::Y, Vec3::Z]
    }

    fn grip(handles: &[Handle], grip: Grip) -> &Handle {
        handles
            .iter()
            .find(|handle| handle.grip == grip)
            .expect("a gizmo holds the grip its mode draws")
    }

    #[test]
    fn a_gizmo_drags_a_node_along_its_axes_and_rings() {
        let target = target();
        let translate = target.handles(Vec3::ZERO, basis(), Mode::Translate);
        let Shape::Axis { tip, .. } = grip(&translate, Grip::Axis(Axis::X)).shape else {
            panic!("a translate gizmo draws axis shafts");
        };
        assert_eq!(hit(&translate, tip), Some(Grip::Axis(Axis::X)));
        let drag = begin(
            target,
            grip(&translate, Grip::Axis(Axis::X)),
            Mode::Translate,
            Transform::IDENTITY,
            Transform::IDENTITY,
            tip,
        );
        let moved = drag.transform(tip + Vec2::new(0.0, -44.0));
        assert!(moved.translation.x.abs() < 0.001);

        let rotate = target.handles(Vec3::ZERO, basis(), Mode::Rotate);
        let ring = grip(&rotate, Grip::Axis(Axis::Z));
        let Shape::Ring { points } = &ring.shape else {
            panic!("a rotate gizmo draws rings");
        };
        let point = *points
            .iter()
            .find(|point| hit(&rotate, **point) == Some(Grip::Axis(Axis::Z)))
            .expect("the z ring draws a grabbable point");
        let drag = begin(
            target,
            ring,
            Mode::Rotate,
            Transform::IDENTITY,
            Transform::IDENTITY,
            point,
        );
        let moved = drag.transform(ring.center + Vec2::new(0.0, -88.0));
        assert!(moved.rotation.to_yaw_pitch_roll().length() > 0.0);
    }
}

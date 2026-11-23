use crate::body::Body;
use crate::joint::{COORDINATES, Coordinate, Joint, JointCache, JointKind, Limit, Motor};
use crate::solver::{MAX_CORRECTION, effective_mass, pair_mut};
use dome_math::{Mat3, Quat, Vec3};
use std::f32::consts::{PI, TAU};

const LINEAR_SLOP: f32 = 1e-4;
const ANGULAR_SLOP: f32 = 1e-4;
const ROWS: usize = 3;

pub(crate) struct JointConstraint {
    first: usize,
    second: usize,
    kind: JointKind,
    anchor: Vec3,
    other_anchor: Vec3,
    axis: Vec3,
    other_axis: Vec3,
    reference: Quat,
    delta: f32,
    linear: Block,
    angular: Block,
    drives: [Option<Drive>; COORDINATES],
    cache: JointCache,
}

#[derive(Clone, Copy)]
struct Drive {
    coordinate: Coordinate,
    limit: Option<Limit>,
    motor: Option<Motor>,
}

struct Block {
    axes: Vec<Vec3>,
    angular: bool,
    offset_first: Vec3,
    offset_second: Vec3,
    mass: [[f32; ROWS]; ROWS],
}

#[derive(Clone, Copy)]
struct Push {
    axis: Vec3,
    angular: bool,
    offset_first: Vec3,
    offset_second: Vec3,
}

#[derive(Clone, Copy)]
struct Row {
    target: f32,
    lower: f32,
    upper: f32,
}

#[derive(Clone, Copy)]
struct Pose {
    push: Push,
    mass: f32,
    value: f32,
}

impl Push {
    fn angular(axis: Vec3) -> Self {
        Self {
            axis,
            angular: true,
            offset_first: Vec3::ZERO,
            offset_second: Vec3::ZERO,
        }
    }

    fn linear(axis: Vec3, offset_first: Vec3, offset_second: Vec3) -> Self {
        Self {
            axis,
            angular: false,
            offset_first,
            offset_second,
        }
    }

    fn flipped(self) -> Self {
        Self {
            axis: -self.axis,
            ..self
        }
    }
}

impl Block {
    fn new(
        axes: Vec<Vec3>,
        angular: bool,
        offset_first: Vec3,
        offset_second: Vec3,
        first: &Body,
        second: &Body,
    ) -> Self {
        let base = if angular {
            add(first.inverse_inertia, second.inverse_inertia)
        } else {
            let mut base =
                Mat3::from_diagonal(Vec3::splat(first.inverse_mass + second.inverse_mass));
            for (body, offset) in [(first, offset_first), (second, offset_second)] {
                let skew = skew(offset);
                base = add(base, skew * body.inverse_inertia * skew.transpose());
            }
            base
        };
        let mut mass = [[0.0; ROWS]; ROWS];
        for (row, axis) in axes.iter().enumerate() {
            let held = base.mul_vec3(*axis);
            for (column, other_axis) in axes.iter().enumerate() {
                mass[row][column] = other_axis.dot(held);
            }
        }
        Self {
            axes,
            angular,
            offset_first,
            offset_second,
            mass,
        }
    }

    fn push(&self, index: usize) -> Push {
        Push {
            axis: self.axes[index],
            angular: self.angular,
            offset_first: self.offset_first,
            offset_second: self.offset_second,
        }
    }

    fn apply(&self, first: &mut Body, second: &mut Body, index: usize, impulse: f32) {
        apply(first, second, self.push(index), impulse);
    }
}

impl JointConstraint {
    pub(crate) fn cache(&self) -> JointCache {
        self.cache
    }

    fn pose(&self, coordinate: Coordinate, first: &Body, second: &Body) -> Pose {
        match coordinate {
            Coordinate::Twist => {
                let axis = first.rotation.rotate_vec3(self.axis);
                let measured = twist(self.relative(first, second), self.axis);
                Pose {
                    push: Push::angular(axis),
                    mass: angular_mass(first, second, axis),
                    value: continued(
                        coordinate,
                        self.cache.drives[coordinate.slot()].value,
                        measured,
                    ),
                }
            }
            Coordinate::Swing => {
                let (direction, value) = swing(self.relative(first, second), self.axis);
                let axis = first.rotation.rotate_vec3(direction);
                Pose {
                    push: Push::angular(axis),
                    mass: angular_mass(first, second, axis),
                    value,
                }
            }
            Coordinate::Slide => {
                let axis = first.rotation.rotate_vec3(self.axis);
                let offset_first = first.anchor_offset(self.anchor);
                let offset_second = second.anchor_offset(self.other_anchor);
                let span = second.position + offset_second - first.position - offset_first;
                Pose {
                    push: Push::linear(axis, offset_first, offset_second),
                    mass: effective_mass(first, second, offset_first, offset_second, axis),
                    value: span.dot(axis),
                }
            }
            Coordinate::Span => {
                let offset_first = first.anchor_offset(self.anchor);
                let offset_second = second.anchor_offset(self.other_anchor);
                let span = second.position + offset_second - first.position - offset_first;
                let reach = span.length();
                let axis = if reach > 1e-6 {
                    span / reach
                } else {
                    first.rotation.rotate_vec3(self.axis)
                };
                Pose {
                    push: Push::linear(axis, offset_first, offset_second),
                    mass: effective_mass(first, second, offset_first, offset_second, axis),
                    value: reach,
                }
            }
        }
    }

    fn relative(&self, first: &Body, second: &Body) -> Quat {
        first.rotation.inverse() * second.rotation * self.reference.inverse()
    }
}

pub(crate) fn prepare(
    first: usize,
    second: usize,
    joint: Joint,
    bodies: &[Body],
    delta: f32,
) -> JointConstraint {
    let held = &bodies[first];
    let other = &bodies[second];
    let offset_first = held.anchor_offset(joint.anchor());
    let offset_second = other.anchor_offset(joint.other_anchor());
    let axis_first = held.rotation.rotate_vec3(joint.axis());
    let basis = perpendicular_basis(joint.axis()).map(|axis| held.rotation.rotate_vec3(axis));
    let (linear_axes, angular_axes) = match joint.kind() {
        JointKind::Fixed => (
            vec![Vec3::X, Vec3::Y, Vec3::Z],
            vec![Vec3::X, Vec3::Y, Vec3::Z],
        ),
        JointKind::Spherical => (vec![Vec3::X, Vec3::Y, Vec3::Z], Vec::new()),
        JointKind::Hinge => (vec![Vec3::X, Vec3::Y, Vec3::Z], basis.to_vec()),
        JointKind::Prismatic => (basis.to_vec(), vec![basis[0], basis[1], axis_first]),
        JointKind::Distance => (Vec::new(), Vec::new()),
    };
    let mut drives = [None; COORDINATES];
    for coordinate in joint.kind().coordinates() {
        let limit = match (joint.limit(*coordinate), coordinate) {
            (Some(limit), _) => Some(limit),
            (None, Coordinate::Span) => Some(Limit::new(joint.rest_length(), joint.rest_length())),
            (None, _) => None,
        };
        let motor = joint.motor(*coordinate);
        if limit.is_some() || motor.is_some() {
            drives[coordinate.slot()] = Some(Drive {
                coordinate: *coordinate,
                limit,
                motor,
            });
        }
    }
    let mut constraint = JointConstraint {
        first,
        second,
        kind: joint.kind(),
        anchor: joint.anchor(),
        other_anchor: joint.other_anchor(),
        axis: joint.axis(),
        other_axis: joint.other_axis(),
        reference: joint.reference(),
        delta,
        linear: Block::new(linear_axes, false, offset_first, offset_second, held, other),
        angular: Block::new(angular_axes, true, Vec3::ZERO, Vec3::ZERO, held, other),
        drives,
        cache: joint.cache(),
    };
    for coordinate in constraint.kind.coordinates() {
        let value = constraint.pose(*coordinate, held, other).value;
        constraint.cache.drives[coordinate.slot()].value = value;
    }
    constraint
}

pub(crate) fn warm_start(bodies: &mut [Body], constraints: &[JointConstraint]) {
    for constraint in constraints {
        let (first, second) = pair_mut(bodies, constraint.first, constraint.second);
        for block in [&constraint.linear, &constraint.angular] {
            let held = if block.angular {
                constraint.cache.angular
            } else {
                constraint.cache.linear
            };
            for (index, _) in block.axes.iter().enumerate() {
                if held[index] != 0.0 {
                    block.apply(first, second, index, held[index]);
                }
            }
        }
        for slot in 0..COORDINATES {
            let Some(drive) = constraint.drives[slot] else {
                continue;
            };
            let cache = constraint.cache.drives[slot];
            let pose = constraint.pose(drive.coordinate, first, second);
            if drive.limit.is_some() {
                if cache.limit[0] != 0.0 {
                    apply(first, second, pose.push.flipped(), cache.limit[0]);
                }
                if cache.limit[1] != 0.0 {
                    apply(first, second, pose.push, cache.limit[1]);
                }
            }
            if drive.motor.is_some() && cache.motor != 0.0 {
                apply(first, second, pose.push, cache.motor);
            }
        }
    }
}

pub(crate) fn solve_velocity(bodies: &mut [Body], constraints: &mut [JointConstraint]) {
    for constraint in constraints {
        let (first, second) = pair_mut(bodies, constraint.first, constraint.second);
        let delta = constraint.delta;
        for slot in 0..COORDINATES {
            let Some(drive) = constraint.drives[slot] else {
                continue;
            };
            let pose = constraint.pose(drive.coordinate, first, second);
            let cache = &mut constraint.cache.drives[slot];
            if let Some(limit) = drive.limit {
                let lower = Row {
                    target: (pose.value - limit.lower()) / delta,
                    lower: f32::NEG_INFINITY,
                    upper: 0.0,
                };
                solve_row(
                    first,
                    second,
                    pose.push.flipped(),
                    pose.mass,
                    lower,
                    &mut cache.limit[0],
                );
                let upper = Row {
                    target: (limit.upper() - pose.value) / delta,
                    lower: f32::NEG_INFINITY,
                    upper: 0.0,
                };
                solve_row(
                    first,
                    second,
                    pose.push,
                    pose.mass,
                    upper,
                    &mut cache.limit[1],
                );
            }
            if let Some(motor) = drive.motor {
                let bound = motor.force() * delta;
                let row = Row {
                    target: motor.speed(),
                    lower: -bound,
                    upper: bound,
                };
                solve_row(first, second, pose.push, pose.mass, row, &mut cache.motor);
            }
        }
        solve_block(
            first,
            second,
            &constraint.linear,
            &mut constraint.cache.linear,
        );
        solve_block(
            first,
            second,
            &constraint.angular,
            &mut constraint.cache.angular,
        );
    }
}

pub(crate) fn settle(bodies: &mut [Body], constraints: &mut [JointConstraint]) {
    for constraint in constraints {
        let (first, second) = pair_mut(bodies, constraint.first, constraint.second);
        for block in [&constraint.linear, &constraint.angular] {
            for (index, axis) in block.axes.iter().enumerate() {
                let mass = reciprocal(block.mass[index][index]);
                if block.angular {
                    let error = angular_error(constraint, first, second, index, *axis);
                    correct(first, second, block.push(index), error, mass);
                } else {
                    let offset_first = first.anchor_offset(constraint.anchor);
                    let offset_second = second.anchor_offset(constraint.other_anchor);
                    let span = second.position + offset_second - first.position - offset_first;
                    let push = Push::linear(*axis, offset_first, offset_second);
                    correct(first, second, push, span.dot(*axis), mass);
                }
            }
        }
        for slot in 0..COORDINATES {
            let Some(drive) = constraint.drives[slot] else {
                continue;
            };
            let Some(limit) = drive.limit else {
                continue;
            };
            let pose = constraint.pose(drive.coordinate, first, second);
            let error = if pose.value > limit.upper() {
                pose.value - limit.upper()
            } else if pose.value < limit.lower() {
                pose.value - limit.lower()
            } else {
                continue;
            };
            correct(first, second, pose.push, error, pose.mass);
        }
        for coordinate in constraint.kind.coordinates() {
            let value = constraint.pose(*coordinate, first, second).value;
            constraint.cache.drives[coordinate.slot()].value = value;
        }
    }
}

fn solve_row(
    first: &mut Body,
    second: &mut Body,
    push: Push,
    mass: f32,
    row: Row,
    impulse: &mut f32,
) {
    let velocity = if push.angular {
        (second.rigid.angular_velocity - first.rigid.angular_velocity).dot(push.axis)
    } else {
        (second.point_velocity(push.offset_second) - first.point_velocity(push.offset_first))
            .dot(push.axis)
    };
    let total = (*impulse + (row.target - velocity) * mass).clamp(row.lower, row.upper);
    let applied = total - *impulse;
    *impulse = total;
    apply(first, second, push, applied);
}

fn solve_block(first: &mut Body, second: &mut Body, block: &Block, impulses: &mut [f32; ROWS]) {
    let count = block.axes.len();
    if count == 0 {
        return;
    }
    let relative = if block.angular {
        second.rigid.angular_velocity - first.rigid.angular_velocity
    } else {
        second.point_velocity(block.offset_second) - first.point_velocity(block.offset_first)
    };
    let mut rhs = [0.0; ROWS];
    for (index, axis) in block.axes.iter().enumerate() {
        rhs[index] = -axis.dot(relative);
    }
    let Some(delta) = solve_mass(&block.mass, count, rhs) else {
        return;
    };
    for index in 0..count {
        let total = impulses[index] + delta[index];
        let applied = total - impulses[index];
        impulses[index] = total;
        block.apply(first, second, index, applied);
    }
}

fn solve_mass(mass: &[[f32; ROWS]; ROWS], count: usize, rhs: [f32; ROWS]) -> Option<[f32; ROWS]> {
    let mut matrix = *mass;
    let mut solution = rhs;
    for column in 0..count {
        let mut pivot = column;
        for row in column + 1..count {
            if matrix[row][column].abs() > matrix[pivot][column].abs() {
                pivot = row;
            }
        }
        if matrix[pivot][column] == 0.0 {
            return None;
        }
        matrix.swap(pivot, column);
        solution.swap(pivot, column);
        let held = matrix[column];
        let scale = solution[column];
        for row in column + 1..count {
            let factor = matrix[row][column] / held[column];
            for (entry, value) in matrix[row][column..].iter_mut().enumerate() {
                *value -= factor * held[column + entry];
            }
            solution[row] -= factor * scale;
        }
    }
    let mut result = [0.0; ROWS];
    for column in (0..count).rev() {
        let held = matrix[column];
        for (entry, value) in held[column + 1..count].iter().enumerate() {
            result[column] -= value * result[column + 1 + entry];
        }
        result[column] += solution[column];
        result[column] /= held[column];
    }
    Some(result)
}

fn apply(first: &mut Body, second: &mut Body, push: Push, impulse: f32) {
    if push.angular {
        first.apply_angular_impulse(push.axis * -impulse);
        second.apply_angular_impulse(push.axis * impulse);
    } else {
        first.apply_impulse(push.offset_first, push.axis * -impulse);
        second.apply_impulse(push.offset_second, push.axis * impulse);
    }
}

fn correct(first: &mut Body, second: &mut Body, push: Push, error: f32, mass: f32) {
    let slop = if push.angular {
        ANGULAR_SLOP
    } else {
        LINEAR_SLOP
    };
    let magnitude = error.abs() - slop;
    if magnitude <= 0.0 {
        return;
    }
    let error = magnitude.min(MAX_CORRECTION).copysign(error);
    let impulse = -error * mass;
    if push.angular {
        first.apply_angular_position_impulse(push.axis * -impulse);
        second.apply_angular_position_impulse(push.axis * impulse);
    } else {
        first.apply_position_impulse(push.offset_first, push.axis * -impulse);
        second.apply_position_impulse(push.offset_second, push.axis * impulse);
    }
}

fn twist(relative: Quat, axis: Vec3) -> f32 {
    let projected = Vec3::new(relative.x, relative.y, relative.z).dot(axis);
    2.0 * projected.atan2(relative.w)
}

fn swing(relative: Quat, axis: Vec3) -> (Vec3, f32) {
    let projected = Vec3::new(relative.x, relative.y, relative.z).dot(axis);
    let held = Quat::new(
        axis.x * projected,
        axis.y * projected,
        axis.z * projected,
        relative.w,
    );
    let held = if held.length() > 1e-6 {
        held.normalize()
    } else {
        Quat::IDENTITY
    };
    let swing = relative * held.inverse();
    let swing = if swing.w < 0.0 {
        Quat::new(-swing.x, -swing.y, -swing.z, -swing.w)
    } else {
        swing
    };
    let angle = 2.0 * swing.w.clamp(-1.0, 1.0).acos();
    let sine = (1.0 - swing.w * swing.w).max(0.0).sqrt();
    let direction = if sine > 1e-6 {
        Vec3::new(swing.x, swing.y, swing.z) / sine
    } else {
        Vec3::ZERO
    };
    (direction, angle)
}

fn continued(coordinate: Coordinate, cached: f32, measured: f32) -> f32 {
    match coordinate {
        Coordinate::Twist => cached + wrap(measured - cached),
        Coordinate::Swing | Coordinate::Slide | Coordinate::Span => measured,
    }
}

fn wrap(angle: f32) -> f32 {
    let wrapped = angle - TAU * ((angle + PI) / TAU).floor();
    if wrapped > PI { wrapped - TAU } else { wrapped }
}

fn angular_mass(first: &Body, second: &Body, axis: Vec3) -> f32 {
    reciprocal(
        axis.dot(first.inverse_inertia.mul_vec3(axis))
            + axis.dot(second.inverse_inertia.mul_vec3(axis)),
    )
}

fn angular_error(
    constraint: &JointConstraint,
    first: &Body,
    second: &Body,
    index: usize,
    axis: Vec3,
) -> f32 {
    match constraint.kind {
        JointKind::Fixed => {
            rotation_error(first.rotation, second.rotation, constraint.reference).dot(axis)
        }
        JointKind::Hinge => {
            let held = first.rotation.rotate_vec3(constraint.axis);
            let other = second.rotation.rotate_vec3(constraint.other_axis);
            other.cross(held).dot(axis)
        }
        JointKind::Prismatic if index == 2 => {
            rotation_error(first.rotation, second.rotation, constraint.reference).dot(axis)
        }
        JointKind::Prismatic => {
            let held = first.rotation.rotate_vec3(constraint.axis);
            let other = second.rotation.rotate_vec3(constraint.other_axis);
            other.cross(held).dot(axis)
        }
        JointKind::Spherical | JointKind::Distance => 0.0,
    }
}

fn rotation_error(first: Quat, second: Quat, reference: Quat) -> Vec3 {
    let relative = first.inverse() * second;
    let error = relative * reference.inverse();
    let (axis, angle) = logarithm(error);
    first.rotate_vec3(axis * angle)
}

fn logarithm(rotation: Quat) -> (Vec3, f32) {
    let rotation = if rotation.w < 0.0 {
        Quat::new(-rotation.x, -rotation.y, -rotation.z, -rotation.w)
    } else {
        rotation
    };
    let angle = 2.0 * rotation.w.clamp(-1.0, 1.0).acos();
    let scale = 1.0 - rotation.w * rotation.w;
    if scale <= 1e-12 {
        return (Vec3::ZERO, 0.0);
    }
    (
        Vec3::new(rotation.x, rotation.y, rotation.z) / scale.sqrt(),
        angle,
    )
}

fn skew(offset: Vec3) -> Mat3 {
    Mat3::from_cols(
        Vec3::new(0.0, offset.z, -offset.y),
        Vec3::new(-offset.z, 0.0, offset.x),
        Vec3::new(offset.y, -offset.x, 0.0),
    )
}

fn add(left: Mat3, right: Mat3) -> Mat3 {
    Mat3::from_cols(
        left.col(0) + right.col(0),
        left.col(1) + right.col(1),
        left.col(2) + right.col(2),
    )
}

fn reciprocal(value: f32) -> f32 {
    if value > 0.0 { 1.0 / value } else { 0.0 }
}

fn perpendicular_basis(axis: Vec3) -> [Vec3; 2] {
    let reference = if axis.x.abs() < 0.577 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let first = axis.cross(reference).normalize();
    [first, axis.cross(first)]
}

use crate::reference::NodeId;
use dome_math::Vec3;
use dome_physics::{Coordinate, JointKind};

#[derive(Clone, Debug, PartialEq)]
pub struct JointRef {
    pub other: Option<NodeId>,
    pub kind: JointKind,
    pub anchor: Vec3,
    pub other_anchor: Vec3,
    pub axis: Vec3,
    pub rest_length: f32,
    pub collide: bool,
    pub limit: bool,
    pub lower: f32,
    pub upper: f32,
    pub cone: f32,
    pub motor: bool,
    pub motor_speed: f32,
    pub motor_force: f32,
    pub position: f32,
    pub swing: f32,
}

impl JointRef {
    pub fn new() -> Self {
        Self {
            other: None,
            kind: JointKind::Fixed,
            anchor: Vec3::ZERO,
            other_anchor: Vec3::ZERO,
            axis: Vec3::Y,
            rest_length: 0.0,
            collide: false,
            limit: false,
            lower: -1.0,
            upper: 1.0,
            cone: 0.0,
            motor: false,
            motor_speed: 0.0,
            motor_force: 100.0,
            position: 0.0,
            swing: 0.0,
        }
    }

    pub(crate) fn primary(&self) -> Option<Coordinate> {
        self.kind.coordinates().first().copied()
    }
}

impl Default for JointRef {
    fn default() -> Self {
        Self::new()
    }
}

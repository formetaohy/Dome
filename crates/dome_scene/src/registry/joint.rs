use super::{Assets, SceneComponent};
use crate::body::BodyRef;
use crate::field::{Axis, finite, no_axis, number, vector_toml, with_axis};
use crate::install::NodeMap;
use crate::joint::JointRef;
use crate::node::SceneNode;
use crate::reference::NodeId;
use crate::scene::Scene;
use crate::scene_value::SceneValue;
use crate::schema::{ComponentSpec, Field, FieldKind, Reach, ReferenceKind};
use crate::target::valid_id;
use crate::value::{Effect, Value};
use dome_ecs::{Entity, World};
use dome_math::Vec3;
use dome_physics::{BodyKind, Coordinate, Joint, JointKind};
use dome_toml::{Fields, Toml, floats};
use std::f32::consts::PI;

static FIELDS: &[Field] = &[
    Field {
        name: "other",
        label: "Other",
        kind: FieldKind::OptionalReference(ReferenceKind::Node),
    },
    Field {
        name: "kind",
        label: "Kind",
        kind: FieldKind::Choice(&["fixed", "spherical", "hinge", "prismatic", "distance"]),
    },
    Field {
        name: "anchor",
        label: "Anchor",
        kind: FieldKind::Vector {
            range: [-10000.0, 10000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "other_anchor",
        label: "Other Anchor",
        kind: FieldKind::Vector {
            range: [-10000.0, 10000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "axis",
        label: "Axis",
        kind: FieldKind::Vector {
            range: [-10000.0, 10000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "rest_length",
        label: "Rest Length",
        kind: FieldKind::Number {
            range: [0.0, 10000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "collide",
        label: "Collide",
        kind: FieldKind::Bool,
    },
    Field {
        name: "limit",
        label: "Limit",
        kind: FieldKind::Bool,
    },
    Field {
        name: "lower",
        label: "Lower",
        kind: FieldKind::Number {
            range: [-10000.0, 10000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "upper",
        label: "Upper",
        kind: FieldKind::Number {
            range: [-10000.0, 10000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "cone",
        label: "Cone",
        kind: FieldKind::Number {
            range: [0.0, 180.0],
            speed: 0.5,
        },
    },
    Field {
        name: "motor",
        label: "Motor",
        kind: FieldKind::Bool,
    },
    Field {
        name: "motor_speed",
        label: "Motor Speed",
        kind: FieldKind::Number {
            range: [-10000.0, 10000.0],
            speed: 0.5,
        },
    },
    Field {
        name: "motor_force",
        label: "Motor Force",
        kind: FieldKind::Number {
            range: [0.0001, 1000000.0],
            speed: 0.1,
        },
    },
];

static RUNTIME_FIELDS: &[&str] = &["motor_speed"];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "joint",
    label: "Joint",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::None,
};

impl SceneComponent for JointRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(match field {
            "other" => Value::Text(
                self.other
                    .as_ref()
                    .map_or(String::new(), |other| other.as_str().to_owned()),
            ),
            "kind" => Value::Text(String::from(self.kind.name())),
            "anchor" => Value::Vector(self.anchor),
            "other_anchor" => Value::Vector(self.other_anchor),
            "axis" => Value::Vector(self.axis),
            "rest_length" => Value::Number(self.rest_length),
            "collide" => Value::Bool(self.collide),
            "limit" => Value::Bool(self.limit),
            "lower" => Value::Number(angle_of(self.primary(), self.lower)),
            "upper" => Value::Number(angle_of(self.primary(), self.upper)),
            "cone" => Value::Number(degrees(self.cone)),
            "motor" => Value::Bool(self.motor),
            "motor_speed" => Value::Number(angle_of(self.primary(), self.motor_speed)),
            "motor_force" => Value::Number(self.motor_force),
            "position" => Value::Number(angle_of(self.primary(), self.position)),
            "swing" => Value::Number(degrees(self.swing)),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        match field {
            "other" => {
                no_axis(axis, "the joint other")?;
                let id = value.text()?;
                self.other = if id.is_empty() {
                    None
                } else {
                    if !valid_id(id) {
                        return Err(format!("the joint other holds the id {id}"));
                    }
                    Some(NodeId::new(id))
                };
            }
            "kind" => {
                no_axis(axis, "the joint kind")?;
                let name = value.text()?;
                self.kind = JointKind::of(name)
                    .ok_or_else(|| format!("the joint holds the kind {name}"))?;
            }
            "anchor" => {
                let anchor = with_axis(self.anchor, axis, value, "the joint anchor")?;
                finite(anchor, "the joint anchor")?;
                self.anchor = anchor;
            }
            "other_anchor" => {
                let anchor = with_axis(self.other_anchor, axis, value, "the joint other anchor")?;
                finite(anchor, "the joint other anchor")?;
                self.other_anchor = anchor;
            }
            "axis" => {
                let direction = with_axis(self.axis, axis, value, "the joint axis")?;
                finite(direction, "the joint axis")?;
                self.axis = direction;
            }
            "rest_length" => {
                no_axis(axis, "the joint rest length")?;
                let length = number(value, "the joint rest length")?;
                if length < 0.0 {
                    return Err(format!("the joint holds the rest length {length}"));
                }
                self.rest_length = length;
            }
            "collide" => {
                no_axis(axis, "the joint collide")?;
                self.collide = value.boolean()?;
            }
            "limit" => {
                no_axis(axis, "the joint limit")?;
                self.limit = value.boolean()?;
            }
            "lower" => {
                no_axis(axis, "the joint lower limit")?;
                self.lower = angle_to(self.primary(), number(value, "the joint lower limit")?);
            }
            "upper" => {
                no_axis(axis, "the joint upper limit")?;
                self.upper = angle_to(self.primary(), number(value, "the joint upper limit")?);
            }
            "cone" => {
                no_axis(axis, "the joint cone")?;
                self.cone = radians(number(value, "the joint cone")?);
            }
            "motor" => {
                no_axis(axis, "the joint motor")?;
                self.motor = value.boolean()?;
            }
            "motor_speed" => {
                no_axis(axis, "the joint motor speed")?;
                self.motor_speed =
                    angle_to(self.primary(), number(value, "the joint motor speed")?);
            }
            "motor_force" => {
                no_axis(axis, "the joint motor force")?;
                let force = number(value, "the joint motor force")?;
                if force <= 0.0 {
                    return Err(format!("the joint holds the motor force {force}"));
                }
                self.motor_force = force;
            }
            _ => return Err(format!("the joint holds no property {field}")),
        }
        Ok(Effect::Value)
    }

    fn authored(&self) -> Toml {
        Toml::object([
            (
                "other",
                Toml::string(self.other.as_ref().map_or("", |other| other.as_str())),
            ),
            ("kind", Toml::string(self.kind.name())),
            ("anchor", vector_toml(self.anchor.to_array())),
            ("other_anchor", vector_toml(self.other_anchor.to_array())),
            ("axis", vector_toml(self.axis.to_array())),
            ("rest_length", Toml::from(self.rest_length)),
            ("collide", Toml::boolean(self.collide)),
            ("limit", Toml::boolean(self.limit)),
            ("lower", Toml::from(self.lower)),
            ("upper", Toml::from(self.upper)),
            ("cone", Toml::from(self.cone)),
            ("motor", Toml::boolean(self.motor)),
            ("motor_speed", Toml::from(self.motor_speed)),
            ("motor_force", Toml::from(self.motor_force)),
        ])
    }

    fn observed(&self) -> Toml {
        Toml::object([
            (
                "other",
                Toml::string(self.other.as_ref().map_or("", |other| other.as_str())),
            ),
            ("kind", Toml::string(self.kind.name())),
            ("anchor", vector_toml(self.anchor.to_array())),
            ("other_anchor", vector_toml(self.other_anchor.to_array())),
            ("axis", vector_toml(self.axis.to_array())),
            ("rest_length", Toml::from(self.rest_length)),
            ("collide", Toml::boolean(self.collide)),
            ("limit", Toml::boolean(self.limit)),
            ("lower", Toml::from(angle_of(self.primary(), self.lower))),
            ("upper", Toml::from(angle_of(self.primary(), self.upper))),
            ("cone", Toml::from(degrees(self.cone))),
            ("motor", Toml::boolean(self.motor)),
            (
                "motor_speed",
                Toml::from(angle_of(self.primary(), self.motor_speed)),
            ),
            ("motor_force", Toml::from(self.motor_force)),
            (
                "position",
                Toml::from(angle_of(self.primary(), self.position)),
            ),
            ("swing", Toml::from(degrees(self.swing))),
        ])
    }

    fn install(
        &self,
        scene: &Scene,
        node: &str,
        world: &mut World,
        entity: Entity,
        _assets: &dyn Assets,
    ) {
        let Some(other) = &self.other else {
            return;
        };
        let first = scene
            .node_index(node)
            .expect("a joint installs on a node of its scene");
        let second = scene
            .node_index(other.as_str())
            .expect("a joint names a node of its scene");
        let first = scene.world_transform(first);
        let second = scene.world_transform(second);
        let first_rotation = first.rotation.normalize();
        let second_rotation = second.rotation.normalize();
        let other = world
            .resource::<NodeMap>()
            .expect("a scene installs the nodes its components name")
            .entity(other.as_str());
        let joint = match self.kind {
            JointKind::Fixed => Joint::fixed(
                other,
                self.anchor,
                self.other_anchor,
                first_rotation,
                second_rotation,
            ),
            JointKind::Spherical => Joint::spherical(
                other,
                self.anchor,
                self.other_anchor,
                self.axis,
                first_rotation,
                second_rotation,
            ),
            JointKind::Hinge => {
                let axis = first_rotation.rotate_vec3(self.axis.normalize());
                let other_axis = second_rotation.inverse().rotate_vec3(axis);
                Joint::hinge(
                    other,
                    self.anchor,
                    self.other_anchor,
                    self.axis,
                    other_axis,
                    first_rotation,
                    second_rotation,
                )
            }
            JointKind::Prismatic => {
                let axis = first_rotation.rotate_vec3(self.axis.normalize());
                let other_axis = second_rotation.inverse().rotate_vec3(axis);
                Joint::prismatic(
                    other,
                    self.anchor,
                    self.other_anchor,
                    self.axis,
                    other_axis,
                    first_rotation,
                    second_rotation,
                )
            }
            JointKind::Distance => {
                Joint::distance(other, self.anchor, self.other_anchor, self.rest_length)
            }
        };
        let joint = match (self.limit, self.motor) {
            (false, false) => joint,
            (true, false) => joint.with_limit(self.lower, self.upper),
            (false, true) => joint.with_motor(self.motor_speed, self.motor_force),
            (true, true) => joint
                .with_limit(self.lower, self.upper)
                .with_motor(self.motor_speed, self.motor_force),
        };
        let joint = if self.kind == JointKind::Spherical && self.cone > 0.0 {
            joint.with_cone(self.cone)
        } else {
            joint
        };
        world.insert(entity, joint.with_collide(self.collide));
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        finite(self.anchor, &format!("the anchor of joint {node}"))?;
        finite(
            self.other_anchor,
            &format!("the other anchor of joint {node}"),
        )?;
        finite(self.axis, &format!("the axis of joint {node}"))?;
        if matches!(
            self.kind,
            JointKind::Hinge | JointKind::Prismatic | JointKind::Spherical
        ) && self.axis.length_squared() <= 0.0
        {
            return Err(format!("the joint {node} holds an axis of no direction"));
        }
        if !(self.rest_length.is_finite() && self.rest_length >= 0.0) {
            return Err(format!(
                "the joint {node} holds the rest length {}",
                self.rest_length,
            ));
        }
        if !(self.cone.is_finite() && (0.0..=PI).contains(&self.cone)) {
            return Err(format!("the joint {node} holds the cone {}", self.cone));
        }
        if self.cone > 0.0 && self.kind != JointKind::Spherical {
            return Err(format!(
                "the joint {node} holds a cone of a {} joint",
                self.kind.name(),
            ));
        }
        if self.limit {
            if self.primary().is_none() {
                return Err(format!(
                    "the joint {node} limits no coordinate of a {} joint",
                    self.kind.name(),
                ));
            }
            if !(self.lower.is_finite() && self.upper.is_finite() && self.lower <= self.upper) {
                return Err(format!(
                    "the joint {node} holds the limits {} to {}",
                    self.lower, self.upper,
                ));
            }
        }
        if self.motor {
            if self.primary().is_none() {
                return Err(format!(
                    "the joint {node} drives no coordinate of a {} joint",
                    self.kind.name(),
                ));
            }
            if !self.motor_speed.is_finite() {
                return Err(format!(
                    "the joint {node} holds the motor speed {}",
                    self.motor_speed,
                ));
            }
            if !(self.motor_force.is_finite() && self.motor_force > 0.0) {
                return Err(format!(
                    "the joint {node} holds the motor force {}",
                    self.motor_force,
                ));
            }
        }
        let self_index = scene
            .node_index(node)
            .ok_or_else(|| format!("the scene holds no node {node}"))?;
        for component in ["body", "joint", "collider"] {
            if scene.ancestors_hold(self_index, component) {
                return Err(format!(
                    "the joint {node} sits under the {component} of a parent",
                ));
            }
        }
        let Some(other) = &self.other else {
            return Ok(());
        };
        if other.as_str() == node {
            return Err(format!("the joint {node} joins itself"));
        }
        other.validate(scene, &format!("the other of joint {node}"))?;
        for id in [node, other.as_str()] {
            let index = scene
                .node_index(id)
                .ok_or_else(|| format!("the scene holds no node {id}"))?;
            for component in ["body", "joint", "collider"] {
                if scene.ancestors_hold(index, component) {
                    return Err(format!(
                        "the joint {node} joins the node {id}, which sits under the {component} of a parent",
                    ));
                }
            }
            let held = &scene.nodes()[index];
            if held
                .component::<BodyRef>()
                .is_some_and(|body| body.kind == BodyKind::Dynamic)
                && !scene.subtree_holds(index, "collider")
            {
                return Err(format!(
                    "the joint {node} joins the dynamic node {id}, which holds no collider",
                ));
            }
        }
        Ok(())
    }

    fn create(_scene: &Scene) -> Result<Self, String> {
        Ok(Self::new())
    }

    fn from_toml(value: Toml, what: &str) -> Self {
        let mut fields = Fields::of(value, what);
        let other = fields
            .take("other")
            .map(|value| {
                let id = value.string_value(&format!("the other of {what}"));
                if id.is_empty() {
                    None
                } else {
                    assert!(valid_id(&id), "{what} holds the other id {id}");
                    Some(NodeId::new(id))
                }
            })
            .unwrap_or(None);
        let kind = fields
            .need("kind")
            .string_value(&format!("the kind of {what}"));
        let kind = JointKind::of(&kind).unwrap_or_else(|| panic!("{what} holds the kind {kind}"));
        let anchor = Vec3::from_array(floats::<3>(
            fields.need("anchor"),
            &format!("the anchor of {what}"),
        ));
        let other_anchor = Vec3::from_array(floats::<3>(
            fields.need("other_anchor"),
            &format!("the other anchor of {what}"),
        ));
        let axis = Vec3::from_array(floats::<3>(
            fields.need("axis"),
            &format!("the axis of {what}"),
        ));
        let rest_length = fields
            .need("rest_length")
            .f32_value(&format!("the rest length of {what}"));
        let collide = fields
            .need("collide")
            .boolean_value(&format!("the collide of {what}"));
        let limit = fields
            .need("limit")
            .boolean_value(&format!("the limit of {what}"));
        let lower = fields
            .need("lower")
            .f32_value(&format!("the lower limit of {what}"));
        let upper = fields
            .need("upper")
            .f32_value(&format!("the upper limit of {what}"));
        let cone = fields
            .need("cone")
            .f32_value(&format!("the cone of {what}"));
        let motor = fields
            .need("motor")
            .boolean_value(&format!("the motor of {what}"));
        let motor_speed = fields
            .need("motor_speed")
            .f32_value(&format!("the motor speed of {what}"));
        let motor_force = fields
            .need("motor_force")
            .f32_value(&format!("the motor force of {what}"));
        fields.done();
        Self {
            other,
            kind,
            anchor,
            other_anchor,
            axis,
            rest_length,
            collide,
            limit,
            lower,
            upper,
            cone,
            motor,
            motor_speed,
            motor_force,
            position: 0.0,
            swing: 0.0,
        }
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        let joint = *world.get::<Joint>(entity)?;
        let other = world.get::<SceneNode>(joint.other())?;
        let primary = primary(joint.kind());
        let limit = primary.and_then(|coordinate| joint.limit(coordinate));
        let cone = joint.limit(Coordinate::Swing);
        let motor = primary.and_then(|coordinate| joint.motor(coordinate));
        Some(Self {
            other: Some(NodeId::new(other.id.clone())),
            kind: joint.kind(),
            anchor: joint.anchor(),
            other_anchor: joint.other_anchor(),
            axis: joint.axis(),
            rest_length: joint.rest_length(),
            collide: joint.collide(),
            limit: limit.is_some(),
            lower: limit.map_or(0.0, |limit| limit.lower()),
            upper: limit.map_or(0.0, |limit| limit.upper()),
            cone: cone.map_or(0.0, |limit| limit.upper()),
            motor: motor.is_some(),
            motor_speed: motor.map_or(0.0, |motor| motor.speed()),
            motor_force: motor.map_or(0.0, |motor| motor.force()),
            position: primary.map_or(0.0, |coordinate| joint.value(coordinate)),
            swing: joint.value(Coordinate::Swing),
        })
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<Joint>(entity);
    }

    fn write_world(
        world: &mut World,
        entity: Entity,
        field: &str,
        axis: Option<Axis>,
        value: &Value,
    ) -> Result<(), String> {
        let mut joint = *world
            .get::<Joint>(entity)
            .ok_or_else(|| format!("the runtime node {} holds no joint", entity.index()))?;
        match field {
            "motor_speed" => {
                no_axis(axis, "the joint motor speed")?;
                let coordinate = primary(joint.kind())
                    .ok_or_else(|| String::from("the joint carries no coordinate to drive"))?;
                if joint.motor(coordinate).is_none() {
                    return Err(String::from("the joint holds no motor"));
                }
                let speed = number(value, "the joint motor speed")?;
                joint.set_motor_speed(angle_to(Some(coordinate), speed));
            }
            _ => return Err(format!("the runtime joint holds no property {field}")),
        }
        world.insert(entity, joint);
        Ok(())
    }

    fn references(&self, kind: ReferenceKind, id: &str) -> bool {
        kind == ReferenceKind::Node
            && self
                .other
                .as_ref()
                .is_some_and(|other| other.as_str() == id)
    }

    fn remap(&mut self, prefix: &str) {
        if let Some(other) = &self.other {
            self.other = Some(NodeId::new(format!("{prefix}{other}")));
        }
    }
}

fn degrees(value: f32) -> f32 {
    value.to_degrees()
}

fn radians(value: f32) -> f32 {
    value.to_radians()
}

fn angle_of(coordinate: Option<Coordinate>, value: f32) -> f32 {
    match coordinate {
        Some(coordinate) if coordinate.angular() => value.to_degrees(),
        _ => value,
    }
}

fn angle_to(coordinate: Option<Coordinate>, value: f32) -> f32 {
    match coordinate {
        Some(coordinate) if coordinate.angular() => value.to_radians(),
        _ => value,
    }
}

fn primary(kind: JointKind) -> Option<Coordinate> {
    kind.coordinates().first().copied()
}

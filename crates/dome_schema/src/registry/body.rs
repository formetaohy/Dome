use crate::assets::Assets;
use crate::body::BodyRef;
use crate::component::SceneComponent;
use dome_ecs::{Entity, World};
use dome_math::Vec3;
use dome_physics::{BodyKind, RigidBody};
use dome_scene::Scene;
use dome_scene::field::{Axis, no_axis, number, vector_toml, with_axis};
use dome_scene::{ComponentSpec, Field, FieldKind, Reach};
use dome_scene::{Effect, Value};
use dome_toml::{Fields, Toml, floats};

static FIELDS: &[Field] = &[
    Field {
        name: "kind",
        label: "Kind",
        kind: FieldKind::Choice(&["static", "dynamic", "kinematic"]),
    },
    Field {
        name: "mass",
        label: "Mass",
        kind: FieldKind::Number {
            range: [0.0001, 1000000.0],
            speed: 0.1,
        },
    },
    Field {
        name: "gravity_scale",
        label: "Gravity Scale",
        kind: FieldKind::Number {
            range: [-10.0, 10.0],
            speed: 0.01,
        },
    },
    Field {
        name: "linear_damping",
        label: "Linear Damping",
        kind: FieldKind::Number {
            range: [0.0, 100.0],
            speed: 0.01,
        },
    },
    Field {
        name: "angular_damping",
        label: "Angular Damping",
        kind: FieldKind::Number {
            range: [0.0, 100.0],
            speed: 0.01,
        },
    },
    Field {
        name: "linear_velocity",
        label: "Linear Velocity",
        kind: FieldKind::Vector {
            range: [-1000.0, 1000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "angular_velocity",
        label: "Angular Velocity",
        kind: FieldKind::Vector {
            range: [-100.0, 100.0],
            speed: 0.01,
        },
    },
    Field {
        name: "can_sleep",
        label: "Can Sleep",
        kind: FieldKind::Bool,
    },
];

static RUNTIME_FIELDS: &[&str] = &["linear_velocity", "angular_velocity", "sleeping"];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "body",
    label: "Body",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::None,
};

impl SceneComponent for BodyRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(match field {
            "kind" => Value::Text(String::from(self.kind.name())),
            "mass" => Value::Number(self.mass),
            "gravity_scale" => Value::Number(self.gravity_scale),
            "linear_damping" => Value::Number(self.linear_damping),
            "angular_damping" => Value::Number(self.angular_damping),
            "linear_velocity" => Value::Vector(self.linear_velocity),
            "angular_velocity" => Value::Vector(self.angular_velocity),
            "can_sleep" => Value::Bool(self.can_sleep),
            "sleeping" => Value::Bool(self.sleeping),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        match field {
            "kind" => {
                no_axis(axis, "the body kind")?;
                let name = value.text()?;
                let kind =
                    BodyKind::of(name).ok_or_else(|| format!("the body holds the kind {name}"))?;
                self.kind = kind;
            }
            "mass" => {
                no_axis(axis, "the body mass")?;
                let mass = number(value, "the body mass")?;
                if mass <= 0.0 {
                    return Err(format!("the body holds the mass {mass}"));
                }
                self.mass = mass;
            }
            "gravity_scale" => {
                no_axis(axis, "the body gravity scale")?;
                self.gravity_scale = number(value, "the body gravity scale")?;
            }
            "linear_damping" => {
                no_axis(axis, "the body linear damping")?;
                let damping = number(value, "the body linear damping")?;
                if damping < 0.0 {
                    return Err(format!("the body holds the linear damping {damping}"));
                }
                self.linear_damping = damping;
            }
            "angular_damping" => {
                no_axis(axis, "the body angular damping")?;
                let damping = number(value, "the body angular damping")?;
                if damping < 0.0 {
                    return Err(format!("the body holds the angular damping {damping}"));
                }
                self.angular_damping = damping;
            }
            "linear_velocity" => {
                let velocity = with_axis(
                    self.linear_velocity,
                    axis,
                    value,
                    "the body linear velocity",
                )?;
                if !finite(velocity) {
                    return Err(format!("the body holds the linear velocity {velocity:?}"));
                }
                self.linear_velocity = velocity;
            }
            "angular_velocity" => {
                let velocity = with_axis(
                    self.angular_velocity,
                    axis,
                    value,
                    "the body angular velocity",
                )?;
                if !finite(velocity) {
                    return Err(format!("the body holds the angular velocity {velocity:?}"));
                }
                self.angular_velocity = velocity;
            }
            "can_sleep" => {
                no_axis(axis, "the body sleep")?;
                self.can_sleep = value.boolean()?;
            }
            _ => return Err(format!("the body holds no property {field}")),
        }
        Ok(Effect::Value)
    }

    fn authored(&self) -> Toml {
        Toml::object([
            ("kind", Toml::string(self.kind.name())),
            ("mass", Toml::from(self.mass)),
            ("gravity_scale", Toml::from(self.gravity_scale)),
            ("linear_damping", Toml::from(self.linear_damping)),
            ("angular_damping", Toml::from(self.angular_damping)),
            (
                "linear_velocity",
                vector_toml(self.linear_velocity.to_array()),
            ),
            (
                "angular_velocity",
                vector_toml(self.angular_velocity.to_array()),
            ),
            ("can_sleep", Toml::boolean(self.can_sleep)),
        ])
    }

    fn observed(&self) -> Toml {
        self.authored()
    }

    fn runtime(&self) -> Toml {
        Toml::object([
            (
                "linear_velocity",
                vector_toml(self.linear_velocity.to_array()),
            ),
            (
                "angular_velocity",
                vector_toml(self.angular_velocity.to_array()),
            ),
            ("sleeping", Toml::boolean(self.sleeping)),
        ])
    }

    fn install(
        &self,
        _scene: &dome_scene::Scene,
        _node: &str,
        world: &mut World,
        entity: Entity,
        _assets: &dyn Assets,
    ) {
        world.insert(entity, self.rigid());
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        if !(self.mass.is_finite() && self.mass > 0.0) {
            return Err(format!(
                "the body of node {node} holds the mass {}",
                self.mass
            ));
        }
        if !self.gravity_scale.is_finite() {
            return Err(format!(
                "the body of node {node} holds the gravity scale {}",
                self.gravity_scale,
            ));
        }
        for (name, damping) in [
            ("linear", self.linear_damping),
            ("angular", self.angular_damping),
        ] {
            if !(damping.is_finite() && damping >= 0.0) {
                return Err(format!(
                    "the body of node {node} holds the {name} damping {damping}",
                ));
            }
        }
        for (name, velocity) in [
            ("linear", self.linear_velocity),
            ("angular", self.angular_velocity),
        ] {
            if !finite(velocity) {
                return Err(format!(
                    "the body of node {node} holds the {name} velocity {velocity:?}",
                ));
            }
        }
        let index = scene
            .node_index(node)
            .ok_or_else(|| format!("the scene holds no node {node}"))?;
        let held = &scene.nodes()[index];
        for scale in [
            scene.transform(index).scale,
            scene.world_transform(index).scale,
        ] {
            let tolerance = 1e-4 * scale.x.abs().max(scale.y.abs()).max(scale.z.abs());
            if !(scale.x > 0.0
                && (scale.x - scale.y).abs() <= tolerance
                && (scale.x - scale.z).abs() <= tolerance)
            {
                return Err(format!("the body of node {node} holds the scale {scale:?}",));
            }
        }
        if self.kind != BodyKind::Static && held.parent.is_some() {
            return Err(format!(
                "the movable body of node {node} follows the parent {}",
                held.parent.as_deref().unwrap_or_default(),
            ));
        }
        for component in ["body", "joint", "collider"] {
            if scene.ancestors_hold(index, component) {
                return Err(format!(
                    "the body of node {node} sits under the {component} of a parent",
                ));
            }
        }
        if held.has_component("animation") {
            return Err(format!(
                "the body of node {node} shares its transform with an animation",
            ));
        }
        if self.kind == BodyKind::Dynamic && !scene.subtree_holds(index, "collider") {
            return Err(format!("the dynamic body of node {node} holds no collider"));
        }
        Ok(())
    }

    fn create(_scene: &Scene) -> Result<Self, String> {
        Ok(Self::new(BodyKind::Dynamic))
    }

    fn from_toml(value: Toml, what: &str) -> Self {
        let mut fields = Fields::of(value, what);
        let kind = fields
            .need("kind")
            .string_value(&format!("the kind of {what}"));
        let kind = BodyKind::of(&kind).unwrap_or_else(|| panic!("{what} holds the kind {kind}"));
        let mass = fields
            .need("mass")
            .f32_value(&format!("the mass of {what}"));
        let gravity_scale = fields
            .need("gravity_scale")
            .f32_value(&format!("the gravity scale of {what}"));
        let linear_damping = fields
            .need("linear_damping")
            .f32_value(&format!("the linear damping of {what}"));
        let angular_damping = fields
            .need("angular_damping")
            .f32_value(&format!("the angular damping of {what}"));
        let linear_velocity = Vec3::from_array(floats::<3>(
            fields.need("linear_velocity"),
            &format!("the linear velocity of {what}"),
        ));
        let angular_velocity = Vec3::from_array(floats::<3>(
            fields.need("angular_velocity"),
            &format!("the angular velocity of {what}"),
        ));
        let can_sleep = fields
            .need("can_sleep")
            .boolean_value(&format!("the sleep of {what}"));
        fields.done();
        Self {
            kind,
            mass,
            gravity_scale,
            linear_damping,
            angular_damping,
            linear_velocity,
            angular_velocity,
            can_sleep,
            sleeping: false,
        }
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        world.get::<RigidBody>(entity).map(BodyRef::observe)
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<RigidBody>(entity);
    }

    fn write_world(
        world: &mut World,
        entity: Entity,
        field: &str,
        axis: Option<Axis>,
        value: &Value,
    ) -> Result<(), String> {
        let body = world
            .get_mut::<RigidBody>(entity)
            .ok_or_else(|| format!("the runtime node {} holds no body", entity.index()))?;
        match field {
            "linear_velocity" => {
                let velocity = with_axis(
                    body.linear_velocity(),
                    axis,
                    value,
                    "the body linear velocity",
                )?;
                if !finite(velocity) {
                    return Err(format!("the body holds the linear velocity {velocity:?}"));
                }
                body.set_linear_velocity(velocity);
            }
            "angular_velocity" => {
                let velocity = with_axis(
                    body.angular_velocity(),
                    axis,
                    value,
                    "the body angular velocity",
                )?;
                if !finite(velocity) {
                    return Err(format!("the body holds the angular velocity {velocity:?}"));
                }
                body.set_angular_velocity(velocity);
            }
            "sleeping" => {
                no_axis(axis, "the body sleep")?;
                body.set_sleeping(value.boolean()?);
            }
            _ => return Err(format!("the runtime body holds no property {field}")),
        }
        Ok(())
    }
}

fn finite(vector: Vec3) -> bool {
    vector.x.is_finite() && vector.y.is_finite() && vector.z.is_finite()
}

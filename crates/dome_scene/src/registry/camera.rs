use super::{Assets, SceneComponent};
use crate::field::{Axis, no_axis, positive};
use crate::scene::Scene;
use crate::schema::{ComponentSpec, Field, FieldKind, Reach};
use crate::value::{Effect, Value};
use dome_ecs::{Entity, World};
use dome_render::Camera;
use dome_toml::{Fields, Toml};

static FIELDS: &[Field] = &[
    Field {
        name: "vertical_fov",
        label: "FOV",
        kind: FieldKind::Number {
            range: [1.0, 179.0],
            speed: 0.5,
        },
    },
    Field {
        name: "near",
        label: "Near",
        kind: FieldKind::Number {
            range: [0.0001, 1000.0],
            speed: 0.001,
        },
    },
    Field {
        name: "far",
        label: "Far",
        kind: FieldKind::Number {
            range: [0.1, 100000.0],
            speed: 1.0,
        },
    },
];

static RUNTIME_FIELDS: &[&str] = &["vertical_fov", "near", "far"];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "camera",
    label: "Camera",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::None,
};

impl SceneComponent for Camera {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(Value::Number(match field {
            "vertical_fov" => self.vertical_fov.to_degrees(),
            "near" => self.near,
            "far" => self.far,
            _ => return None,
        }))
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        no_axis(axis, &format!("the camera {field}"))?;
        let number = value.number()?;
        let mut fov = self.vertical_fov.to_degrees();
        let mut near = self.near;
        let mut far = self.far;
        match field {
            "vertical_fov" => fov = number,
            "near" => near = positive(number, "the camera near")?,
            "far" => far = positive(number, "the camera far")?,
            _ => return Err(format!("the camera holds no property {field}")),
        }
        if !(1.0..=179.0).contains(&fov) {
            return Err(format!("the camera holds fov {fov}"));
        }
        if near >= far {
            return Err(format!("the camera holds near {near} and far {far}"));
        }
        *self = Camera::new(fov.to_radians(), near, far);
        Ok(Effect::Value)
    }

    fn authored(&self) -> Toml {
        Toml::object([
            ("vertical_fov", Toml::from(self.vertical_fov)),
            ("near", Toml::from(self.near)),
            ("far", Toml::from(self.far)),
        ])
    }

    fn observed(&self) -> Toml {
        Toml::object([
            ("vertical_fov", Toml::from(self.vertical_fov.to_degrees())),
            ("near", Toml::from(self.near)),
            ("far", Toml::from(self.far)),
        ])
    }

    fn install(
        &self,
        _scene: &crate::scene::Scene,
        _node: &str,
        world: &mut World,
        entity: Entity,
        _assets: &dyn Assets,
    ) {
        world.insert(entity, *self);
    }

    fn create(_scene: &Scene) -> Result<Self, String> {
        Ok(Self::new(1.0, 0.1, 1000.0))
    }

    fn from_toml(value: Toml, what: &str) -> Self {
        let mut fields = Fields::of(value, what);
        let vertical_fov = fields
            .need("vertical_fov")
            .f32_value(&format!("the vertical fov of {what}"));
        let near = fields
            .need("near")
            .f32_value(&format!("the near of {what}"));
        let far = fields.need("far").f32_value(&format!("the far of {what}"));
        fields.done();
        Self::new(vertical_fov, near, far)
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        world.get::<Camera>(entity).copied()
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<Camera>(entity);
    }

    fn write_world(
        world: &mut World,
        entity: Entity,
        field: &str,
        axis: Option<Axis>,
        value: &Value,
    ) -> Result<(), String> {
        let mut camera = *world
            .get::<Camera>(entity)
            .ok_or_else(|| format!("the runtime node {} holds no camera", entity.index()))?;
        camera.write(field, axis, value)?;
        world.insert(entity, camera);
        Ok(())
    }
}

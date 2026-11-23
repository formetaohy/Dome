use crate::assets::Assets;
use crate::component::SceneComponent;
use dome_ecs::{Entity, World};
use dome_math::Vec3;
use dome_render::PointLight;
use dome_scene::Scene;
use dome_scene::field::{Axis, no_axis, positive, unit, vector_toml, with_axis};
use dome_scene::{ComponentSpec, Field, FieldKind, Reach};
use dome_scene::{Effect, Value};
use dome_toml::{Fields, Toml, floats};

static FIELDS: &[Field] = &[
    Field {
        name: "color",
        label: "Color",
        kind: FieldKind::Color,
    },
    Field {
        name: "intensity",
        label: "Intensity",
        kind: FieldKind::Number {
            range: [0.001, 100000.0],
            speed: 0.05,
        },
    },
    Field {
        name: "range",
        label: "Range",
        kind: FieldKind::Number {
            range: [0.001, 100000.0],
            speed: 0.05,
        },
    },
];

static RUNTIME_FIELDS: &[&str] = &["color", "intensity", "range"];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "point_light",
    label: "Point Light",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::World,
};

impl SceneComponent for PointLight {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(match field {
            "color" => Value::Vector(self.color),
            "intensity" => Value::Number(self.intensity),
            "range" => Value::Number(self.range),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        let mut color = self.color;
        let mut intensity = self.intensity;
        let mut range = self.range;
        match field {
            "color" => {
                color = with_axis(color, axis, value, "the light color")?;
                unit(color, "the light color")?;
            }
            "intensity" => {
                no_axis(axis, "the light intensity")?;
                intensity = positive(value.number()?, "the light intensity")?;
            }
            "range" => {
                no_axis(axis, "the light range")?;
                range = positive(value.number()?, "the light range")?;
            }
            _ => return Err(format!("the point light holds no property {field}")),
        }
        *self = Self::new(color, intensity, range);
        Ok(Effect::Value)
    }

    fn authored(&self) -> Toml {
        Toml::object([
            ("color", vector_toml(self.color.to_array())),
            ("intensity", Toml::from(self.intensity)),
            ("range", Toml::from(self.range)),
        ])
    }

    fn observed(&self) -> Toml {
        self.authored()
    }

    fn install(
        &self,
        _scene: &dome_scene::Scene,
        _node: &str,
        world: &mut World,
        entity: Entity,
        _assets: &dyn Assets,
    ) {
        world.insert(entity, *self);
    }

    fn create(_scene: &Scene) -> Result<Self, String> {
        Ok(Self::new(Vec3::ONE, 20.0, 10.0))
    }

    fn from_toml(value: Toml, what: &str) -> Self {
        let mut fields = Fields::of(value, what);
        let color = Vec3::from_array(floats::<3>(
            fields.need("color"),
            &format!("the color of {what}"),
        ));
        let intensity = fields
            .need("intensity")
            .f32_value(&format!("the intensity of {what}"));
        let range = fields
            .need("range")
            .f32_value(&format!("the range of {what}"));
        fields.done();
        Self::new(color, intensity, range)
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        world.get::<Self>(entity).copied()
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<Self>(entity);
    }

    fn write_world(
        world: &mut World,
        entity: Entity,
        field: &str,
        axis: Option<Axis>,
        value: &Value,
    ) -> Result<(), String> {
        let mut light = *world
            .get::<Self>(entity)
            .ok_or_else(|| format!("the runtime node {} holds no point light", entity.index()))?;
        light.write(field, axis, value)?;
        world.insert(entity, light);
        Ok(())
    }
}

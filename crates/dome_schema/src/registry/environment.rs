use crate::assets::Assets;
use crate::component::SceneComponent;
use crate::environment::EnvironmentRef;
use dome_ecs::{Entity, World};
use dome_render::Environment;
use dome_scene::EnvironmentId;
use dome_scene::Scene;
use dome_scene::SceneValue;
use dome_scene::field::{Axis, no_axis, positive};
use dome_scene::{ComponentSpec, Field, FieldKind, Reach, ReferenceKind};
use dome_scene::{Effect, Value};
use dome_toml::{Fields, Toml};

static FIELDS: &[Field] = &[
    Field {
        name: "environment",
        label: "Environment",
        kind: FieldKind::Reference(ReferenceKind::Environment),
    },
    Field {
        name: "intensity",
        label: "Intensity",
        kind: FieldKind::Number {
            range: [0.0001, 100.0],
            speed: 0.01,
        },
    },
    Field {
        name: "sky",
        label: "Sky",
        kind: FieldKind::Bool,
    },
    Field {
        name: "local",
        label: "Local Probe",
        kind: FieldKind::Bool,
    },
    Field {
        name: "blend",
        label: "Blend",
        kind: FieldKind::Number {
            range: [0.0001, 10000.0],
            speed: 0.05,
        },
    },
];

static RUNTIME_FIELDS: &[&str] = &["intensity", "sky", "local", "blend"];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "environment",
    label: "Environment Probe",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::World,
};

impl SceneComponent for EnvironmentRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(match field {
            "environment" => Value::Text(String::from(self.environment.as_str())),
            "intensity" => Value::Number(self.intensity),
            "sky" => Value::Bool(self.sky),
            "local" => Value::Bool(self.local),
            "blend" => Value::Number(self.blend),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        match field {
            "environment" => {
                self.environment =
                    EnvironmentId::write(&self.environment, axis, value, "the environment")?;
                Ok(Effect::Structure)
            }
            "intensity" => {
                no_axis(axis, "the environment intensity")?;
                self.intensity = positive(value.number()?, "the environment intensity")?;
                Ok(Effect::Value)
            }
            "sky" => {
                no_axis(axis, "the environment sky")?;
                self.sky = value.boolean()?;
                Ok(Effect::Value)
            }
            "local" => {
                no_axis(axis, "the environment local")?;
                self.local = value.boolean()?;
                Ok(Effect::Value)
            }
            "blend" => {
                no_axis(axis, "the environment blend")?;
                self.blend = positive(value.number()?, "the environment blend")?;
                Ok(Effect::Value)
            }
            _ => Err(format!("the environment holds no property {field}")),
        }
    }

    fn authored(&self) -> Toml {
        Toml::object([
            ("environment", Toml::string(self.environment.as_str())),
            ("intensity", Toml::from(self.intensity)),
            ("sky", Toml::boolean(self.sky)),
            ("local", Toml::boolean(self.local)),
            ("blend", Toml::from(self.blend)),
        ])
    }

    fn observed(&self) -> Toml {
        self.authored()
    }

    fn install(
        &self,
        _scene: &Scene,
        _node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    ) {
        let environment = assets.environment(self.environment.as_str());
        let intensity = environment.intensity * self.intensity;
        let environment = environment.lit(intensity, self.sky);
        let environment = if self.local {
            environment.local(self.blend)
        } else {
            environment
        };
        world.insert(entity, environment);
        if let Some(probe) = assets.capture(self.environment.as_str()) {
            world.insert(entity, probe);
        }
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        self.environment
            .validate(scene, &format!("the environment of node {node}"))?;
        positive(self.intensity, &format!("the environment of node {node}"))?;
        positive(self.blend, &format!("the environment blend of node {node}"))?;
        Ok(())
    }

    fn create(scene: &Scene) -> Result<Self, String> {
        let environment = EnvironmentId::new(
            scene
                .environments()
                .first()
                .ok_or_else(|| String::from("the scene holds no environment to light with"))?
                .id
                .as_str(),
        );
        Ok(Self::new(environment))
    }

    fn from_toml(value: Toml, what: &str) -> Result<Self, String> {
        let mut fields = Fields::of(value, what)?;
        let environment = EnvironmentId::parse(
            fields.need("environment")?,
            &format!("the environment of {what}"),
        )?;
        let intensity = fields
            .need("intensity")?
            .f32_value(&format!("the intensity of {what}"))?;
        let sky = fields
            .need("sky")?
            .boolean_value(&format!("the sky of {what}"))?;
        let local = fields
            .need("local")?
            .boolean_value(&format!("the local of {what}"))?;
        let blend = fields
            .need("blend")?
            .f32_value(&format!("the blend of {what}"))?;
        fields.done()?;
        Ok(Self {
            environment,
            intensity,
            sky,
            local,
            blend,
        })
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        let held = world.get::<Environment>(entity)?;
        Some(Self {
            environment: EnvironmentId::default(),
            intensity: held.intensity,
            sky: held.background,
            local: held.local,
            blend: held.blend,
        })
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<Environment>(entity);
        world.remove::<dome_render::CaptureProbe>(entity);
    }

    fn references(&self, kind: ReferenceKind, id: &str) -> bool {
        kind == ReferenceKind::Environment && self.environment.as_str() == id
    }

    fn write_world(
        world: &mut World,
        entity: Entity,
        field: &str,
        axis: Option<Axis>,
        value: &Value,
    ) -> Result<(), String> {
        let mut environment = world
            .get::<Environment>(entity)
            .cloned()
            .ok_or_else(|| format!("the runtime node {} holds no environment", entity.index()))?;
        match field {
            "intensity" => {
                no_axis(axis, "the environment intensity")?;
                let intensity = positive(value.number()?, "the environment intensity")?;
                let sky = environment.background;
                environment = environment.lit(intensity, sky);
            }
            "sky" => {
                no_axis(axis, "the environment sky")?;
                let sky = value.boolean()?;
                let intensity = environment.intensity;
                environment = environment.lit(intensity, sky);
            }
            "local" => {
                no_axis(axis, "the environment local")?;
                environment.local = value.boolean()?;
            }
            "blend" => {
                no_axis(axis, "the environment blend")?;
                environment.blend = positive(value.number()?, "the environment blend")?;
            }
            _ => return Err(format!("the runtime environment holds no property {field}")),
        }
        world.insert(entity, environment);
        Ok(())
    }
}

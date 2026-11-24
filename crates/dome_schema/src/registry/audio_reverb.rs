use super::audio_bus::require_bus;
use crate::assets::Assets;
use crate::component::SceneComponent;
use dome_audio::AudioReverb as Held;
use dome_ecs::{Entity, World};
use dome_scene::Scene;
use dome_scene::field::{Axis, no_axis, number};
use dome_scene::{ComponentSpec, Field, FieldKind, Reach};
use dome_scene::{Effect, Value};
use dome_toml::{Fields, Toml};

static FIELDS: &[Field] = &[
    Field {
        name: "enabled",
        label: "Enabled",
        kind: FieldKind::Bool,
    },
    Field {
        name: "room",
        label: "Room",
        kind: FieldKind::Number {
            range: [0.0, 1.0],
            speed: 0.005,
        },
    },
    Field {
        name: "damping",
        label: "Damping",
        kind: FieldKind::Number {
            range: [0.0, 1.0],
            speed: 0.005,
        },
    },
    Field {
        name: "mix",
        label: "Mix",
        kind: FieldKind::Number {
            range: [0.0, 1.0],
            speed: 0.005,
        },
    },
];

static RUNTIME_FIELDS: &[&str] = &["enabled", "room", "damping", "mix"];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "audio_reverb",
    label: "Audio Reverb",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::None,
};

#[derive(Clone, Debug, PartialEq)]
pub struct AudioReverbRef {
    pub enabled: bool,
    pub room: f32,
    pub damping: f32,
    pub mix: f32,
}

impl Default for AudioReverbRef {
    fn default() -> Self {
        let reverb = Held::default();
        Self::of(&reverb)
    }
}

impl AudioReverbRef {
    fn of(reverb: &Held) -> Self {
        Self {
            enabled: reverb.enabled(),
            room: reverb.room(),
            damping: reverb.damping(),
            mix: reverb.mix(),
        }
    }
}

impl SceneComponent for AudioReverbRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(match field {
            "enabled" => Value::Bool(self.enabled),
            "room" => Value::Number(self.room),
            "damping" => Value::Number(self.damping),
            "mix" => Value::Number(self.mix),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        match field {
            "enabled" => {
                no_axis(axis, "the audio reverb enabled")?;
                self.enabled = value.boolean()?;
            }
            "room" => {
                no_axis(axis, "the audio reverb room")?;
                let room = number(value, "the audio reverb room")?;
                check(room, "room")?;
                self.room = room;
            }
            "damping" => {
                no_axis(axis, "the audio reverb damping")?;
                let damping = number(value, "the audio reverb damping")?;
                check(damping, "damping")?;
                self.damping = damping;
            }
            "mix" => {
                no_axis(axis, "the audio reverb mix")?;
                let mix = number(value, "the audio reverb mix")?;
                check(mix, "mix")?;
                self.mix = mix;
            }
            _ => return Err(format!("the audio reverb holds no property {field}")),
        }
        Ok(Effect::Value)
    }

    fn authored(&self) -> Toml {
        Toml::object([
            ("enabled", Toml::boolean(self.enabled)),
            ("room", Toml::from(self.room)),
            ("damping", Toml::from(self.damping)),
            ("mix", Toml::from(self.mix)),
        ])
    }

    fn observed(&self) -> Toml {
        self.authored()
    }

    fn runtime(&self) -> Toml {
        self.authored()
    }

    fn install(
        &self,
        _scene: &Scene,
        _node: &str,
        world: &mut World,
        entity: Entity,
        _assets: &dyn Assets,
    ) {
        world.insert(
            entity,
            Held::new(self.enabled, self.room, self.damping, self.mix),
        );
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        require_bus(scene, node, "audio reverb")?;
        check(self.room, "room")?;
        check(self.damping, "damping")?;
        check(self.mix, "mix")?;
        Ok(())
    }

    fn create(_scene: &Scene) -> Result<Self, String> {
        Ok(Self::default())
    }

    fn from_toml(value: Toml, what: &str) -> Result<Self, String> {
        let mut fields = Fields::of(value, what)?;
        let enabled = fields
            .need("enabled")?
            .boolean_value(&format!("the enabled state of {what}"))?;
        let room = fields
            .need("room")?
            .f32_value(&format!("the room of {what}"))?;
        let damping = fields
            .need("damping")?
            .f32_value(&format!("the damping of {what}"))?;
        let mix = fields
            .need("mix")?
            .f32_value(&format!("the mix of {what}"))?;
        fields.done()?;
        Ok(Self {
            enabled,
            room,
            damping,
            mix,
        })
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        world.get::<Held>(entity).map(Self::of)
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<Held>(entity);
    }

    fn write_world(
        world: &mut World,
        entity: Entity,
        field: &str,
        axis: Option<Axis>,
        value: &Value,
    ) -> Result<(), String> {
        let mut reverb = world
            .get::<Held>(entity)
            .copied()
            .ok_or_else(|| format!("the runtime node {} holds no audio reverb", entity.index(),))?;
        match field {
            "enabled" => {
                no_axis(axis, "the audio reverb enabled")?;
                reverb.set_enabled(value.boolean()?);
            }
            "room" => {
                no_axis(axis, "the audio reverb room")?;
                let room = number(value, "the audio reverb room")?;
                check(room, "room")?;
                reverb.set_room(room);
            }
            "damping" => {
                no_axis(axis, "the audio reverb damping")?;
                let damping = number(value, "the audio reverb damping")?;
                check(damping, "damping")?;
                reverb.set_damping(damping);
            }
            "mix" => {
                no_axis(axis, "the audio reverb mix")?;
                let mix = number(value, "the audio reverb mix")?;
                check(mix, "mix")?;
                reverb.set_mix(mix);
            }
            _ => {
                return Err(format!(
                    "the runtime audio reverb holds no property {field}",
                ));
            }
        }
        world.insert(entity, reverb);
        Ok(())
    }
}

fn check(value: f32, name: &str) -> Result<(), String> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(format!("the audio reverb holds the {name} {value}"))
    }
}

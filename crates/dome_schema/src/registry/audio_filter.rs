use super::audio_bus::require_bus;
use crate::assets::Assets;
use crate::component::SceneComponent;
use dome_audio::{AudioFilter as Held, FilterKind};
use dome_ecs::{Entity, World};
use dome_scene::Scene;
use dome_scene::field::{Axis, no_axis, number};
use dome_scene::{ComponentSpec, Field, FieldKind, Reach};
use dome_scene::{Effect, Value};
use dome_toml::{Fields, Toml};

const KINDS: &[&str] = &[
    "low_pass",
    "high_pass",
    "band_pass",
    "peaking",
    "low_shelf",
    "high_shelf",
];

static FIELDS: &[Field] = &[
    Field {
        name: "enabled",
        label: "Enabled",
        kind: FieldKind::Bool,
    },
    Field {
        name: "kind",
        label: "Kind",
        kind: FieldKind::Choice(KINDS),
    },
    Field {
        name: "frequency",
        label: "Frequency",
        kind: FieldKind::Number {
            range: [20.0, 20_000.0],
            speed: 10.0,
        },
    },
    Field {
        name: "q",
        label: "Q",
        kind: FieldKind::Number {
            range: [0.1, 10.0],
            speed: 0.01,
        },
    },
    Field {
        name: "gain_db",
        label: "Gain (dB)",
        kind: FieldKind::Number {
            range: [-24.0, 24.0],
            speed: 0.1,
        },
    },
];

static RUNTIME_FIELDS: &[&str] = &["enabled", "kind", "frequency", "q", "gain_db"];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "audio_filter",
    label: "Audio Filter",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::None,
};

#[derive(Clone, Debug, PartialEq)]
pub struct AudioFilterRef {
    pub enabled: bool,
    pub kind: FilterKind,
    pub frequency: f32,
    pub q: f32,
    pub gain_db: f32,
}

impl Default for AudioFilterRef {
    fn default() -> Self {
        let filter = Held::default();
        Self {
            enabled: filter.enabled(),
            kind: filter.kind(),
            frequency: filter.frequency(),
            q: filter.q(),
            gain_db: filter.gain_db(),
        }
    }
}

impl AudioFilterRef {
    fn of(filter: &Held) -> Self {
        Self {
            enabled: filter.enabled(),
            kind: filter.kind(),
            frequency: filter.frequency(),
            q: filter.q(),
            gain_db: filter.gain_db(),
        }
    }
}

impl SceneComponent for AudioFilterRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(match field {
            "enabled" => Value::Bool(self.enabled),
            "kind" => Value::Text(String::from(self.kind.name())),
            "frequency" => Value::Number(self.frequency),
            "q" => Value::Number(self.q),
            "gain_db" => Value::Number(self.gain_db),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        match field {
            "enabled" => {
                no_axis(axis, "the audio filter enabled")?;
                self.enabled = value.boolean()?;
            }
            "kind" => {
                no_axis(axis, "the audio filter kind")?;
                let name = value.text()?;
                self.kind =
                    FilterKind::of(name).ok_or_else(|| format!("the audio filter holds {name}"))?;
            }
            "frequency" => {
                no_axis(axis, "the audio filter frequency")?;
                let frequency = number(value, "the audio filter frequency")?;
                check_frequency(frequency)?;
                self.frequency = frequency;
            }
            "q" => {
                no_axis(axis, "the audio filter q")?;
                let q = number(value, "the audio filter q")?;
                check_q(q)?;
                self.q = q;
            }
            "gain_db" => {
                no_axis(axis, "the audio filter gain")?;
                let gain = number(value, "the audio filter gain")?;
                check_gain(gain)?;
                self.gain_db = gain;
            }
            _ => return Err(format!("the audio filter holds no property {field}")),
        }
        Ok(Effect::Value)
    }

    fn authored(&self) -> Toml {
        Toml::object([
            ("enabled", Toml::boolean(self.enabled)),
            ("kind", Toml::string(self.kind.name())),
            ("frequency", Toml::from(self.frequency)),
            ("q", Toml::from(self.q)),
            ("gain_db", Toml::from(self.gain_db)),
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
            Held::new(
                self.enabled,
                self.kind,
                self.frequency,
                self.q,
                self.gain_db,
            ),
        );
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        require_bus(scene, node, "audio filter")?;
        check_frequency(self.frequency)?;
        check_q(self.q)?;
        check_gain(self.gain_db)?;
        Ok(())
    }

    fn create(_scene: &Scene) -> Result<Self, String> {
        Ok(Self::default())
    }

    fn from_toml(value: Toml, what: &str) -> Self {
        let mut fields = Fields::of(value, what);
        let enabled = fields
            .need("enabled")
            .boolean_value(&format!("the enabled state of {what}"));
        let kind = fields
            .need("kind")
            .string_value(&format!("the kind of {what}"));
        let kind = FilterKind::of(&kind).unwrap_or_else(|| panic!("{what} holds the kind {kind}"));
        let frequency = fields
            .need("frequency")
            .f32_value(&format!("the frequency of {what}"));
        let q = fields.need("q").f32_value(&format!("the q of {what}"));
        let gain_db = fields
            .need("gain_db")
            .f32_value(&format!("the gain of {what}"));
        fields.done();
        Self {
            enabled,
            kind,
            frequency,
            q,
            gain_db,
        }
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
        let mut filter = world
            .get::<Held>(entity)
            .copied()
            .ok_or_else(|| format!("the runtime node {} holds no audio filter", entity.index(),))?;
        match field {
            "enabled" => {
                no_axis(axis, "the audio filter enabled")?;
                filter.set_enabled(value.boolean()?);
            }
            "kind" => {
                no_axis(axis, "the audio filter kind")?;
                let name = value.text()?;
                filter.set_kind(
                    FilterKind::of(name).ok_or_else(|| format!("the audio filter holds {name}"))?,
                );
            }
            "frequency" => {
                no_axis(axis, "the audio filter frequency")?;
                let frequency = number(value, "the audio filter frequency")?;
                check_frequency(frequency)?;
                filter.set_frequency(frequency);
            }
            "q" => {
                no_axis(axis, "the audio filter q")?;
                let q = number(value, "the audio filter q")?;
                check_q(q)?;
                filter.set_q(q);
            }
            "gain_db" => {
                no_axis(axis, "the audio filter gain")?;
                let gain = number(value, "the audio filter gain")?;
                check_gain(gain)?;
                filter.set_gain_db(gain);
            }
            _ => {
                return Err(format!(
                    "the runtime audio filter holds no property {field}",
                ));
            }
        }
        world.insert(entity, filter);
        Ok(())
    }
}

fn check_frequency(frequency: f32) -> Result<(), String> {
    if frequency.is_finite() && (20.0..=20_000.0).contains(&frequency) {
        Ok(())
    } else {
        Err(format!("the audio filter holds the frequency {frequency}"))
    }
}

fn check_q(q: f32) -> Result<(), String> {
    if q.is_finite() && (0.1..=10.0).contains(&q) {
        Ok(())
    } else {
        Err(format!("the audio filter holds the q {q}"))
    }
}

fn check_gain(gain: f32) -> Result<(), String> {
    if gain.is_finite() && (-24.0..=24.0).contains(&gain) {
        Ok(())
    } else {
        Err(format!("the audio filter holds the gain {gain} decibels"))
    }
}

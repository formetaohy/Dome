use super::audio_bus::require_bus;
use crate::assets::Assets;
use crate::component::SceneComponent;
use dome_audio::AudioCompressor as Held;
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
        name: "threshold_db",
        label: "Threshold (dB)",
        kind: FieldKind::Number {
            range: [-60.0, 0.0],
            speed: 0.1,
        },
    },
    Field {
        name: "ratio",
        label: "Ratio",
        kind: FieldKind::Number {
            range: [1.0, 20.0],
            speed: 0.05,
        },
    },
    Field {
        name: "attack_ms",
        label: "Attack (ms)",
        kind: FieldKind::Number {
            range: [0.1, 1_000.0],
            speed: 0.5,
        },
    },
    Field {
        name: "release_ms",
        label: "Release (ms)",
        kind: FieldKind::Number {
            range: [1.0, 5_000.0],
            speed: 5.0,
        },
    },
    Field {
        name: "makeup_db",
        label: "Makeup (dB)",
        kind: FieldKind::Number {
            range: [0.0, 24.0],
            speed: 0.1,
        },
    },
];

static RUNTIME_FIELDS: &[&str] = &[
    "enabled",
    "threshold_db",
    "ratio",
    "attack_ms",
    "release_ms",
    "makeup_db",
];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "audio_compressor",
    label: "Audio Compressor",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::None,
};

#[derive(Clone, Debug, PartialEq)]
pub struct AudioCompressorRef {
    pub enabled: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_db: f32,
}

impl Default for AudioCompressorRef {
    fn default() -> Self {
        let compressor = Held::default();
        Self::of(&compressor)
    }
}

impl AudioCompressorRef {
    fn of(compressor: &Held) -> Self {
        Self {
            enabled: compressor.enabled(),
            threshold_db: compressor.threshold_db(),
            ratio: compressor.ratio(),
            attack_ms: compressor.attack_ms(),
            release_ms: compressor.release_ms(),
            makeup_db: compressor.makeup_db(),
        }
    }
}

impl SceneComponent for AudioCompressorRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(match field {
            "enabled" => Value::Bool(self.enabled),
            "threshold_db" => Value::Number(self.threshold_db),
            "ratio" => Value::Number(self.ratio),
            "attack_ms" => Value::Number(self.attack_ms),
            "release_ms" => Value::Number(self.release_ms),
            "makeup_db" => Value::Number(self.makeup_db),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        match field {
            "enabled" => {
                no_axis(axis, "the audio compressor enabled")?;
                self.enabled = value.boolean()?;
            }
            "threshold_db" => {
                no_axis(axis, "the audio compressor threshold")?;
                let threshold = number(value, "the audio compressor threshold")?;
                check_threshold(threshold)?;
                self.threshold_db = threshold;
            }
            "ratio" => {
                no_axis(axis, "the audio compressor ratio")?;
                let ratio = number(value, "the audio compressor ratio")?;
                check_ratio(ratio)?;
                self.ratio = ratio;
            }
            "attack_ms" => {
                no_axis(axis, "the audio compressor attack")?;
                let attack = number(value, "the audio compressor attack")?;
                check_attack(attack)?;
                self.attack_ms = attack;
            }
            "release_ms" => {
                no_axis(axis, "the audio compressor release")?;
                let release = number(value, "the audio compressor release")?;
                check_release(release)?;
                self.release_ms = release;
            }
            "makeup_db" => {
                no_axis(axis, "the audio compressor makeup")?;
                let makeup = number(value, "the audio compressor makeup")?;
                check_makeup(makeup)?;
                self.makeup_db = makeup;
            }
            _ => return Err(format!("the audio compressor holds no property {field}")),
        }
        Ok(Effect::Value)
    }

    fn authored(&self) -> Toml {
        Toml::object([
            ("enabled", Toml::boolean(self.enabled)),
            ("threshold_db", Toml::from(self.threshold_db)),
            ("ratio", Toml::from(self.ratio)),
            ("attack_ms", Toml::from(self.attack_ms)),
            ("release_ms", Toml::from(self.release_ms)),
            ("makeup_db", Toml::from(self.makeup_db)),
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
                self.threshold_db,
                self.ratio,
                self.attack_ms,
                self.release_ms,
                self.makeup_db,
            ),
        );
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        require_bus(scene, node, "audio compressor")?;
        check_threshold(self.threshold_db)?;
        check_ratio(self.ratio)?;
        check_attack(self.attack_ms)?;
        check_release(self.release_ms)?;
        check_makeup(self.makeup_db)?;
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
        let threshold_db = fields
            .need("threshold_db")?
            .f32_value(&format!("the threshold of {what}"))?;
        let ratio = fields
            .need("ratio")?
            .f32_value(&format!("the ratio of {what}"))?;
        let attack_ms = fields
            .need("attack_ms")?
            .f32_value(&format!("the attack of {what}"))?;
        let release_ms = fields
            .need("release_ms")?
            .f32_value(&format!("the release of {what}"))?;
        let makeup_db = fields
            .need("makeup_db")?
            .f32_value(&format!("the makeup of {what}"))?;
        fields.done()?;
        Ok(Self {
            enabled,
            threshold_db,
            ratio,
            attack_ms,
            release_ms,
            makeup_db,
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
        let mut compressor = world.get::<Held>(entity).copied().ok_or_else(|| {
            format!(
                "the runtime node {} holds no audio compressor",
                entity.index(),
            )
        })?;
        match field {
            "enabled" => {
                no_axis(axis, "the audio compressor enabled")?;
                compressor.set_enabled(value.boolean()?);
            }
            "threshold_db" => {
                no_axis(axis, "the audio compressor threshold")?;
                let threshold = number(value, "the audio compressor threshold")?;
                check_threshold(threshold)?;
                compressor.set_threshold_db(threshold);
            }
            "ratio" => {
                no_axis(axis, "the audio compressor ratio")?;
                let ratio = number(value, "the audio compressor ratio")?;
                check_ratio(ratio)?;
                compressor.set_ratio(ratio);
            }
            "attack_ms" => {
                no_axis(axis, "the audio compressor attack")?;
                let attack = number(value, "the audio compressor attack")?;
                check_attack(attack)?;
                compressor.set_attack_ms(attack);
            }
            "release_ms" => {
                no_axis(axis, "the audio compressor release")?;
                let release = number(value, "the audio compressor release")?;
                check_release(release)?;
                compressor.set_release_ms(release);
            }
            "makeup_db" => {
                no_axis(axis, "the audio compressor makeup")?;
                let makeup = number(value, "the audio compressor makeup")?;
                check_makeup(makeup)?;
                compressor.set_makeup_db(makeup);
            }
            _ => {
                return Err(format!(
                    "the runtime audio compressor holds no property {field}",
                ));
            }
        }
        world.insert(entity, compressor);
        Ok(())
    }
}

fn check_threshold(threshold: f32) -> Result<(), String> {
    if threshold.is_finite() && (-60.0..=0.0).contains(&threshold) {
        Ok(())
    } else {
        Err(format!(
            "the audio compressor holds the threshold {threshold} decibels",
        ))
    }
}

fn check_ratio(ratio: f32) -> Result<(), String> {
    if ratio.is_finite() && (1.0..=20.0).contains(&ratio) {
        Ok(())
    } else {
        Err(format!("the audio compressor holds the ratio {ratio}"))
    }
}

fn check_attack(attack: f32) -> Result<(), String> {
    if attack.is_finite() && (0.1..=1_000.0).contains(&attack) {
        Ok(())
    } else {
        Err(format!(
            "the audio compressor holds the attack {attack} milliseconds",
        ))
    }
}

fn check_release(release: f32) -> Result<(), String> {
    if release.is_finite() && (1.0..=5_000.0).contains(&release) {
        Ok(())
    } else {
        Err(format!(
            "the audio compressor holds the release {release} milliseconds",
        ))
    }
}

fn check_makeup(makeup: f32) -> Result<(), String> {
    if makeup.is_finite() && (0.0..=24.0).contains(&makeup) {
        Ok(())
    } else {
        Err(format!(
            "the audio compressor holds the makeup {makeup} decibels",
        ))
    }
}

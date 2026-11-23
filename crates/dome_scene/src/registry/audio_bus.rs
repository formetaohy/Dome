use super::{Assets, SceneComponent};
use crate::field::{Axis, no_axis, number};
use crate::reference::AudioBusId;
use crate::scene::Scene;
use crate::scene_value::SceneValue;
use crate::schema::{ComponentSpec, Field, FieldKind, Reach, ReferenceKind};
use crate::value::{Effect, Value};
use dome_audio::AudioBus as Held;
use dome_ecs::{Entity, World};
use dome_toml::{Fields, Toml};

static FIELDS: &[Field] = &[
    Field {
        name: "parent",
        label: "Parent",
        kind: FieldKind::OptionalReference(ReferenceKind::AudioBus),
    },
    Field {
        name: "send",
        label: "Send",
        kind: FieldKind::OptionalReference(ReferenceKind::AudioBus),
    },
    Field {
        name: "gain_db",
        label: "Gain (dB)",
        kind: FieldKind::Number {
            range: [-60.0, 12.0],
            speed: 0.1,
        },
    },
    Field {
        name: "mute",
        label: "Mute",
        kind: FieldKind::Bool,
    },
    Field {
        name: "send_db",
        label: "Send (dB)",
        kind: FieldKind::Number {
            range: [-60.0, 0.0],
            speed: 0.1,
        },
    },
];

static RUNTIME_FIELDS: &[&str] = &["gain_db", "mute", "send_db"];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "audio_bus",
    label: "Audio Bus",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::None,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioBusRef {
    pub parent: Option<AudioBusId>,
    pub send: Option<AudioBusId>,
    pub gain_db: f32,
    pub mute: bool,
    pub send_db: f32,
}

impl AudioBusRef {
    fn of(bus: &Held) -> Self {
        Self {
            parent: bus.parent().map(AudioBusId::new),
            send: bus.send().map(AudioBusId::new),
            gain_db: bus.gain_db(),
            mute: bus.mute(),
            send_db: bus.send_db(),
        }
    }
}

impl SceneComponent for AudioBusRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(match field {
            "parent" => Value::Text(
                self.parent
                    .as_ref()
                    .map_or(String::new(), |parent| parent.as_str().to_owned()),
            ),
            "send" => Value::Text(
                self.send
                    .as_ref()
                    .map_or(String::new(), |send| send.as_str().to_owned()),
            ),
            "gain_db" => Value::Number(self.gain_db),
            "mute" => Value::Bool(self.mute),
            "send_db" => Value::Number(self.send_db),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        match field {
            "parent" => {
                no_axis(axis, "the audio bus parent")?;
                let id = value.text()?;
                self.parent = optional(id, "the audio bus parent")?;
                Ok(Effect::Structure)
            }
            "send" => {
                no_axis(axis, "the audio bus send")?;
                let id = value.text()?;
                self.send = optional(id, "the audio bus send")?;
                Ok(Effect::Structure)
            }
            "gain_db" => {
                no_axis(axis, "the audio bus gain")?;
                let gain = number(value, "the audio bus gain")?;
                check_gain(gain)?;
                self.gain_db = gain;
                Ok(Effect::Value)
            }
            "mute" => {
                no_axis(axis, "the audio bus mute")?;
                self.mute = value.boolean()?;
                Ok(Effect::Value)
            }
            "send_db" => {
                no_axis(axis, "the audio bus send")?;
                let send = number(value, "the audio bus send")?;
                check_send(send)?;
                self.send_db = send;
                Ok(Effect::Value)
            }
            _ => Err(format!("the audio bus holds no property {field}")),
        }
    }

    fn authored(&self) -> Toml {
        Toml::object([
            (
                "parent",
                Toml::string(self.parent.as_ref().map_or("", |parent| parent.as_str())),
            ),
            (
                "send",
                Toml::string(self.send.as_ref().map_or("", |send| send.as_str())),
            ),
            ("gain_db", Toml::from(self.gain_db)),
            ("mute", Toml::boolean(self.mute)),
            ("send_db", Toml::from(self.send_db)),
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
        node: &str,
        world: &mut World,
        entity: Entity,
        _assets: &dyn Assets,
    ) {
        world.insert(
            entity,
            Held::new(
                node,
                self.parent
                    .as_ref()
                    .map(|parent| parent.as_str().to_owned()),
                self.send.as_ref().map(|send| send.as_str().to_owned()),
                self.gain_db,
                self.mute,
                self.send_db,
            ),
        );
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        for (kind, held) in [("parent", &self.parent), ("send", &self.send)] {
            let Some(held) = held else {
                continue;
            };
            held.validate(scene, &format!("the {kind} of audio bus {node}"))?;
            if held.as_str() == node {
                return Err(format!("the audio bus {node} {kind}s to itself"));
            }
        }
        check_gain(self.gain_db).map_err(|error| format!("the audio bus {node} holds {error}"))?;
        check_send(self.send_db).map_err(|error| format!("the audio bus {node} holds {error}"))?;
        Ok(())
    }

    fn create(scene: &Scene) -> Result<Self, String> {
        Ok(Self {
            parent: scene
                .asset_ids(ReferenceKind::AudioBus)
                .first()
                .map(|bus| AudioBusId::new(*bus)),
            ..Self::default()
        })
    }

    fn from_toml(value: Toml, what: &str) -> Self {
        let mut fields = Fields::of(value, what);
        let parent = parse_optional(fields.take("parent"), &format!("the parent of {what}"));
        let send = parse_optional(fields.take("send"), &format!("the send of {what}"));
        let gain_db = fields
            .need("gain_db")
            .f32_value(&format!("the gain of {what}"));
        let mute = fields
            .need("mute")
            .boolean_value(&format!("the mute of {what}"));
        let send_db = fields
            .need("send_db")
            .f32_value(&format!("the send of {what}"));
        fields.done();
        Self {
            parent,
            send,
            gain_db,
            mute,
            send_db,
        }
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        world.get::<Held>(entity).map(Self::of)
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<Held>(entity);
    }

    fn references(&self, kind: ReferenceKind, id: &str) -> bool {
        kind == ReferenceKind::AudioBus
            && [self.parent.as_ref(), self.send.as_ref()]
                .into_iter()
                .flatten()
                .any(|held| held.as_str() == id)
    }

    fn remap(&mut self, prefix: &str) {
        self.parent = self
            .parent
            .as_ref()
            .map(|parent| AudioBusId::new(format!("{prefix}{parent}")));
        self.send = self
            .send
            .as_ref()
            .map(|send| AudioBusId::new(format!("{prefix}{send}")));
    }

    fn write_world(
        world: &mut World,
        entity: Entity,
        field: &str,
        axis: Option<Axis>,
        value: &Value,
    ) -> Result<(), String> {
        let mut bus = world
            .get::<Held>(entity)
            .cloned()
            .ok_or_else(|| format!("the runtime node {} holds no audio bus", entity.index(),))?;
        match field {
            "gain_db" => {
                no_axis(axis, "the audio bus gain")?;
                let gain = number(value, "the audio bus gain")?;
                check_gain(gain)?;
                bus.set_gain_db(gain);
            }
            "mute" => {
                no_axis(axis, "the audio bus mute")?;
                bus.set_mute(value.boolean()?);
            }
            "send_db" => {
                no_axis(axis, "the audio bus send")?;
                let send = number(value, "the audio bus send")?;
                check_send(send)?;
                bus.set_send_db(send);
            }
            _ => {
                return Err(format!("the runtime audio bus holds no property {field}",));
            }
        }
        world.insert(entity, bus);
        Ok(())
    }
}

pub(super) fn require_bus(scene: &Scene, node: &str, what: &str) -> Result<(), String> {
    let index = scene
        .node_index(node)
        .ok_or_else(|| format!("the scene holds no node {node}"))?;
    if !scene.nodes()[index].has_component(SPEC.name) {
        return Err(format!(
            "the {what} of node {node} sits on a node of no audio bus",
        ));
    }
    Ok(())
}

fn optional(id: &str, what: &str) -> Result<Option<AudioBusId>, String> {
    if id.is_empty() {
        return Ok(None);
    }
    if !crate::target::valid_id(id) {
        return Err(format!("{what} holds the id {id}"));
    }
    Ok(Some(AudioBusId::new(id)))
}

fn parse_optional(value: Option<Toml>, what: &str) -> Option<AudioBusId> {
    let id = value?.string_value(what);
    if id.is_empty() {
        return None;
    }
    assert!(crate::target::valid_id(&id), "{what} holds the id {id}");
    Some(AudioBusId::new(id))
}

fn check_gain(gain: f32) -> Result<(), String> {
    if gain.is_finite() && (-60.0..=12.0).contains(&gain) {
        Ok(())
    } else {
        Err(format!("the gain {gain} decibels"))
    }
}

fn check_send(send: f32) -> Result<(), String> {
    if send.is_finite() && (-60.0..=0.0).contains(&send) {
        Ok(())
    } else {
        Err(format!("the send {send} decibels"))
    }
}

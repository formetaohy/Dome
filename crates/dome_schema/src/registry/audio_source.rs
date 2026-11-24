use crate::assets::Assets;
use crate::component::SceneComponent;
use dome_audio::AudioSource as Playing;
use dome_ecs::{Entity, World};
use dome_scene::Scene;
use dome_scene::SceneValue;
use dome_scene::field::{Axis, no_axis, number};
use dome_scene::{AudioBusId, SoundId};
use dome_scene::{ComponentSpec, Field, FieldKind, Reach, ReferenceKind};
use dome_scene::{Effect, Value};
use dome_toml::{Fields, Toml};

static FIELDS: &[Field] = &[
    Field {
        name: "sound",
        label: "Sound",
        kind: FieldKind::Reference(ReferenceKind::Sound),
    },
    Field {
        name: "bus",
        label: "Bus",
        kind: FieldKind::OptionalReference(ReferenceKind::AudioBus),
    },
    Field {
        name: "gain",
        label: "Gain",
        kind: FieldKind::Number {
            range: [0.0, 8.0],
            speed: 0.01,
        },
    },
    Field {
        name: "looping",
        label: "Looping",
        kind: FieldKind::Bool,
    },
    Field {
        name: "spatial",
        label: "Spatial",
        kind: FieldKind::Bool,
    },
    Field {
        name: "min_distance",
        label: "Min Distance",
        kind: FieldKind::Number {
            range: [0.01, 1000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "max_distance",
        label: "Max Distance",
        kind: FieldKind::Number {
            range: [0.01, 10000.0],
            speed: 0.1,
        },
    },
    Field {
        name: "pitch",
        label: "Pitch",
        kind: FieldKind::Number {
            range: [0.25, 4.0],
            speed: 0.01,
        },
    },
    Field {
        name: "priority",
        label: "Priority",
        kind: FieldKind::Number {
            range: [0.0, 100.0],
            speed: 1.0,
        },
    },
    Field {
        name: "autoplay",
        label: "Autoplay",
        kind: FieldKind::Bool,
    },
];

static RUNTIME_FIELDS: &[&str] = &["playing", "gain"];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "audio_source",
    label: "Audio Source",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::None,
};

#[derive(Clone, Debug, PartialEq)]
pub struct AudioSourceRef {
    pub sound: SoundId,
    pub bus: Option<AudioBusId>,
    pub gain: f32,
    pub looping: bool,
    pub spatial: bool,
    pub min_distance: f32,
    pub max_distance: f32,
    pub pitch: f32,
    pub priority: u8,
    pub autoplay: bool,
    pub playing: bool,
}

impl AudioSourceRef {
    pub fn new(sound: SoundId) -> Self {
        Self {
            sound,
            bus: None,
            gain: 1.0,
            looping: false,
            spatial: true,
            min_distance: 1.0,
            max_distance: 50.0,
            pitch: 1.0,
            priority: 0,
            autoplay: true,
            playing: false,
        }
    }
}

impl SceneComponent for AudioSourceRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(match field {
            "sound" => Value::Text(String::from(self.sound.as_str())),
            "bus" => Value::Text(
                self.bus
                    .as_ref()
                    .map_or(String::new(), |bus| bus.as_str().to_owned()),
            ),
            "gain" => Value::Number(self.gain),
            "looping" => Value::Bool(self.looping),
            "spatial" => Value::Bool(self.spatial),
            "min_distance" => Value::Number(self.min_distance),
            "max_distance" => Value::Number(self.max_distance),
            "pitch" => Value::Number(self.pitch),
            "priority" => Value::Number(f32::from(self.priority)),
            "autoplay" => Value::Bool(self.autoplay),
            "playing" => Value::Bool(self.playing),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        match field {
            "sound" => {
                self.sound =
                    SoundId::write(&self.sound, axis, value, "the sound of the audio source")?;
                Ok(Effect::Structure)
            }
            "bus" => {
                no_axis(axis, "the audio source bus")?;
                let id = value.text()?;
                self.bus = if id.is_empty() {
                    None
                } else {
                    if !dome_scene::valid_id(id) {
                        return Err(format!("the audio source bus holds the id {id}"));
                    }
                    Some(AudioBusId::new(id))
                };
                Ok(Effect::Structure)
            }
            "gain" => {
                no_axis(axis, "the audio source gain")?;
                let gain = number(value, "the audio source gain")?;
                if !(gain.is_finite() && gain >= 0.0) {
                    return Err(format!("the audio source holds the gain {gain}"));
                }
                self.gain = gain;
                Ok(Effect::Value)
            }
            "looping" => {
                no_axis(axis, "the audio source looping")?;
                self.looping = value.boolean()?;
                Ok(Effect::Value)
            }
            "spatial" => {
                no_axis(axis, "the audio source spatial")?;
                self.spatial = value.boolean()?;
                Ok(Effect::Value)
            }
            "min_distance" => {
                no_axis(axis, "the audio source minimum distance")?;
                let distance = number(value, "the audio source minimum distance")?;
                if !(distance.is_finite() && distance > 0.0 && distance < self.max_distance) {
                    return Err(format!(
                        "the audio source holds the minimum distance {distance}",
                    ));
                }
                self.min_distance = distance;
                Ok(Effect::Value)
            }
            "max_distance" => {
                no_axis(axis, "the audio source maximum distance")?;
                let distance = number(value, "the audio source maximum distance")?;
                if !(distance.is_finite() && distance > self.min_distance) {
                    return Err(format!(
                        "the audio source holds the maximum distance {distance}",
                    ));
                }
                self.max_distance = distance;
                Ok(Effect::Value)
            }
            "pitch" => {
                no_axis(axis, "the audio source pitch")?;
                let pitch = number(value, "the audio source pitch")?;
                if !(pitch.is_finite() && pitch > 0.0) {
                    return Err(format!("the audio source holds the pitch {pitch}"));
                }
                self.pitch = pitch;
                Ok(Effect::Value)
            }
            "priority" => {
                no_axis(axis, "the audio source priority")?;
                let priority = number(value, "the audio source priority")?;
                if !(0.0..=100.0).contains(&priority) {
                    return Err(format!("the audio source holds the priority {priority}"));
                }
                self.priority = priority.round() as u8;
                Ok(Effect::Value)
            }
            "autoplay" => {
                no_axis(axis, "the audio source autoplay")?;
                self.autoplay = value.boolean()?;
                Ok(Effect::Value)
            }
            _ => Err(format!("the audio source holds no property {field}")),
        }
    }

    fn authored(&self) -> Toml {
        Toml::object([
            ("sound", Toml::string(self.sound.as_str())),
            (
                "bus",
                Toml::string(self.bus.as_ref().map_or("", |bus| bus.as_str())),
            ),
            ("gain", Toml::from(self.gain)),
            ("looping", Toml::boolean(self.looping)),
            ("spatial", Toml::boolean(self.spatial)),
            ("min_distance", Toml::from(self.min_distance)),
            ("max_distance", Toml::from(self.max_distance)),
            ("pitch", Toml::from(self.pitch)),
            ("priority", Toml::from(u32::from(self.priority))),
            ("autoplay", Toml::boolean(self.autoplay)),
        ])
    }

    fn observed(&self) -> Toml {
        self.authored()
    }

    fn runtime(&self) -> Toml {
        Toml::object([
            ("sound", Toml::string(self.sound.as_str())),
            (
                "bus",
                Toml::string(self.bus.as_ref().map_or("", |bus| bus.as_str())),
            ),
            ("playing", Toml::boolean(self.playing)),
            ("gain", Toml::from(self.gain)),
        ])
    }

    fn install(
        &self,
        _scene: &Scene,
        _node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    ) {
        let sound = assets.sound(self.sound.as_str());
        let mut source = Playing::new(self.sound.as_str(), sound);
        source.set_bus(self.bus.as_ref().map_or("", |bus| bus.as_str()));
        source.set_gain(self.gain);
        source.set_looping(self.looping);
        source.set_spatial(self.spatial);
        source.set_distances(self.min_distance, self.max_distance);
        source.set_pitch(self.pitch);
        source.set_priority(self.priority);
        if self.autoplay {
            source.play();
        }
        world.insert(entity, source);
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        self.sound
            .validate(scene, &format!("the sound of node {node}"))?;
        if let Some(bus) = &self.bus {
            bus.validate(scene, &format!("the bus of audio source {node}"))?;
        }
        if !(self.gain.is_finite() && self.gain >= 0.0) {
            return Err(format!(
                "the audio source of node {node} holds the gain {}",
                self.gain,
            ));
        }
        if !(self.min_distance.is_finite() && self.min_distance > 0.0) {
            return Err(format!(
                "the audio source of node {node} holds the minimum distance {}",
                self.min_distance,
            ));
        }
        if !(self.max_distance.is_finite() && self.max_distance > self.min_distance) {
            return Err(format!(
                "the audio source of node {node} holds the maximum distance {}",
                self.max_distance,
            ));
        }
        if !(self.pitch.is_finite() && self.pitch > 0.0) {
            return Err(format!(
                "the audio source of node {node} holds the pitch {}",
                self.pitch,
            ));
        }
        Ok(())
    }

    fn create(scene: &Scene) -> Result<Self, String> {
        let sound = scene
            .sounds()
            .first()
            .ok_or_else(|| String::from("the scene holds no sound to play"))?;
        let mut source = Self::new(SoundId::new(sound.id.as_str()));
        source.bus = scene
            .asset_ids(ReferenceKind::AudioBus)
            .first()
            .map(|bus| AudioBusId::new(*bus));
        Ok(source)
    }

    fn from_toml(value: Toml, what: &str) -> Result<Self, String> {
        let mut fields = Fields::of(value, what)?;
        let sound = SoundId::parse(fields.need("sound")?, &format!("the sound of {what}"))?;
        let bus = match fields.take("bus") {
            Some(value) => {
                let id = value.string_value(&format!("the bus of {what}"))?;
                if id.is_empty() {
                    None
                } else {
                    if !dome_scene::valid_id(&id) {
                        return Err(format!("{what} holds the bus id {id}"));
                    }
                    Some(AudioBusId::new(id))
                }
            }
            None => None,
        };
        let gain = fields
            .need("gain")?
            .f32_value(&format!("the gain of {what}"))?;
        let looping = fields
            .need("looping")?
            .boolean_value(&format!("the looping of {what}"))?;
        let spatial = fields
            .need("spatial")?
            .boolean_value(&format!("the spatial of {what}"))?;
        let min_distance = fields
            .need("min_distance")?
            .f32_value(&format!("the minimum distance of {what}"))?;
        let max_distance = fields
            .need("max_distance")?
            .f32_value(&format!("the maximum distance of {what}"))?;
        let pitch = fields
            .need("pitch")?
            .f32_value(&format!("the pitch of {what}"))?;
        let priority = match fields.take("priority") {
            Some(priority) => priority
                .f32_value(&format!("the priority of {what}"))?
                .round() as u8,
            None => 0,
        };
        let autoplay = fields
            .need("autoplay")?
            .boolean_value(&format!("the autoplay of {what}"))?;
        fields.done()?;
        Ok(Self {
            sound,
            bus,
            gain,
            looping,
            spatial,
            min_distance,
            max_distance,
            pitch,
            priority,
            autoplay,
            playing: false,
        })
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        world.get::<Playing>(entity).map(|source| Self {
            sound: SoundId::new(source.name()),
            bus: (!source.bus().is_empty()).then(|| AudioBusId::new(source.bus())),
            gain: source.gain(),
            looping: source.looping(),
            spatial: source.spatial(),
            min_distance: source.min_distance(),
            max_distance: source.max_distance(),
            pitch: source.pitch(),
            priority: source.priority(),
            autoplay: false,
            playing: source.is_playing(),
        })
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<Playing>(entity);
    }

    fn references(&self, kind: ReferenceKind, id: &str) -> bool {
        (kind == ReferenceKind::Sound && self.sound.as_str() == id)
            || (kind == ReferenceKind::AudioBus
                && self.bus.as_ref().is_some_and(|bus| bus.as_str() == id))
    }

    fn remap(&mut self, prefix: &str) {
        self.bus = self
            .bus
            .as_ref()
            .map(|bus| AudioBusId::new(format!("{prefix}{bus}")));
    }

    fn refresh(
        &self,
        scene: &Scene,
        node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    ) {
        let sound = assets.sound(self.sound.as_str());
        match world
            .get::<Playing>(entity)
            .filter(|held| held.name() == self.sound.as_str())
        {
            Some(held) => {
                let mut source = held.clone();
                source.set_source(sound);
                world.insert(entity, source);
            }
            None => self.install(scene, node, world, entity, assets),
        }
    }

    fn write_world(
        world: &mut World,
        entity: Entity,
        field: &str,
        axis: Option<Axis>,
        value: &Value,
    ) -> Result<(), String> {
        let mut source = world
            .get::<Playing>(entity)
            .cloned()
            .ok_or_else(|| format!("the runtime node {} holds no audio source", entity.index()))?;
        match field {
            "playing" => {
                no_axis(axis, "the audio source playing state")?;
                if value.boolean()? {
                    source.play();
                } else {
                    source.stop();
                }
            }
            "gain" => {
                no_axis(axis, "the audio source gain")?;
                let gain = number(value, "the audio source gain")?;
                if !(gain.is_finite() && gain >= 0.0) {
                    return Err(format!("the audio source holds the gain {gain}"));
                }
                source.set_gain(gain);
            }
            _ => {
                return Err(format!(
                    "the runtime audio source holds no property {field}",
                ));
            }
        }
        world.insert(entity, source);
        Ok(())
    }
}

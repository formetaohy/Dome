use super::{Assets, SceneComponent};
use crate::change::Changed;
use crate::field::{Axis, no_axis, number};
use crate::graph_library::GraphLibrary;
use crate::node::AnimationRef;
use crate::reference::GraphId;
use crate::scene::Scene;
use crate::scene_value::SceneValue;
use crate::schema::{ComponentSpec, Field, FieldKind, Reach, ReferenceKind};
use crate::value::{Effect, Value};
use dome_animation::{AnimationMachine, Parameter, apply};
use dome_ecs::{Entity, World, propagate};
use dome_toml::{Fields, Toml};

static FIELDS: &[Field] = &[
    Field {
        name: "graph",
        label: "Graph",
        kind: FieldKind::Reference(ReferenceKind::Graph),
    },
    Field {
        name: "playing",
        label: "Playing",
        kind: FieldKind::Bool,
    },
    Field {
        name: "fade",
        label: "Fade",
        kind: FieldKind::Number {
            range: [0.0, 10.0],
            speed: 0.01,
        },
    },
];

static RUNTIME_FIELDS: &[&str] = &["state", "time", "playing", "parameter.*"];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "animation",
    label: "Animation",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::None,
};

impl SceneComponent for AnimationRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        if let Some(name) = field.strip_prefix("parameter.") {
            return self.parameters().get(name).map(|value| value_of(*value));
        }
        Some(match field {
            "graph" => Value::Text(String::from(self.graph.as_str())),
            "playing" => Value::Bool(self.playing),
            "fade" => Value::Number(self.fade),
            "state" => Value::Text(String::from(self.state())),
            "time" => Value::Number(self.time()),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        match field {
            "graph" => {
                self.graph =
                    GraphId::write(&self.graph, axis, value, "the graph of the component")?;
                Ok(Effect::Structure)
            }
            "playing" => {
                no_axis(axis, "the animation playback")?;
                self.playing = value.boolean()?;
                Ok(Effect::Value)
            }
            "fade" => {
                no_axis(axis, "the animation fade")?;
                let fade = number(value, "the animation fade")?;
                if fade < 0.0 {
                    return Err(format!("the animation fades over {fade}"));
                }
                self.fade = fade;
                Ok(Effect::Value)
            }
            _ => Err(format!("the animation holds no property {field}")),
        }
    }

    fn authored(&self) -> Toml {
        Toml::object([
            ("graph", Toml::string(self.graph.as_str())),
            ("playing", Toml::boolean(self.playing)),
            ("fade", Toml::from(self.fade)),
        ])
    }

    fn observed(&self) -> Toml {
        self.authored()
    }

    fn runtime(&self) -> Toml {
        Toml::object([
            ("state", Toml::string(self.state())),
            ("time", Toml::from(self.time())),
            ("playing", Toml::boolean(self.playing)),
            (
                "parameters",
                Toml::object(
                    self.parameters()
                        .iter()
                        .map(|(name, value)| (name.as_str(), toml(*value))),
                ),
            ),
        ])
    }

    fn install(
        &self,
        _scene: &Scene,
        _node: &str,
        world: &mut World,
        entity: Entity,
        _assets: &dyn Assets,
    ) {
        let graph = world
            .resource::<GraphLibrary>()
            .expect("a scene installs the animation graphs its players read")
            .source(self.graph.as_str());
        world.insert(
            entity,
            AnimationMachine::new(graph, self.playing, self.fade),
        );
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        self.graph
            .validate(scene, &format!("the animation of node {node}"))?;
        if !self.fade.is_finite() || self.fade < 0.0 {
            let fade = self.fade;
            return Err(format!("the animation of node {node} fades over {fade}"));
        }
        Ok(())
    }

    fn create(scene: &Scene) -> Result<Self, String> {
        let graph = scene
            .graphs()
            .first()
            .ok_or_else(|| String::from("the scene holds no animation graph to play"))?;
        Ok(Self::new(
            graph.id.as_str(),
            true,
            AnimationRef::DEFAULT_FADE,
        ))
    }

    fn from_toml(value: Toml, what: &str) -> Self {
        let mut fields = Fields::of(value, what);
        let graph = GraphId::parse(fields.need("graph"), &format!("the graph of {what}"));
        let playing = fields
            .need("playing")
            .boolean_value(&format!("the playback of {what}"));
        let fade = fields
            .need("fade")
            .f32_value(&format!("the fade of {what}"));
        fields.done();
        Self::new(graph, playing, fade)
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        let machine = world.get::<AnimationMachine>(entity)?;
        Some(
            Self::new(machine.graph().name(), machine.playing(), machine.fade()).with_runtime(
                machine.state().id(),
                machine.time(),
                machine
                    .parameters()
                    .map(|(name, value)| (String::from(name), value))
                    .collect(),
            ),
        )
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<AnimationMachine>(entity);
    }

    fn references(&self, kind: ReferenceKind, id: &str) -> bool {
        kind == ReferenceKind::Graph && self.graph.as_str() == id
    }

    fn stale(&self, changed: &Changed) -> bool {
        changed.holds(ReferenceKind::Graph) || changed.holds(ReferenceKind::Animation)
    }

    fn refresh(
        &self,
        scene: &Scene,
        node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    ) {
        let graph = world
            .resource::<GraphLibrary>()
            .expect("a scene installs the animation graphs its players read")
            .source(self.graph.as_str());
        match world
            .get::<AnimationMachine>(entity)
            .filter(|held| held.graph().name() == self.graph.as_str())
        {
            Some(held) => {
                let mut machine = held.clone();
                machine.rebase(graph);
                world.insert(entity, machine);
            }
            None => self.install(scene, node, world, entity, assets),
        }
    }

    fn remap(&mut self, prefix: &str) {
        self.graph = GraphId::new(format!("{prefix}{}", self.graph.as_str()));
    }

    fn write_world(
        world: &mut World,
        entity: Entity,
        field: &str,
        axis: Option<Axis>,
        value: &Value,
    ) -> Result<(), String> {
        let mut machine = world
            .get::<AnimationMachine>(entity)
            .cloned()
            .ok_or_else(|| format!("the runtime node {} plays no animation", entity.index()))?;
        let mut seek = false;
        if let Some(name) = field.strip_prefix("parameter.") {
            no_axis(axis, "the animation parameter")?;
            machine.set_parameter(
                name,
                parameter(value, &format!("the animation parameter {name}"))?,
            )?;
        } else {
            match field {
                "state" => {
                    no_axis(axis, "the animation state")?;
                    let state = value.text()?;
                    machine.set_state(state)?;
                    seek = true;
                }
                "time" => {
                    no_axis(axis, "the animation time")?;
                    machine.set_time(number(value, "the animation time")?);
                    seek = true;
                }
                "playing" => {
                    no_axis(axis, "the animation playback")?;
                    machine.set_playing(value.boolean()?);
                }
                _ => return Err(format!("the runtime animation holds no property {field}")),
            }
        }
        world.insert(entity, machine.clone());
        if seek {
            apply(world, &machine);
            propagate(world);
        }
        Ok(())
    }
}

fn parameter(value: &Value, what: &str) -> Result<Parameter, String> {
    match value {
        Value::Number(value) => Ok(Parameter::Number(*value)),
        Value::Bool(value) => Ok(Parameter::Boolean(*value)),
        other => Err(format!("{what} reads a value from a {}", other.kind())),
    }
}

fn value_of(value: Parameter) -> Value {
    match value {
        Parameter::Number(value) => Value::Number(value),
        Parameter::Boolean(value) => Value::Bool(value),
    }
}

fn toml(value: Parameter) -> Toml {
    match value {
        Parameter::Number(value) => Toml::from(value),
        Parameter::Boolean(value) => Toml::boolean(value),
    }
}

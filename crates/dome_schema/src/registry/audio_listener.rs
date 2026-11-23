use crate::assets::Assets;
use crate::component::SceneComponent;
use dome_audio::AudioListener as Held;
use dome_ecs::{Entity, World};
use dome_scene::Scene;
use dome_scene::field::Axis;
use dome_scene::{ComponentSpec, Reach};
use dome_scene::{Effect, Value};
use dome_toml::{Fields, Toml};

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "audio_listener",
    label: "Audio Listener",
    required: false,
    fields: &[],
    runtime_fields: &[],
    reach: Reach::None,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioListenerRef;

impl SceneComponent for AudioListenerRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, _field: &str) -> Option<Value> {
        None
    }

    fn write(
        &mut self,
        field: &str,
        _axis: Option<Axis>,
        _value: &Value,
    ) -> Result<Effect, String> {
        Err(format!("the audio listener holds no property {field}",))
    }

    fn authored(&self) -> Toml {
        Toml::object(std::iter::empty::<(String, Toml)>())
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
        _assets: &dyn Assets,
    ) {
        world.insert(entity, Held);
    }

    fn create(_scene: &Scene) -> Result<Self, String> {
        Ok(Self)
    }

    fn from_toml(value: Toml, what: &str) -> Self {
        let fields = Fields::of(value, what);
        fields.done();
        Self
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        world.contains::<Held>(entity).then_some(Self)
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<Held>(entity);
    }
}

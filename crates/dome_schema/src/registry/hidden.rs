use crate::assets::Assets;
use crate::component::SceneComponent;
use dome_ecs::{Entity, World};
use dome_render::Hidden;
use dome_scene::Scene;
use dome_scene::field::Axis;
use dome_scene::{ComponentSpec, Reach};
use dome_scene::{Effect, Value};
use dome_toml::{Fields, Toml};

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "hidden",
    label: "Hidden",
    required: false,
    fields: &[],
    runtime_fields: &[],
    reach: Reach::Node,
};

impl SceneComponent for Hidden {
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
        Err(format!("the hidden component holds no property {field}"))
    }

    fn authored(&self) -> Toml {
        Toml::object(std::iter::empty::<(String, Toml)>())
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
        Ok(Self)
    }

    fn from_toml(value: Toml, what: &str) -> Result<Self, String> {
        let fields = Fields::of(value, what)?;
        fields.done()?;
        Ok(Self)
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        world.get::<Self>(entity).copied()
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<Self>(entity);
    }
}

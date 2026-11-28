use crate::assets::Assets;
use crate::component::SceneComponent;
use crate::node::SkinRef;
use dome_ecs::{Entity, World};
use dome_render::Skin;
use dome_scene::Scene;
use dome_scene::SceneValue;
use dome_scene::SkinId;
use dome_scene::field::Axis;
use dome_scene::{ComponentSpec, Field, FieldKind, Reach, ReferenceKind};
use dome_scene::{Effect, Value};
use dome_toml::{Fields, Toml};
use std::sync::Arc;

static FIELDS: &[Field] = &[Field {
    name: "skin",
    label: "Skin",
    kind: FieldKind::Reference(ReferenceKind::Skin),
}];

static RUNTIME_FIELDS: &[&str] = &[];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "skin",
    label: "Skin",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::Node,
};

impl SceneComponent for SkinRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        match field {
            "skin" => Some(Value::Text(String::from(self.skin.as_str()))),
            _ => None,
        }
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        if field != "skin" {
            return Err(format!("the skin holds no property {field}"));
        }
        self.skin = SkinId::write(&self.skin, axis, value, "the skin of the component")?;
        Ok(Effect::Structure)
    }

    fn authored(&self) -> Toml {
        Toml::object([("skin", Toml::string(self.skin.as_str()))])
    }

    fn observed(&self) -> Toml {
        self.authored()
    }

    fn install(
        &self,
        _scene: &dome_scene::Scene,
        node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    ) {
        let data = assets.skin(self.skin.as_str(), node);
        let nodes = world
            .resource::<crate::install::NodeMap>()
            .expect("a scene installs the nodes its skin joints name");
        let joints = data
            .joints()
            .iter()
            .map(|joint| nodes.entity(joint.as_str()))
            .collect::<Vec<_>>();
        let inverse_binds = Arc::from(data.inverse_binds());
        world.insert(entity, Skin::new(self.skin.as_str(), joints, inverse_binds));
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        self.skin
            .validate(scene, &format!("the skin of node {node}"))
    }

    fn create(scene: &Scene) -> Result<Self, String> {
        let skin = scene
            .skins()
            .first()
            .ok_or_else(|| String::from("the scene holds no skin to deform"))?;
        Ok(Self::new(skin.id.as_str()))
    }

    fn from_toml(value: Toml, what: &str) -> Result<Self, String> {
        let mut fields = Fields::of(value, what)?;
        let skin = SkinId::parse(fields.need("skin")?, &format!("the skin of {what}"))?;
        fields.done()?;
        Ok(Self { skin })
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        world.get::<Skin>(entity).map(|skin| Self {
            skin: SkinId::new(skin.source()),
        })
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<Skin>(entity);
    }

    fn references(&self, kind: ReferenceKind, id: &str) -> bool {
        kind == ReferenceKind::Skin && self.skin.as_str() == id
    }

    fn remap(&mut self, prefix: &str) {
        self.skin = SkinId::new(format!("{prefix}{}", self.skin.as_str()));
    }
}

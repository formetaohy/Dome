use dome_ecs::World;
use dome_math::Vec3;
use dome_scene::{
    Assets, Axis, FieldKind, MaterialId, MeshId, NodeId, ReferenceKind, Registry, Scene,
    SceneComponent, SceneNode, SceneValue, Target, Value, observe_runtime, set_runtime_property,
};
use dome_toml::Toml;
use std::sync::Arc;

struct NoAssets;

impl Assets for NoAssets {
    fn mesh(&self, _id: &str) -> dome_render::Mesh {
        panic!("a health component renders no mesh")
    }

    fn material(&self, _id: &str) -> dome_render::Material {
        panic!("a health component renders no material")
    }

    fn skin(&self, _id: &str) -> dome_scene::SkinData {
        panic!("a health component deforms no mesh")
    }

    fn animation(&self, _id: &str) -> Arc<dome_animation::Clip> {
        panic!("a health component plays no animation")
    }
}

#[derive(Clone, Debug, PartialEq, SceneComponent)]
#[scene(name = "health", label = "Health")]
struct Health {
    #[scene(label = "Value", range = [0.0, 1000.0], speed = 1.0, runtime)]
    value: f32,
    #[scene(label = "Team", choices = ["red", "blue"])]
    team: String,
    #[scene(label = "Spawn", color)]
    spawn: Vec3,
    #[scene(label = "Alive", runtime)]
    alive: bool,
    #[scene(skip, runtime)]
    cooldown: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            value: 100.0,
            team: String::from("red"),
            spawn: Vec3::ZERO,
            alive: true,
            cooldown: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, SceneComponent)]
#[scene(name = "player", label = "Player")]
struct Player;

#[derive(Clone, Debug, Default, PartialEq)]
struct Distance(f32);

impl SceneValue for Distance {
    const KIND: FieldKind = FieldKind::Number {
        range: [0.0, 10.0],
        speed: 0.1,
    };

    fn read(&self) -> Value {
        Value::Number(self.0)
    }

    fn write(
        current: &Self,
        axis: Option<Axis>,
        value: &Value,
        what: &str,
    ) -> Result<Self, String> {
        <f32 as SceneValue>::write(&current.0, axis, value, what).map(Self)
    }

    fn authored(&self) -> Toml {
        <f32 as SceneValue>::authored(&self.0)
    }

    fn parse(value: Toml, what: &str) -> Self {
        Self(<f32 as SceneValue>::parse(value, what))
    }

    fn validate(&self, scene: &Scene, what: &str) -> Result<(), String> {
        <f32 as SceneValue>::validate(&self.0, scene, what)?;
        if self.0 > 10.0 {
            return Err(format!("{what} holds {}", self.0));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, SceneComponent)]
#[scene(name = "spawn", label = "Spawn")]
struct Spawn {
    #[scene(label = "Mesh")]
    mesh: MeshId,
    #[scene(label = "Material")]
    material: MaterialId,
    #[scene(label = "Target")]
    target: NodeId,
    #[scene(label = "Reach", speed = 0.5)]
    reach: Distance,
}

impl Default for Spawn {
    fn default() -> Self {
        Self {
            mesh: MeshId::default(),
            material: MaterialId::default(),
            target: NodeId::default(),
            reach: Distance(0.0),
        }
    }
}

const SPAWN_SCENE: &str = r#"format = "dome_scene"
images = []

[[meshes]]
id = "cube"
shape = "cube"

[[materials]]
id = "grey"
metallic = 0.0
roughness = 0.5

base_color = [1.0, 1.0, 1.0]
alpha = 1.0
alpha_mode = "opaque"
alpha_cutoff = 0.5
normal_scale = 1.0
occlusion_strength = 1.0
emissive = [0.0, 0.0, 0.0]

[[nodes]]
id = "cube"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[[nodes]]
id = "player"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.spawn]
mesh = "cube"
material = "grey"
target = "cube"
reach = 3.0
"#;

const SCENE: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []

[[nodes]]
id = "player"

[nodes.health]
value = 10.0
team = "red"
spawn = [0.0, 0.0, 0.0]
alive = true

[nodes.player]

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]
"#;

#[test]
fn a_derived_component_travels_through_every_layer() {
    let mut registry = Registry::default();
    registry.register::<Health>().register::<Player>();
    let health = registry
        .components()
        .find(|component| component.name == "health")
        .expect("the registry holds the derived health");
    assert_eq!(
        health
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        ["value", "team", "spawn", "alive"],
    );
    assert_eq!(health.runtime_fields, ["value", "alive", "cooldown"]);
    assert!(matches!(
        health.fields[1].kind,
        FieldKind::Choice(&["red", "blue"])
    ));
    let registry = Arc::new(registry);
    let mut scene = Scene::parse_with(SCENE, ".", "the test scene", registry.clone());
    let player = Target::Node(String::from("player"));
    assert_eq!(scene.property(&player, "health"), Some(Value::Bool(true)));
    assert_eq!(
        scene.property(&player, "health.value"),
        Some(Value::Number(10.0))
    );
    assert_eq!(
        scene.property(&player, "health.team"),
        Some(Value::Text(String::from("red")))
    );
    assert_eq!(
        scene.property(&player, "health.spawn.x"),
        Some(Value::Number(0.0))
    );
    assert_eq!(
        scene.property(&player, "health.alive"),
        Some(Value::Bool(true))
    );
    assert_eq!(scene.property(&player, "player"), Some(Value::Bool(true)));
    assert_eq!(scene.property(&player, "health.cooldown"), None);
    edit(&mut scene, |edit| {
        edit.set_property(&player, "health", &Value::Bool(false))
    })
    .expect("a derived component leaves its node");
    assert_eq!(scene.property(&player, "health"), Some(Value::Bool(false)));
    edit(&mut scene, |edit| {
        edit.set_property(&player, "health", &Value::Bool(true))
    })
    .expect("a derived component joins its node");
    assert_eq!(
        scene.property(&player, "health.value"),
        Some(Value::Number(100.0))
    );
    edit(&mut scene, |edit| {
        edit.set_property(&player, "health.value", &Value::Number(20.0))
    })
    .expect("a derived component edits its number");
    edit(&mut scene, |edit| {
        edit.set_property(&player, "health.spawn.x", &Value::Number(1.0))
    })
    .expect("a derived component edits one vector axis");
    edit(&mut scene, |edit| {
        edit.set_property(&player, "health.team", &Value::Text(String::from("green")))
    })
    .expect_err("a derived component rejects a choice it does not hold");
    edit(&mut scene, |edit| {
        edit.set_property(&player, "health.team", &Value::Text(String::from("blue")))
    })
    .expect("a derived component edits its choice");
    edit(&mut scene, |edit| {
        edit.set_property(&player, "player", &Value::Bool(false))
    })
    .expect("a derived tag leaves its node");
    assert_eq!(scene.property(&player, "player"), Some(Value::Bool(false)));
    edit(&mut scene, |edit| {
        edit.set_property(&player, "player", &Value::Bool(true))
    })
    .expect("a derived tag joins its node");
    assert_eq!(scene.property(&player, "player"), Some(Value::Bool(true)));
    let written = scene.write();
    assert!(written.contains("[nodes.health]"));
    assert!(written.contains("team = \"blue\""));
    assert!(!written.contains("cooldown"));
    assert!(written.contains("[nodes.player]"));
    let again = Scene::parse_with(&written, ".", "the written scene", registry.clone());
    assert_eq!(again, scene);
    assert!(scene.observe().write().contains("value = 20.0"));

    let mut world = World::new();
    let entity = world.spawn();
    world.insert(
        entity,
        SceneNode {
            id: String::from("player"),
        },
    );
    world.insert(entity, Health::default());
    world.insert(entity, Player);
    let observed = Toml::object([("nodes", observe_runtime(&world, &registry))]).write();
    assert!(observed.contains("value = 100.0"));
    assert!(observed.contains("cooldown = 0.0"));
    set_runtime_property(
        &mut world,
        &registry,
        &player,
        "health.value",
        &Value::Number(30.0),
    )
    .expect("a derived component edits its runtime field");
    assert_eq!(
        world.get::<Health>(entity).map(|health| health.value),
        Some(30.0)
    );
    set_runtime_property(
        &mut world,
        &registry,
        &player,
        "health.team",
        &Value::Text(String::from("red")),
    )
    .expect_err("a derived component writes only its runtime fields");
    set_runtime_property(
        &mut world,
        &registry,
        &player,
        "health.cooldown",
        &Value::Number(1.5),
    )
    .expect("a derived component edits its skipped runtime field");
    assert_eq!(
        world.get::<Health>(entity).map(|health| health.cooldown),
        Some(1.5)
    );

    let mut installed = World::new();
    let entities = scene.install(&mut installed, &NoAssets);
    assert_eq!(
        installed
            .get::<Health>(entities[0])
            .map(|health| health.value),
        Some(20.0),
    );
    assert!(installed.contains::<Player>(entities[0]));
    let mut applied = World::new();
    let mut sync = dome_scene::SceneSync::new();
    sync.sync(
        &scene,
        &mut applied,
        &NoAssets,
        &dome_scene::Changed::none(),
    );
    let applied_entity = sync.entity(0).expect("the sync spawns the node");
    assert_eq!(
        applied
            .get::<Health>(applied_entity)
            .map(|health| health.value),
        Some(20.0),
    );
}

#[test]
fn a_derived_component_holds_custom_values_and_asset_references() {
    let mut registry = Registry::default();
    registry.register::<Spawn>();
    let spec = registry
        .components()
        .find(|component| component.name == "spawn")
        .expect("the registry holds the derived spawn");
    assert!(matches!(
        spec.fields[0].kind,
        FieldKind::Reference(ReferenceKind::Mesh)
    ));
    assert!(matches!(
        spec.fields[1].kind,
        FieldKind::Reference(ReferenceKind::Material)
    ));
    assert!(matches!(
        spec.fields[2].kind,
        FieldKind::Reference(ReferenceKind::Node)
    ));
    assert!(matches!(
        spec.fields[3].kind,
        FieldKind::Number {
            range: [0.0, 10.0],
            speed: 0.5
        }
    ));
    let registry = Arc::new(registry);
    let mut scene = Scene::parse_with(SPAWN_SCENE, ".", "the spawn scene", registry.clone());
    let player = Target::Node(String::from("player"));
    assert_eq!(
        scene.property(&player, "spawn.mesh"),
        Some(Value::Text(String::from("cube")))
    );
    assert_eq!(
        scene.property(&player, "spawn.reach"),
        Some(Value::Number(3.0))
    );
    assert_eq!(scene.asset_ids(ReferenceKind::Node), vec!["cube", "player"]);
    edit(&mut scene, |edit| {
        edit.set_property(&player, "spawn.mesh", &Value::Text(String::from("ghost")))
    })
    .expect_err("a reference names an asset the scene holds");
    edit(&mut scene, |edit| {
        edit.set_property(&player, "spawn.reach", &Value::Number(11.0))
    })
    .expect_err("a custom value validates itself");
    edit(&mut scene, |edit| {
        edit.set_property(&player, "spawn.reach", &Value::Number(4.0))
    })
    .expect("a custom value edits its number");
    assert_eq!(
        scene.property(&player, "spawn.reach"),
        Some(Value::Number(4.0))
    );
    edit(&mut scene, |edit| {
        edit.set_property(&player, "spawn", &Value::Bool(false))
    })
    .expect("a derived component leaves its node");
    edit(&mut scene, |edit| {
        edit.set_property(&player, "spawn", &Value::Bool(true))
    })
    .expect("a derived component joins its node with scene references");
    assert_eq!(
        scene.property(&player, "spawn.mesh"),
        Some(Value::Text(String::from("cube")))
    );
    assert_eq!(
        scene.property(&player, "spawn.material"),
        Some(Value::Text(String::from("grey")))
    );
    assert_eq!(
        scene.property(&player, "spawn.target"),
        Some(Value::Text(String::from("cube")))
    );
    assert_eq!(
        scene.property(&player, "spawn.reach"),
        Some(Value::Number(0.0))
    );
    let written = scene.write();
    assert!(written.contains("reach = 0.0"));
    let again = Scene::parse_with(&written, ".", "the written spawn scene", registry);
    assert_eq!(again, scene);

    assert!(
        edit(&mut scene, |edit| edit
            .remove(&Target::Mesh(String::from("cube"))))
        .is_err(),
        "a referenced mesh stays"
    );
    assert!(
        edit(&mut scene, |edit| edit
            .remove(&Target::Material(String::from("grey"))))
        .is_err(),
        "a referenced material stays"
    );
    assert!(
        edit(&mut scene, |edit| edit
            .remove(&Target::Node(String::from("cube"))))
        .is_err(),
        "a referenced node stays"
    );
    edit(&mut scene, |edit| {
        edit.remove(&Target::Node(String::from("player")))
    })
    .expect("a node leaves with the components it owns");
    edit(&mut scene, |edit| {
        edit.remove(&Target::Mesh(String::from("cube")))
    })
    .expect("an unreferenced mesh leaves");
    edit(&mut scene, |edit| {
        edit.remove(&Target::Material(String::from("grey")))
    })
    .expect("an unreferenced material leaves");
}
fn edit<T>(
    scene: &mut dome_scene::Scene,
    change: impl FnOnce(&mut dome_scene::Edit<'_>) -> Result<T, String>,
) -> Result<T, String> {
    scene.edit(change).map(|(value, _)| value)
}

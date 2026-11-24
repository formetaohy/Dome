use dome_animation::{AnimationMachine, Clip, Interpolation, Parameter, Property, Track, advance};
use dome_ecs::{LocalTransform, World};
use dome_math::Mat4;
use dome_scene::{Changed, NodeId, ReferenceKind, Scene, SkinData, Target, Value};
use dome_schema::{
    AnimationLibrary, GraphLibrary, SceneSync, observe_runtime, read_runtime_property,
    set_runtime_property,
};
use dome_schema::{Assets, SceneNode};
use dome_toml::Toml;
use std::sync::Arc;

struct Mock;

impl Assets for Mock {
    fn mesh(&self, _id: &str) -> dome_render::Mesh {
        panic!("a skinned scene renders no mesh")
    }

    fn material(&self, _id: &str) -> dome_render::Material {
        panic!("a skinned scene renders no material")
    }

    fn skin(&self, id: &str) -> SkinData {
        assert_eq!(id, "rig_skin_0");
        SkinData::new(vec![NodeId::new("joint")], Arc::from([Mat4::IDENTITY]))
    }

    fn animation(&self, id: &str) -> Arc<Clip> {
        let (start, end) = match id {
            "rig_animation_0" => (0.0, 1.0),
            "rig_animation_1" => (10.0, 11.0),
            other => panic!("the mock holds no animation {other}"),
        };
        Arc::new(Clip::new(vec![Track::new(
            "joint",
            Property::Translation,
            Interpolation::Linear,
            vec![0.0, 1.0],
            vec![start, 0.0, 0.0, end, 0.0, 0.0],
        )]))
    }
}

const SCENE: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []

[[models]]
id = "rig"
path = "assets/rig.glb"

[[skins]]
id = "rig_skin_0"
model = "rig"
skin = 0
joints = ["joint"]

[[animations]]
id = "rig_animation_0"
model = "rig"
animation = 0
targets = ["joint"]

[[animations]]
id = "rig_animation_1"
model = "rig"
animation = 1
targets = ["joint"]

[[graphs]]
id = "rig_graph"
entry = "idle"

[[graphs.parameters]]
name = "walk"
boolean = false

[[graphs.states]]
id = "idle"
clip = "rig_animation_0"
speed = 1.0
looped = true

[[graphs.states]]
id = "walk"
clip = "rig_animation_1"
speed = 1.0
looped = true

[[graphs.transitions]]
from = "idle"
to = "walk"
fade = 0.25
exit = 0.0

[[graphs.transitions.conditions]]
parameter = "walk"
comparison = "equal"
boolean = true

[[nodes]]
id = "joint"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[[nodes]]
id = "holder"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.skin]
skin = "rig_skin_0"

[nodes.animation]
graph = "rig_graph"
playing = true
fade = 0.25
"#;

#[test]
fn a_self_transition_is_refused_while_the_graph_is_authored() {
    let mut scene =
        Scene::parse(SCENE, ".", "the graph scene", schema()).expect("the graph scene parses");
    let refused =
        scene.edit(|edit| edit.add_graph_transition("rig_graph", "idle", "idle", 0.0, 0.5));
    assert!(
        refused.is_err(),
        "a graph transitions between the states it holds",
    );
    assert_eq!(
        scene
            .graph("rig_graph")
            .expect("the scene holds its graph")
            .asset
            .transitions()
            .len(),
        1,
        "a refused transition leaves the graph as it stood",
    );
    let self_transition = SCENE.replace("to = \"walk\"", "to = \"idle\"");
    let message = Scene::parse(&self_transition, ".", "the self transition scene", schema())
        .expect_err("a self transition is refused while the graph parses");
    assert!(message.contains("leaves idle for itself"), "{message}");
}

#[test]
fn a_scene_resolves_its_skin_and_graph_into_runtime_components() {
    let mut scene =
        Scene::parse(SCENE, ".", "the test scene", schema()).expect("the test scene parses");
    let mut world = World::new();
    let mut sync = SceneSync::new(schema());
    sync.sync(&scene, &mut world, &Mock, &Changed::none());
    let joint = sync.entity(0).expect("the sync spawns the joint");
    let holder = sync.entity(1).expect("the sync spawns the holder");
    assert!(
        world
            .get::<dome_render::Skin>(holder)
            .expect("the holder deforms its skin")
            .joints()
            .contains(&joint)
    );
    assert!(world.get::<AnimationMachine>(holder).is_some());
    advance(&mut world, 0.5);
    let held = world
        .get::<LocalTransform>(joint)
        .expect("the joint holds its sampled pose");
    assert!((held.0.translation.x - 0.5).abs() < 1e-3);
    let target = Target::Node(String::from("holder"));
    assert_eq!(
        read_runtime_property(&world, &schema(), &target, "animation.parameter.walk"),
        Some(Value::Bool(false)),
    );
    let observed = Toml::object([("nodes", observe_runtime(&world, &schema()))]);
    let text = observed.write();
    assert!(text.contains("state = \"idle\""));
    assert!(text.contains("time = 0.5"));
    assert!(text.contains("walk = false"));
    assert!(text.contains("skin = \"rig_skin_0\""));
    set_runtime_property(
        &mut world,
        &schema(),
        &target,
        "animation.parameter.walk",
        &Value::Bool(true),
    )
    .expect("the runtime raises the walk parameter");
    advance(&mut world, 0.125);
    let held = world
        .get::<LocalTransform>(joint)
        .expect("the joint holds its sampled pose");
    assert!((held.0.translation.x - 0.625).abs() < 1e-3);
    advance(&mut world, 0.125);
    let held = world
        .get::<LocalTransform>(joint)
        .expect("the joint holds its blended pose");
    assert!((held.0.translation.x - 5.4375).abs() < 1e-3);
    advance(&mut world, 0.125);
    let held = world
        .get::<LocalTransform>(joint)
        .expect("the joint holds its faded pose");
    assert!((held.0.translation.x - 10.25).abs() < 1e-3);
    let observed = Toml::object([("nodes", observe_runtime(&world, &schema()))]);
    let text = observed.write();
    assert!(text.contains("state = \"walk\""));
    let played = world
        .get::<AnimationMachine>(holder)
        .expect("the holder plays its graph");
    let (state, time) = (String::from(played.state().id()), played.time());
    let source = |world: &World, id: &str| {
        Arc::as_ptr(
            &world
                .resource::<AnimationLibrary>()
                .expect("a synced scene holds its clips")
                .get(id)
                .expect("the scene holds the clip"),
        )
    };
    let graph = |world: &World| {
        Arc::as_ptr(
            &world
                .resource::<GraphLibrary>()
                .expect("a synced scene holds its graphs")
                .get("rig_graph")
                .expect("the scene holds its graph"),
        )
    };
    let held = (
        source(&world, "rig_animation_0"),
        source(&world, "rig_animation_1"),
    );
    let before = graph(&world);
    let mut changed = Changed::none();
    changed.insert(ReferenceKind::Animation, "rig_animation_1");
    sync.sync(&scene, &mut world, &Mock, &changed);
    assert_eq!(
        source(&world, "rig_animation_0"),
        held.0,
        "a clip no change names keeps the source it built",
    );
    assert_ne!(
        source(&world, "rig_animation_1"),
        held.1,
        "a changed clip rebuilds its source",
    );
    assert_ne!(
        graph(&world),
        before,
        "a rebuilt clip rebuilds the graph that plays it",
    );
    let rebuilt = world
        .get::<AnimationMachine>(holder)
        .expect("the reloaded clip keeps its machine");
    assert_eq!(
        rebuilt.state().id(),
        state,
        "a reloaded clip keeps the state its machine played",
    );
    assert_eq!(
        rebuilt.time(),
        time,
        "a reloaded clip keeps the time its machine reached",
    );
    scene
        .edit(|edit| edit.set_property(&target, "animation.playing", &Value::Bool(false)))
        .expect("the scene stops the animation it authored");
    sync.apply(&scene, &mut world, &Mock, 1);
    assert!(
        !world
            .get::<AnimationMachine>(holder)
            .expect("the holder plays its graph")
            .playing(),
        "an authored playback change replaces the machine an asset reload kept",
    );
    let written = scene.write().expect("a scene writes its text");
    let parsed = Scene::parse(&written, ".", "the written scene", schema())
        .expect("the written scene parses");
    assert_eq!(parsed, scene);
    assert!(parsed.graph("rig_graph").is_some());
    assert!(
        world
            .iter::<SceneNode>()
            .any(|(_, node)| node.id == "joint")
    );
}

#[test]
fn a_graph_edited_in_the_document_reaches_the_machine_that_plays_it() {
    let mut scene =
        Scene::parse(SCENE, ".", "the graph scene", schema()).expect("the graph scene parses");
    let mut world = World::new();
    let mut sync = SceneSync::new(schema());
    sync.sync(&scene, &mut world, &Mock, &Changed::none());
    let holder = sync.entity(1).expect("the sync spawns the holder");
    let target = Target::Node(String::from("holder"));
    let read = |world: &World| {
        read_runtime_property(world, &schema(), &target, "animation.parameter.sprint")
    };
    assert_eq!(read(&world), None, "the graph holds no sprint parameter");
    scene
        .edit(|edit| edit.add_graph_parameter("rig_graph", "sprint", Parameter::Boolean(false)))
        .expect("the graph holds the parameter it adds");
    sync.sync(&scene, &mut world, &Mock, &Changed::none());
    assert!(
        world.get::<AnimationMachine>(holder).is_some(),
        "the holder keeps the machine that plays its graph",
    );
    assert_eq!(
        read(&world),
        Some(Value::Bool(false)),
        "a graph edited in the document reaches the machine that plays it",
    );
}

fn schema() -> std::sync::Arc<dome_schema::Registry> {
    std::sync::Arc::new(dome_schema::Registry::default())
}

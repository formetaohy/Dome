use dome_ecs::{LocalTransform, Parent, World, children};
use dome_math::{Transform, Vec3};
use dome_render::SceneDamage;
use dome_scene::{Changed, ReferenceKind, Scene, SkinData, Target};
use dome_schema::Assets;
use dome_schema::SceneSync;
use std::sync::Arc;

struct NoAssets;

impl Assets for NoAssets {
    fn mesh(&self, _id: &str) -> dome_render::Mesh {
        panic!("a synced scene renders no mesh")
    }

    fn material(&self, _id: &str) -> dome_render::Material {
        panic!("a synced scene renders no material")
    }

    fn skin(&self, _id: &str, _node: &str) -> SkinData {
        panic!("a synced scene deforms no mesh")
    }

    fn animation(&self, _id: &str, _targets: &[dome_scene::NodeId]) -> Arc<dome_animation::Clip> {
        panic!("a synced scene plays no animation")
    }
}

#[derive(Debug, PartialEq)]
struct Ticks(u32);

const SCENE: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []

[[nodes]]
id = "a"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[[nodes]]
id = "b"
parent = "a"

[nodes.transform]
translation = [0.0, 1.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]
"#;

#[test]
fn a_scene_sync_follows_the_structure_it_edits() {
    let mut scene =
        Scene::parse(SCENE, ".", "the test scene", schema()).expect("the test scene parses");
    let mut world = World::new();
    let mut sync = SceneSync::new(schema());
    sync.sync(&scene, &mut world, &NoAssets, &Changed::none());
    let a = sync.id("a").expect("the sync holds the node a");
    let b = sync.id("b").expect("the sync holds the node b");
    world.insert(a, Ticks(7));
    assert_eq!(sync.entities(), [a, b]);

    edit(&mut scene, |edit| {
        edit.set_transform(0, Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)))
    })
    .expect("the scene moves the node a");
    edit(&mut scene, |edit| {
        edit.insert_node(dome_scene::Node::new("c"))
    })
    .expect("the scene adds a node");
    let b_at = scene.node_index("b").expect("the scene holds b");
    let c_at = scene.node_index("c").expect("the scene holds c");
    edit(&mut scene, |edit| {
        edit.reparent(b_at, Some(c_at));
        Ok(())
    })
    .expect("the scene reparents the node b");
    for (at, node) in scene.nodes().iter().enumerate() {
        assert_eq!(
            scene.node_index(&node.id),
            Some(at),
            "the index names the slot of {}",
            node.id,
        );
    }
    assert_eq!(scene.children(c_at).collect::<Vec<_>>(), [b_at]);
    assert_eq!(scene.subtree(c_at), [b_at, c_at]);

    sync.sync(&scene, &mut world, &NoAssets, &Changed::none());
    assert_eq!(sync.id("a"), Some(a));
    assert_eq!(sync.id("b"), Some(b));
    assert_eq!(world.get::<Ticks>(a), Some(&Ticks(7)));
    assert_eq!(
        world.get::<LocalTransform>(a),
        Some(&LocalTransform(Transform::from_translation(Vec3::new(
            2.0, 0.0, 0.0
        )))),
    );
    let c = sync.id("c").expect("the sync spawns the added node");
    assert!(world.is_alive(c));
    assert_eq!(world.get::<Parent>(b).map(Parent::entity), Some(c));

    world.insert(
        a,
        LocalTransform(Transform::from_translation(Vec3::new(9.0, 0.0, 0.0))),
    );
    sync.invalidate();
    sync.sync(&scene, &mut world, &NoAssets, &Changed::none());
    assert_eq!(sync.id("a"), Some(a));
    assert_eq!(world.get::<Ticks>(a), Some(&Ticks(7)));
    assert_eq!(
        world.get::<LocalTransform>(a),
        Some(&LocalTransform(Transform::from_translation(Vec3::new(
            2.0, 0.0, 0.0
        )))),
    );

    edit(&mut scene, |edit| {
        edit.insert_node(dome_scene::Node::new("d"))
    })
    .expect("the scene adds the node d");
    let c_at = scene.node_index("c").expect("the scene holds c");
    let d_at = scene.node_index("d").expect("the scene holds d");
    edit(&mut scene, |edit| {
        edit.reparent(d_at, Some(c_at));
        Ok(())
    })
    .expect("the scene hangs the node d under c");
    sync.sync(&scene, &mut world, &NoAssets, &Changed::none());
    let d = sync.id("d").expect("the sync spawns the node d");
    assert_eq!(world.get::<Parent>(d).map(Parent::entity), Some(c));

    edit(&mut scene, |edit| {
        edit.remove(&Target::Node(String::from("b")))
    })
    .expect("the scene removes the node b");
    let a_at = scene.node_index("a").expect("the scene holds a");
    let d_at = scene.node_index("d").expect("the scene holds d");
    edit(&mut scene, |edit| {
        edit.reparent(d_at, Some(a_at));
        edit.remove(&Target::Node(String::from("c")))
    })
    .expect("the scene moves d out of c before it removes c");
    sync.sync(&scene, &mut world, &NoAssets, &Changed::none());
    assert!(!world.is_alive(b));
    assert!(
        world.is_alive(d),
        "a node that moved out outlives its parent"
    );
    assert_eq!(world.get::<Parent>(d).map(Parent::entity), Some(a));
    assert_eq!(children(&world, a), [d]);

    edit(&mut scene, |edit| {
        edit.set_transform(a_at, Transform::from_translation(Vec3::new(3.0, 0.0, 0.0)))
    })
    .expect("the scene moves the node a again");
    let stamp = world
        .resource::<SceneDamage>()
        .map_or(0, |held| held.stamp());
    let held_nodes = world
        .resource::<SceneDamage>()
        .map_or(0, |held| held.nodes().len());
    sync.apply(&scene, &mut world, &NoAssets, a_at);
    let damage = world
        .resource::<SceneDamage>()
        .expect("a synced scene carries the damage it renders");
    assert_eq!(
        damage.stamp(),
        stamp,
        "an applied transform leaves the authored damage to the rendered instances",
    );
    assert_eq!(
        damage.nodes().len(),
        held_nodes,
        "an applied transform damages no node the scene authored",
    );
    assert!(
        damage.nodes().contains(&b) && damage.nodes().contains(&c),
        "a publish keeps the damage an earlier publish of the same frame left",
    );
    assert_eq!(
        world.get::<LocalTransform>(a),
        Some(&LocalTransform(Transform::from_translation(Vec3::new(
            3.0, 0.0, 0.0
        )))),
        "an applied node installs the component its scene changed",
    );
    let nodes = world
        .resource::<dome_schema::NodeMap>()
        .expect("a synced scene holds the nodes it installs");
    assert_eq!(
        nodes.id("a"),
        Some(a),
        "an applied node keeps the table its sync installed",
    );
    let held = world
        .resource::<SceneDamage>()
        .expect("a synced scene carries its damage")
        .stamp();
    let mut silent = Changed::none();
    silent.insert(ReferenceKind::AudioBus, "master");
    sync.sync(&scene, &mut world, &NoAssets, &silent);
    assert_eq!(
        world.resource::<SceneDamage>().map(|held| held.stamp()),
        Some(held),
        "a change no probe can see publishes no damage",
    );
    let mut unrendered = Changed::none();
    unrendered.insert(ReferenceKind::Mesh, "cube");
    sync.sync(&scene, &mut world, &NoAssets, &unrendered);
    assert_eq!(
        world.resource::<SceneDamage>().map(|held| held.stamp()),
        Some(held),
        "a changed mesh no node renders publishes no damage",
    );
    let mut global = Changed::none();
    global.insert(ReferenceKind::Environment, "sky");
    sync.sync(&scene, &mut world, &NoAssets, &global);
    let damage = world
        .resource::<SceneDamage>()
        .expect("a rebuilt environment damages the probes");
    assert_eq!(
        damage.stamp(),
        held + 1,
        "a rebuilt environment publishes the damage the probes rebake on",
    );
    assert!(
        damage.sky(),
        "a rebuilt environment reaches every probe it lights",
    );
}
fn edit<T>(
    scene: &mut dome_scene::Scene,
    change: impl FnOnce(&mut dome_scene::Edit<'_>) -> Result<T, String>,
) -> Result<T, String> {
    scene.edit(change).map(|(value, _)| value)
}

#[test]
fn a_shared_scene_snapshot_keeps_the_nodes_it_copied() {
    let mut scene =
        Scene::parse(SCENE, ".", "the test scene", schema()).expect("the test scene parses");
    let snapshot = scene.clone();
    edit(&mut scene, |edit| {
        edit.set_transform(0, Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)))
    })
    .expect("the scene moves the node a");
    let mut world = World::new();
    let mut sync = SceneSync::new(schema());
    sync.sync(&snapshot, &mut world, &NoAssets, &Changed::none());
    let a = sync.id("a").expect("the sync holds the node a");
    assert_eq!(
        world.get::<LocalTransform>(a),
        Some(&LocalTransform(Transform::IDENTITY)),
        "an edited scene rewrites the snapshot it already copied",
    );
    sync.sync(&scene, &mut world, &NoAssets, &Changed::none());
    assert_eq!(
        world.get::<LocalTransform>(a),
        Some(&LocalTransform(Transform::from_translation(Vec3::new(
            2.0, 0.0, 0.0
        )))),
        "the edited scene installs the transform its node holds",
    );
}

fn schema() -> std::sync::Arc<dome_schema::Registry> {
    std::sync::Arc::new(dome_schema::Registry::default())
}

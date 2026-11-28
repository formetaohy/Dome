use dome_ecs::World;
use dome_math::Transform;
use dome_physics::{Collider, Gravity, RigidBody, step};
use dome_scene::Scene;
use dome_schema::observe_runtime;
use dome_toml::Toml;

const SCENE: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []

[[nodes]]
id = "ground"

[nodes.transform]
translation = [0.0, -0.5, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.collider]
shape = "cuboid"
radius = 0.5
half_extents = [10.0, 0.5, 10.0]
half_height = 0.5
center = [0.0, 0.0, 0.0]
friction = 0.5
restitution = 0.0
sensor = false
layer = 0
mask = 65535

[[nodes]]
id = "ball"

[nodes.transform]
translation = [0.0, 3.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.body]
kind = "dynamic"
mass = 1.0
gravity_scale = 1.0
linear_damping = 0.0
angular_damping = 0.0
linear_velocity = [0.0, 0.0, 0.0]
angular_velocity = [0.0, 0.0, 0.0]
can_sleep = true

[nodes.collider]
shape = "sphere"
radius = 0.5
half_extents = [0.5, 0.5, 0.5]
half_height = 0.5
center = [0.0, 0.0, 0.0]
friction = 0.5
restitution = 0.0
sensor = false
layer = 1
mask = 65535

[[nodes]]
id = "trigger"

[nodes.transform]
translation = [0.0, 2.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.collider]
shape = "cuboid"
radius = 0.5
half_extents = [1.0, 1.0, 1.0]
half_height = 0.5
center = [0.0, 0.0, 0.0]
friction = 0.5
restitution = 0.0
sensor = true
layer = 2
mask = 65535
"#;

#[test]
fn a_scene_installs_bodies_and_steps_them() {
    let scene =
        Scene::parse(SCENE, ".", "the physics scene", schema()).expect("the physics scene parses");
    let written = scene.write().expect("a scene writes its text");
    let reloaded = Scene::parse(&written, ".", "the written physics scene", schema())
        .expect("the written physics scene parses");
    assert_eq!(scene, reloaded);
    let mut world = World::new();
    world.insert_resource(Gravity::default());
    let entities = install_scene(&scene, &mut world, &Mock);
    let ball = entities[1];
    let trigger = entities[2];
    assert_eq!(
        world
            .get::<Collider>(ball)
            .expect("the ball holds its collider")
            .filter(),
        dome_physics::CollisionFilter::new(1, 65535),
        "a scene installs the layer and mask of its collider",
    );
    assert!(
        world
            .get::<Collider>(trigger)
            .expect("the trigger holds its collider")
            .is_sensor(),
        "a scene installs its sensor",
    );
    for _ in 0..240 {
        step(&mut world, 1.0 / 60.0);
    }
    let held = world
        .get::<Transform>(ball)
        .expect("the ball holds its transform");
    assert!((held.translation.y - 0.5).abs() < 0.05);
    let body = world
        .get::<RigidBody>(ball)
        .expect("the ball holds its body");
    assert!(body.is_sleeping());
    let observed = Toml::object([("nodes", observe_runtime(&world, &schema()))]);
    let text = observed.write();
    assert!(text.contains("sleeping = true"));
    assert!(text.contains("shape = \"sphere\""));
    assert!(text.contains("layer = 1"));
    assert!(text.contains("mask = 65535"));
}

struct Mock;

impl dome_schema::Assets for Mock {
    fn mesh(&self, _id: &str) -> dome_render::Mesh {
        panic!("the physics scene renders no mesh")
    }

    fn material(&self, _id: &str) -> dome_render::Material {
        panic!("the physics scene renders no material")
    }

    fn skin(&self, _id: &str, _node: &str) -> dome_scene::SkinData {
        panic!("the physics scene deforms no skin")
    }

    fn animation(
        &self,
        _id: &str,
        _targets: &[dome_scene::NodeId],
    ) -> std::sync::Arc<dome_animation::Clip> {
        panic!("the physics scene plays no animation")
    }
}

const COMPOUND: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []

[[nodes]]
id = "ground"

[nodes.transform]
translation = [0.0, -0.5, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.collider]
shape = "cuboid"
radius = 0.5
half_extents = [10.0, 0.5, 10.0]
half_height = 0.5
center = [0.0, 0.0, 0.0]
friction = 0.5
restitution = 0.0
sensor = false
layer = 0
mask = 65535

[[nodes]]
id = "rig"

[nodes.transform]
translation = [0.0, 3.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.body]
kind = "dynamic"
mass = 1.0
gravity_scale = 1.0
linear_damping = 0.0
angular_damping = 0.0
linear_velocity = [0.0, 0.0, 0.0]
angular_velocity = [0.0, 0.0, 0.0]
can_sleep = true

[[nodes]]
id = "part"
parent = "rig"

[nodes.transform]
translation = [1.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.collider]
shape = "sphere"
radius = 0.5
half_extents = [0.5, 0.5, 0.5]
half_height = 0.5
center = [0.0, 0.0, 0.0]
friction = 0.5
restitution = 0.0
sensor = false
layer = 0
mask = 65535
"#;

#[test]
fn a_scene_composes_the_colliders_of_a_node_into_its_body() {
    let scene = Scene::parse(COMPOUND, ".", "the compound scene", schema())
        .expect("the compound scene parses");
    let written = scene.write().expect("a scene writes its text");
    assert_eq!(
        Scene::parse(&written, ".", "the written compound scene", schema())
            .expect("the written compound scene parses"),
        scene,
    );
    let mut world = World::new();
    world.insert_resource(Gravity::default());
    let entities = install_scene(&scene, &mut world, &Mock);
    let rig = entities[1];
    let part = entities[2];
    assert!(
        world.get::<RigidBody>(part).is_none(),
        "a collider below a body holds no body of its own",
    );
    for _ in 0..300 {
        step(&mut world, 1.0 / 60.0);
    }
    let rig = world
        .get::<Transform>(rig)
        .expect("the rig holds its transform");
    let part = world
        .get::<Transform>(part)
        .expect("the part holds its transform");
    assert!(
        (rig.translation - dome_math::Vec3::new(0.0, 0.5, 0.0)).length() < 0.05,
        "the body holds its origin a part aside at {:?}",
        rig.translation,
    );
    assert!(
        (part.translation - dome_math::Vec3::new(1.0, 0.5, 0.0)).length() < 0.05,
        "the part rests under the body at {:?}",
        part.translation,
    );
}

struct Meshes;

impl dome_schema::Assets for Meshes {
    fn mesh(&self, _id: &str) -> dome_render::Mesh {
        panic!("the mesh scene renders with a renderer")
    }

    fn material(&self, _id: &str) -> dome_render::Material {
        panic!("the mesh scene renders no material")
    }

    fn skin(&self, _id: &str, _node: &str) -> dome_scene::SkinData {
        panic!("the mesh scene deforms no skin")
    }

    fn animation(
        &self,
        _id: &str,
        _targets: &[dome_scene::NodeId],
    ) -> std::sync::Arc<dome_animation::Clip> {
        panic!("the mesh scene plays no animation")
    }

    fn mesh_shape(&self, id: &str) -> std::sync::Arc<dome_physics::TriangleMesh> {
        assert_eq!(id, "ground");
        std::sync::Arc::new(dome_physics::TriangleMesh::from_positions(&[
            [-10.0, 0.0, -10.0],
            [-10.0, 0.0, 10.0],
            [10.0, 0.0, 10.0],
            [-10.0, 0.0, -10.0],
            [10.0, 0.0, 10.0],
            [10.0, 0.0, -10.0],
        ]))
    }
}

const MESH: &str = r#"format = "dome_scene"
images = []
materials = []

[[meshes]]
id = "ground"

[[meshes.vertices]]
position = [-10.0, 0.0, -10.0]
normal = [0.0, 1.0, 0.0]
uv = [0.0, 0.0]

[[meshes.vertices]]
position = [-10.0, 0.0, 10.0]
normal = [0.0, 1.0, 0.0]
uv = [0.0, 1.0]

[[meshes.vertices]]
position = [10.0, 0.0, 10.0]
normal = [0.0, 1.0, 0.0]
uv = [1.0, 1.0]

[[nodes]]
id = "ground"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.collider]
shape = "mesh"
mesh = "ground"
radius = 0.5
half_extents = [0.5, 0.5, 0.5]
half_height = 0.5
center = [0.0, 0.0, 0.0]
friction = 0.5
restitution = 0.0
sensor = false
layer = 0
mask = 65535

[[nodes]]
id = "ball"

[nodes.transform]
translation = [0.0, 3.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.body]
kind = "dynamic"
mass = 1.0
gravity_scale = 1.0
linear_damping = 0.0
angular_damping = 0.0
linear_velocity = [0.0, 0.0, 0.0]
angular_velocity = [0.0, 0.0, 0.0]
can_sleep = true

[nodes.collider]
shape = "sphere"
radius = 0.5
half_extents = [0.5, 0.5, 0.5]
half_height = 0.5
center = [0.0, 0.0, 0.0]
friction = 0.5
restitution = 0.0
sensor = false
layer = 0
mask = 65535
"#;

#[test]
fn a_scene_collides_the_triangles_of_its_mesh() {
    let scene = Scene::parse(MESH, ".", "the mesh scene", schema()).expect("the mesh scene parses");
    let written = scene.write().expect("a scene writes its text");
    assert!(written.contains("shape = \"mesh\""));
    assert!(written.contains("mesh = \"ground\""));
    assert_eq!(
        Scene::parse(&written, ".", "the written mesh scene", schema())
            .expect("the written mesh scene parses"),
        scene
    );
    let mut edited = Scene::parse(MESH, ".", "the edited mesh scene", schema())
        .expect("the edited mesh scene parses");
    let ground = dome_scene::Target::Node(String::from("ground"));
    assert_eq!(
        edit(&mut edited, |edit| {
            edit.set_property(
                &ground,
                "collider.shape",
                &dome_scene::Value::Text(String::from("cuboid")),
            )
        }),
        Ok(dome_scene::Effect::Value),
    );
    assert_eq!(
        edit(&mut edited, |edit| {
            edit.set_property(
                &ground,
                "collider.shape",
                &dome_scene::Value::Text(String::from("mesh")),
            )
        }),
        Ok(dome_scene::Effect::Structure),
    );
    assert_eq!(
        edit(&mut edited, |edit| {
            edit.set_property(
                &ground,
                "collider.mesh",
                &dome_scene::Value::Text(String::from("ground")),
            )
        }),
        Ok(dome_scene::Effect::Structure),
    );
    let mut world = World::new();
    world.insert_resource(Gravity::default());
    let entities = install_scene(&scene, &mut world, &Meshes);
    for _ in 0..240 {
        step(&mut world, 1.0 / 60.0);
    }
    let ball = world
        .get::<Transform>(entities[1])
        .expect("the ball holds its transform");
    assert!(
        (ball.translation.y - 0.5).abs() < 0.05,
        "a scene rests its ball on a mesh collider at {:?}",
        ball.translation,
    );
    let observed = Toml::object([("nodes", observe_runtime(&world, &schema()))]);
    let text = observed.write();
    assert!(text.contains("shape = \"mesh\""));
}
const CYCLING: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []

[[nodes]]
id = "first"
parent = "second"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.body]
kind = "static"
mass = 1.0
gravity_scale = 1.0
linear_damping = 0.0
angular_damping = 0.0
linear_velocity = [0.0, 0.0, 0.0]
angular_velocity = [0.0, 0.0, 0.0]
can_sleep = true

[[nodes]]
id = "second"
parent = "first"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]
"#;

#[test]
fn a_scene_that_closes_its_parent_chain_answers_before_a_component_reads_the_chain() {
    assert_eq!(
        Scene::parse(CYCLING, ".", "the cyclic scene", schema()).expect_err("a cycle is refused"),
        "the cyclic scene closes the parent chain of first into a cycle",
    );
}

fn edit<T>(
    scene: &mut dome_scene::Scene,
    change: impl FnOnce(&mut dome_scene::Edit<'_>) -> Result<T, String>,
) -> Result<T, String> {
    scene.edit(change).map(|(value, _)| value)
}

fn schema() -> std::sync::Arc<dome_schema::Registry> {
    std::sync::Arc::new(dome_schema::Registry::default())
}

fn install_scene(
    scene: &Scene,
    world: &mut dome_ecs::World,
    assets: &dyn dome_schema::Assets,
) -> Vec<dome_ecs::Entity> {
    let mut sync = dome_schema::SceneSync::new(schema());
    sync.sync(scene, world, assets, &dome_scene::Changed::none());
    sync.entities().to_vec()
}

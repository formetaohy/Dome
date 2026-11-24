use dome_ecs::World;
use dome_math::Transform;
use dome_physics::{Coordinate, Gravity, step};
use dome_scene::{Scene, Target, Value};
use dome_schema::JointRef;
use dome_schema::NodeComponents;
use dome_schema::{observe_runtime, set_runtime_property};
use dome_toml::Toml;

const SCENE: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []

[[nodes]]
id = "anchor"

[nodes.transform]
translation = [0.0, 3.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.joint]
other = "ball"
kind = "hinge"
anchor = [0.0, 0.0, 0.0]
other_anchor = [0.0, 2.0, 0.0]
axis = [0.0, 0.0, 1.0]
rest_length = 0.0
collide = false
limit = true
lower = 0.0
upper = 0.5235988
cone = 0.0
motor = true
motor_speed = 1.0
motor_force = 100.0

[[nodes]]
id = "ball"

[nodes.transform]
translation = [2.0, 3.0, 0.0]
rotation = [0.0, 0.0, 0.70710677, 0.70710677]
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
fn a_scene_installs_and_steps_the_joints_it_authors() {
    let scene =
        Scene::parse(SCENE, ".", "the joint scene", schema()).expect("the joint scene parses");
    let written = scene.write().expect("a scene writes its text");
    let reloaded = Scene::parse(&written, ".", "the written joint scene", schema())
        .expect("the written joint scene parses");
    assert_eq!(scene, reloaded);
    let mut world = World::new();
    world.insert_resource(Gravity::default());
    let entities = install_scene(&scene, &mut world, &Mock);
    let anchor = entities[0];
    let ball = entities[1];
    assert!(
        world.get::<dome_physics::Joint>(anchor).is_some(),
        "a scene installs the joint of its node",
    );
    assert_eq!(
        scene.nodes()[0]
            .component::<JointRef>()
            .expect("the anchor holds its joint")
            .other
            .as_ref()
            .map(|other| other.as_str()),
        Some("ball"),
    );
    for _ in 0..120 {
        step(&mut world, 1.0 / 60.0);
    }
    let held = world
        .get::<Transform>(anchor)
        .expect("the anchor holds its transform");
    let ball = world
        .get::<Transform>(ball)
        .expect("the ball holds its transform")
        .translation;
    let reach = ball.distance(held.translation);
    assert!(
        (reach - 2.0).abs() < 0.05,
        "the scene hinge joint holds a span of {reach}",
    );
    let driven = world
        .get::<dome_physics::Joint>(anchor)
        .expect("the anchor holds its joint");
    assert!(
        (driven.coordinate() - std::f32::consts::FRAC_PI_6).abs() < 0.01,
        "the scene joint drives its coordinate to its limit at {}",
        driven.coordinate(),
    );
    let observed = Toml::object([("nodes", observe_runtime(&world, &schema()))]);
    let text = observed.write();
    assert!(text.contains("kind = \"hinge\""));
    assert!(text.contains("other = \"ball\""));
    assert!(text.contains("limit = true"));
    assert!(text.contains("motor = true"));
    assert!(text.contains("cone = 0.0"));
    set_runtime_property(
        &mut world,
        &schema(),
        &Target::Node(String::from("anchor")),
        "joint.motor_speed",
        &Value::Number(0.0),
    )
    .expect("the runtime drives the motor of a joint");
    let driven = world
        .get::<dome_physics::Joint>(anchor)
        .expect("the anchor holds its joint");
    assert_eq!(
        driven.motor(Coordinate::Twist).map(|motor| motor.speed()),
        Some(0.0),
    );

    let spherical = SCENE
        .replace("kind = \"hinge\"", "kind = \"spherical\"")
        .replace("axis = [0.0, 0.0, 1.0]", "axis = [1.0, 0.0, 0.0]")
        .replace("limit = true", "limit = false")
        .replace("motor = true", "motor = false")
        .replace("cone = 0.0", "cone = 0.5");
    let scene = Scene::parse(&spherical, ".", "the spherical scene", schema())
        .expect("the spherical scene parses");
    let mut world = World::new();
    world.insert_resource(Gravity::default());
    let anchor = install_scene(&scene, &mut world, &Mock)[0];
    for _ in 0..240 {
        step(&mut world, 1.0 / 60.0);
    }
    let driven = world
        .get::<dome_physics::Joint>(anchor)
        .expect("the anchor holds its joint");
    let cone = driven
        .limit(Coordinate::Swing)
        .expect("a spherical joint limits its swing");
    assert!(
        (cone.upper() - 0.5).abs() < 1e-6,
        "the scene limits the swing of its joint to {}",
        cone.upper(),
    );
    let swing = driven.value(Coordinate::Swing);
    assert!(
        (swing - 0.5).abs() < 0.05,
        "the scene joint holds a swing of {swing}",
    );
    let observed = Toml::object([("nodes", observe_runtime(&world, &schema()))]);
    let text = observed.write();
    assert!(text.contains("cone = 28.64"));
    assert!(text.contains("swing = 28."));
}

struct Mock;

impl dome_schema::Assets for Mock {
    fn mesh(&self, _id: &str) -> dome_render::Mesh {
        panic!("the joint scene renders no mesh")
    }

    fn material(&self, _id: &str) -> dome_render::Material {
        panic!("the joint scene renders no material")
    }

    fn skin(&self, _id: &str) -> dome_scene::SkinData {
        panic!("the joint scene deforms no skin")
    }

    fn animation(&self, _id: &str) -> std::sync::Arc<dome_animation::Clip> {
        panic!("the joint scene plays no animation")
    }
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

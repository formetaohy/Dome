use dome_ecs::World;
use dome_gpu::{
    BufferUsages, Gpu, GpuBuffer, GpuRequest, Submission, Texture, TextureFormat, TextureUsages,
};
use dome_render::{Renderer, Target};
use dome_scene::Scene;
use dome_schema::SceneNode;

const SIDE: u32 = 64;

const SCENE: &str = r#"format = "dome_scene"
images = []

[[meshes]]
id = "cube"
shape = "cube"

[[materials]]
id = "red"
metallic = 0.0
roughness = 0.8

base_color = [1.0, 0.0, 0.0]
alpha = 1.0
alpha_mode = "opaque"
alpha_cutoff = 0.5
normal_scale = 1.0
occlusion_strength = 1.0
emissive = [0.0, 0.0, 0.0]

[[nodes]]
id = "camera"

[nodes.transform]
translation = [0.0, 0.0, 3.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.camera]
vertical_fov = 1.5707963
near = 0.1
far = 100.0

[[nodes]]
id = "sun"

[nodes.transform]
translation = [0.0, 2.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.directional_light]
color = [1.0, 1.0, 1.0]
intensity = 3.1415927
shadow_distance = 100.0

[[nodes]]
id = "rig"

[nodes.transform]
translation = [0.0, 1.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[[nodes]]
id = "cube"
parent = "rig"

[nodes.transform]
translation = [0.0, -1.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.mesh]
mesh = "cube"
material = "red"
tint = [1.0, 1.0, 1.0]

[[nodes]]
id = "hidden"
parent = "rig"

[nodes.transform]
translation = [0.5, -1.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.mesh]
mesh = "cube"
material = "red"
tint = [1.0, 1.0, 1.0]

[nodes.hidden]
"#;

#[test]
fn a_scene_instantiates_into_a_visible_world() {
    paint();
}

fn paint() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device renders a scene: {error}"));
    let target = Texture::new(
        context.device(),
        "scene frame",
        SIDE,
        SIDE,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let scene =
        Scene::parse(SCENE, ".", "the test scene", schema()).expect("the test scene parses");
    let mut world = World::new();
    let entities = instantiate_scene(&scene, &mut world, &mut renderer);
    assert_eq!(world.count::<SceneNode>(), 5);
    assert_eq!(
        world
            .get::<dome_math::Transform>(entities[3])
            .expect("the cube holds its world transform")
            .translation,
        dome_math::Vec3::ZERO,
    );
    renderer.render(
        &mut world,
        Target::Texture(&target),
        [0.0, 0.0, 0.0, 1.0],
        None,
    );
    assert_eq!(
        renderer.visible_instances(),
        1,
        "draws the cube of the scene",
    );
    let pixels = read(&context, &target);
    assert!(
        pixels.iter().any(|pixel| pixel[0] > 32),
        "paints no lit cube of the scene",
    );
}

fn read(context: &Gpu, target: &Texture) -> Vec<[u8; 4]> {
    let queue = context.queue();
    let readback = GpuBuffer::new(
        context.device(),
        "scene readback",
        target.readback_bytes(),
        BufferUsages::COPY_DST | BufferUsages::MAP_READ,
    );
    let mut submission = Submission::new(context.device(), "scene readback");
    submission.copy_texture(target, &readback, 0);
    let index = submission.submit(&queue);
    let bytes = readback.read(&queue, index, target.readback_bytes());
    target
        .rows(&bytes)
        .flat_map(|row| row.as_chunks::<4>().0)
        .copied()
        .collect()
}

fn schema() -> std::sync::Arc<dome_schema::Registry> {
    std::sync::Arc::new(dome_schema::Registry::default())
}

fn instantiate_scene(
    scene: &Scene,
    world: &mut dome_ecs::World,
    renderer: &mut dome_render::Renderer,
) -> Vec<dome_ecs::Entity> {
    let library = dome_asset::Library::build(scene, renderer);
    install_scene(scene, world, &library)
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

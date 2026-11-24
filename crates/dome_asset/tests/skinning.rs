use dome_animation::AnimationMachine;
use dome_asset::Import;
use dome_ecs::{World, propagate};
use dome_gpu::{
    BufferUsages, Gpu, GpuBuffer, GpuRequest, Submission, Texture, TextureFormat, TextureUsages,
};
use dome_math::{Quat, Transform, Vec3};
use dome_render::{Camera, DirectionalLight, Renderer, Skin, Target};
use dome_scene::{Node, Scene};
use dome_schema::NodeComponents;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

const SIDE: u32 = 64;
const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

#[test]
fn an_imported_skin_and_animation_deform_into_a_rendered_frame() {
    paint();
}

fn paint() {
    let root = directory();
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("assets")).expect("a test opens its directory");
    let binary = skinned_triangle();
    fs::write(root.join("assets/model.bin"), &binary).expect("a test writes a buffer");
    fs::write(
        root.join("assets/model.gltf"),
        serde_json::to_vec(&document(binary.len())).expect("a test writes JSON"),
    )
    .expect("a test writes a model");

    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device renders a skin: {error}"));
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let target = Texture::new(
        context.device(),
        "skinned scene frame",
        SIDE,
        SIDE,
        1,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let mut scene = Scene::new(&root, schema());
    edit(&mut scene, |edit| {
        edit.import_model("rig", "assets/model.gltf", None)
    })
    .expect("a scene imports a skinned model");
    edit(&mut scene, |edit| {
        edit.insert_node(
            placed(
                Node::new("camera"),
                Transform::from_translation(Vec3::new(0.0, 0.0, 3.0)),
            )
            .with(Camera::new(std::f32::consts::FRAC_PI_2, 0.1, 100.0)),
        )
    })
    .expect("a test adds its camera");
    edit(&mut scene, |edit| {
        edit.insert_node(
            placed(
                Node::new("sun"),
                Transform::from_rotation(Quat::from_rotation_x(-0.8)),
            )
            .with(DirectionalLight::new(
                Vec3::ONE,
                std::f32::consts::PI,
                100.0,
            )),
        )
    })
    .expect("a test adds its light");
    let mut world = World::new();
    instantiate_scene(&scene, &mut world, &mut renderer);
    assert_eq!(world.count::<Skin>(), 1);
    assert_eq!(world.count::<AnimationMachine>(), 1);
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    assert_eq!(renderer.pick(dome_math::Vec2::new(45.0, 40.0)), None);
    dome_animation::advance(&mut world, 0.5);
    propagate(&mut world);
    renderer.render(&mut world, Target::Texture(&target), CLEAR, None);
    assert!(
        renderer.pick(dome_math::Vec2::new(45.0, 40.0)).is_some(),
        "picks the animated skin over its rest pose",
    );
    let pixels = read(&context, &target);
    assert!(
        pixels[(40 * SIDE + 45) as usize][0] > 16,
        "paints the animated skin",
    );
    let _ = fs::remove_dir_all(&root);
}

fn document(length: usize) -> serde_json::Value {
    json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [0, 1] }],
        "nodes": [
            { "mesh": 0, "skin": 0 },
            { "name": "joint" },
        ],
        "meshes": [{
            "primitives": [{
                "attributes": { "POSITION": 0, "JOINTS_0": 1, "WEIGHTS_0": 2 },
            }],
        }],
        "skins": [{ "joints": [1], "inverseBindMatrices": 3 }],
        "animations": [{
            "name": "bend",
            "channels": [{
                "sampler": 0,
                "target": { "node": 1, "path": "translation" },
            }],
            "samplers": [{ "input": 4, "output": 5, "interpolation": "LINEAR" }],
        }],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" },
            { "bufferView": 1, "componentType": 5123, "count": 3, "type": "VEC4" },
            { "bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC4" },
            { "bufferView": 3, "componentType": 5126, "count": 1, "type": "MAT4" },
            { "bufferView": 4, "componentType": 5126, "count": 2, "type": "SCALAR" },
            { "bufferView": 5, "componentType": 5126, "count": 2, "type": "VEC3" },
        ],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
            { "buffer": 0, "byteOffset": 36, "byteLength": 24 },
            { "buffer": 0, "byteOffset": 60, "byteLength": 48 },
            { "buffer": 0, "byteOffset": 108, "byteLength": 64 },
            { "buffer": 0, "byteOffset": 172, "byteLength": 8 },
            { "buffer": 0, "byteOffset": 180, "byteLength": 24 },
        ],
        "buffers": [{ "uri": "model.bin", "byteLength": length }],
    })
}

fn skinned_triangle() -> Vec<u8> {
    let mut binary = Vec::new();
    for value in [-1.0f32, -1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0, 0.0] {
        binary.extend(value.to_le_bytes());
    }
    for _ in 0..3 {
        for joint in [0u16, 0, 0, 0] {
            binary.extend(joint.to_le_bytes());
        }
    }
    for _ in 0..3 {
        for weight in [1.0f32, 0.0, 0.0, 0.0] {
            binary.extend(weight.to_le_bytes());
        }
    }
    for value in [
        1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ] {
        binary.extend(value.to_le_bytes());
    }
    for value in [0.0f32, 1.0] {
        binary.extend(value.to_le_bytes());
    }
    for value in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0] {
        binary.extend(value.to_le_bytes());
    }
    binary
}

fn read(context: &Gpu, target: &Texture) -> Vec<[u8; 4]> {
    let queue = context.queue();
    let readback = GpuBuffer::new(
        context.device(),
        "skinned readback",
        target.readback_bytes(),
        BufferUsages::COPY_DST | BufferUsages::MAP_READ,
    );
    let mut submission = Submission::new(context.device(), "skinned readback");
    submission.copy_texture(target, &readback, 0);
    let index = submission.submit(&queue);
    let bytes = readback.read(&queue, index, target.readback_bytes());
    target
        .rows(&bytes)
        .flat_map(|row| row.as_chunks::<4>().0)
        .copied()
        .collect()
}

fn directory() -> PathBuf {
    std::env::temp_dir().join(format!("dome_skinning_{}", std::process::id()))
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

fn placed(mut node: dome_scene::Node, transform: Transform) -> dome_scene::Node {
    node.set_transform(transform);
    node
}

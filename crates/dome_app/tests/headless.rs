use dome_app::{App, AppConfig, HeadlessConfig, Time};
use dome_ecs::World;
use dome_image::Image;
use dome_scene::SceneNode;
use std::fs;
use std::path::PathBuf;

const SCENE: &str = r#"format = "dome_scene"
images = []
environments = [{ id = "sky", path = "sky.hdr" }]

[[meshes]]
id = "cube"
shape = "cube"

[[materials]]
id = "grey"
metallic = 0.0
roughness = 0.5
base_color = [0.8, 0.8, 0.8]
alpha = 1.0
alpha_mode = "opaque"
alpha_cutoff = 0.5
normal_scale = 1.0
occlusion_strength = 1.0
emissive = [0.0, 0.0, 0.0]

[[nodes]]
id = "camera"

[nodes.transform]
translation = [0.0, 1.0, 4.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.camera]
vertical_fov = 1.0
near = 0.1
far = 100.0

[[nodes]]
id = "sun"

[nodes.transform]
translation = [0.0, 8.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.directional_light]
color = [1.0, 1.0, 1.0]
intensity = 3.0
shadow_distance = 50.0

[[nodes]]
id = "sky"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.environment]
environment = "sky"
intensity = 1.0
sky = true
local = false
blend = 1.0

[[nodes]]
id = "cube"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.mesh]
mesh = "cube"
material = "grey"
tint = [1.0, 1.0, 1.0]
"#;

const CAMERA_SCENE: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []

[[nodes]]
id = "camera"

[nodes.transform]
translation = [0.0, 1.0, 4.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.camera]
vertical_fov = 1.0
near = 0.1
far = 100.0
"#;

const WATCH_SCENE: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []

[[nodes]]
id = "camera"

[nodes.transform]
translation = [0.0, 1.0, 4.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.camera]
vertical_fov = 1.0
near = 0.1
far = 100.0

[[nodes]]
id = "prop"
prefab = "prop.toml"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]
"#;

const WATCH_BROKEN: &str = r#"format = "dome_prefab"
[[nodes]]
id = "body"
"#;

const WATCH_PREFAB: &str = r#"format = "dome_prefab"
images = []
meshes = []
materials = []

[[nodes]]
id = "body"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]
"#;

const WATCH_PREFAB_EXTENDED: &str = r#"format = "dome_prefab"
images = []
meshes = []
materials = []

[[nodes]]
id = "body"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[[nodes]]
id = "extra"
parent = "body"

[nodes.transform]
translation = [1.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]
"#;

#[test]
fn a_headless_run_renders_its_scene() {
    let root = directory("render");
    let scene = root.join("scene.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a headless test opens its directory");
    fs::write(&scene, SCENE).expect("a headless test writes its scene");
    fs::write(root.join("sky.hdr"), radiance()).expect("a headless test writes its environment");

    let mut app = App::with_config(AppConfig {
        scene: Some(scene),
        audio: false,
        ..Default::default()
    });
    let png = app
        .run_headless(HeadlessConfig {
            width: 64,
            height: 64,
            frames: Some(2),
            realtime: false,
            screenshot: true,
        })
        .expect("a captured headless run holds its frame");

    let image = Image::decode(&png, "the captured headless frame");
    assert_eq!((image.width(), image.height()), (64, 64));
    assert!(
        image
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[1] > 200 && pixel[0] < 80 && pixel[2] < 80),
        "a headless frame paints the environment as its sky",
    );
    let _ = fs::remove_dir_all(&root);
}

struct Steps(u32);

#[test]
fn a_headless_run_steps_its_fixed_schedule() {
    let root = directory("steps");
    let scene = root.join("scene.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a headless test opens its directory");
    fs::write(&scene, CAMERA_SCENE).expect("a headless test writes its scene");

    let mut app = App::with_config(AppConfig {
        scene: Some(scene),
        audio: false,
        ..Default::default()
    });
    app.world_mut().insert_resource(Steps(0));
    app.add_fixed(|world: &mut World| {
        world
            .resource_mut::<Steps>()
            .expect("a fixed step counts")
            .0 += 1;
    });

    let png = app.run_headless(HeadlessConfig {
        width: 8,
        height: 8,
        frames: Some(3),
        realtime: false,
        screenshot: false,
    });

    assert!(png.is_none(), "an uncaptured headless run holds no frame");
    assert_eq!(
        app.world()
            .resource::<Steps>()
            .expect("a fixed step counts")
            .0,
        3,
    );
    assert_eq!(
        app.world()
            .resource::<Time>()
            .expect("an application keeps its time")
            .frame(),
        3,
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_headless_run_follows_a_changed_prefab_and_keeps_the_last_good_scene() {
    let root = directory("watch");
    let scene = root.join("scene.toml");
    let prefab = root.join("prop.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a headless test opens its directory");
    fs::write(&scene, WATCH_SCENE).expect("a headless test writes its scene");
    fs::write(&prefab, WATCH_PREFAB).expect("a headless test writes its prefab");

    let mut app = App::with_config(AppConfig {
        scene: Some(scene),
        audio: false,
        ..Default::default()
    });
    let mut frame = 0;
    app.add_system(move |_: &mut World| {
        frame += 1;
        if frame == 1 {
            fs::write(&prefab, WATCH_PREFAB_EXTENDED)
                .expect("a headless system extends the prefab");
        }
        if frame == 5 {
            fs::write(&prefab, WATCH_BROKEN).expect("a headless system breaks the prefab");
        }
    });
    app.run_headless(HeadlessConfig {
        width: 32,
        height: 32,
        frames: Some(8),
        realtime: false,
        screenshot: false,
    });

    let ids = node_ids(app.world());
    assert!(ids.iter().any(|id| id == "prop_body"));
    assert!(
        ids.iter().any(|id| id == "prop_extra"),
        "a reload follows the prefab it changed",
    );
    assert!(
        app.reload_error()
            .is_some_and(|error| error.starts_with("reload: ")),
        "a broken prefab reports its reload error",
    );
    assert!(
        ids.iter().any(|id| id == "prop_body") && ids.iter().any(|id| id == "prop_extra"),
        "a broken prefab keeps the scene it last installed",
    );
    let _ = fs::remove_dir_all(&root);
}

fn node_ids(world: &World) -> Vec<String> {
    world
        .iter::<SceneNode>()
        .map(|(_, node)| node.id.clone())
        .collect()
}

fn radiance() -> Vec<u8> {
    let mut bytes = b"#?RADIANCE
FORMAT=32-bit_rle_rgbe

-Y 2 +X 4
"
    .to_vec();
    for _ in 0..8 {
        bytes.extend_from_slice(&[0, 255, 0, 128]);
    }
    bytes
}

fn directory(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("dome_app_{name}_{}", std::process::id()))
}

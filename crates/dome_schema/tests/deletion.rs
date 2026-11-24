use dome_scene::{Scene, Target};
use std::fs;
use std::path::PathBuf;

const PROP: &str = r#"format = "dome_prefab"
images = []

[[meshes]]
id = "shell"
shape = "cube"

[[materials]]
id = "skin"
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
id = "body"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.mesh]
mesh = "shell"
material = "skin"
tint = [1.0, 1.0, 1.0]
"#;

const INSTANCE: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []

[[nodes]]
id = "prop"
prefab = "prefabs/prop.toml"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]
"#;

const REFERENCED: &str = r#"format = "dome_scene"
images = []

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

const ARMATURE: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []

[[nodes]]
id = "rig"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[[nodes]]
id = "tip"
parent = "rig"

[nodes.transform]
translation = [0.0, 1.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[[nodes]]
id = "arm"

[nodes.transform]
translation = [2.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.joint]
other = "tip"
kind = "hinge"
anchor = [0.0, 0.0, 0.0]
other_anchor = [0.0, 0.0, 0.0]
axis = [0.0, 0.0, 1.0]
rest_length = 0.0
collide = false
limit = false
lower = 0.0
upper = 0.0
cone = 0.0
motor = false
motor_speed = 0.0
motor_force = 0.0
"#;

#[test]
fn deleting_a_prefab_instance_prunes_the_assets_it_owns() {
    let root = directory("deletion_instance");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("prefabs")).expect("a test opens its directory");
    fs::write(root.join("prefabs/prop.toml"), PROP).expect("a test writes its prefab");
    fs::write(root.join("scene.toml"), INSTANCE).expect("a test writes its scene");
    let mut scene = Scene::load(root.join("scene.toml"), &root, schema());
    assert_eq!(
        scene.meshes().len(),
        1,
        "a scene holds the mesh of the prefab it instances",
    );
    assert_eq!(
        scene.materials().len(),
        1,
        "a scene holds the material of the prefab it instances",
    );
    assert!(
        scene.node_index("prop_body").is_some(),
        "a prefab instance expands the nodes of its prefab",
    );
    edit(&mut scene, |edit| {
        edit.remove(&Target::Node(String::from("prop")))
    })
    .expect("a prefab instance deletes");
    assert!(
        scene.node_index("prop").is_none(),
        "the deleted prefab instance leaves no node",
    );
    assert!(
        scene.node_index("prop_body").is_none(),
        "the deleted prefab instance leaves none of its nodes",
    );
    assert!(
        scene.meshes().is_empty(),
        "the deleted prefab instance owns the mesh it rendered",
    );
    assert!(
        scene.materials().is_empty(),
        "the deleted prefab instance owns the material it rendered",
    );
    assert!(
        scene
            .asset_owner(dome_scene::ReferenceKind::Mesh, "shell")
            .is_none(),
        "the deleted prefab instance keeps no owner of the mesh it held",
    );
}

#[test]
fn a_refused_deletion_leaves_the_scene_as_it_stood() {
    let mut scene = Scene::parse(REFERENCED, ".", "a referenced scene", schema());
    let before = scene.write().expect("a scene writes its text");
    assert!(
        edit(&mut scene, |edit| edit
            .remove(&Target::Mesh(String::from("cube"))))
        .is_err(),
        "a scene refuses to delete the mesh a node renders",
    );
    assert_eq!(
        scene.write().expect("a scene writes its text"),
        before,
        "a refused deletion changes no node and no asset",
    );
    let baked = scene.write().expect("a scene writes its text");
    edit(&mut scene, |edit| {
        edit.remove(&Target::Node(String::from("cube")))
    })
    .expect("an unreferenced node deletes");
    assert_ne!(
        scene.write().expect("a scene writes its text"),
        baked,
        "a deletion that stands writes its own scene",
    );

    let mut rigged = Scene::parse(ARMATURE, ".", "an armature scene", schema());
    let stood = rigged.write().expect("a rigged scene writes its text");
    assert!(
        edit(&mut rigged, |edit| edit
            .remove(&Target::Node(String::from("rig"))))
        .is_err(),
        "a scene refuses to delete a subtree a joint of another node reaches into",
    );
    assert_eq!(
        rigged.write().expect("a rigged scene writes its text"),
        stood,
        "a refused subtree deletion leaves the scene as it stood",
    );
    edit(&mut rigged, |edit| {
        edit.remove(&Target::Node(String::from("arm")))
    })
    .expect("the node that holds the joint deletes");
    edit(&mut rigged, |edit| {
        edit.remove(&Target::Node(String::from("rig")))
    })
    .expect("a subtree no other node reaches into deletes");
}

fn directory(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("dome_scene_{name}_{}", std::process::id()))
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

use dome_scene::Scene;
use dome_toml::Toml;
use std::fs;
use std::path::PathBuf;

const PREFAB: &str = r#"format = "dome_prefab"
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

const SCENE: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []

[[models]]
id = "rig"
path = "rig.glb"

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

[[graphs]]
id = "rig_graph"
entry = "idle"

[[graphs.states]]
id = "idle"
clip = "rig_animation_0"
speed = 1.0
looped = true

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

[[nodes]]
id = "prop"
prefab = "prop.toml"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]
"#;

#[test]
fn an_observed_scene_holds_every_asset_and_every_node_it_installed() {
    let root = directory("observe");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a test opens its directory");
    fs::write(root.join("prop.toml"), PREFAB).expect("a test writes its prefab");
    fs::write(root.join("scene.toml"), SCENE).expect("a test writes its scene");
    let mut scene = Scene::load(root.join("scene.toml"));
    edit(&mut scene, |edit| {
        edit.insert_child("prop", dome_scene::Node::new("antenna"))
    })
    .expect("an instance takes an addition");

    let fields = scene.observe().object_fields("an observed scene");
    for name in [
        "models",
        "images",
        "sounds",
        "environments",
        "meshes",
        "materials",
        "skins",
        "animations",
        "graphs",
        "nodes",
    ] {
        assert!(
            field(&fields, name).is_array(),
            "an observed scene holds {name}",
        );
    }
    assert_eq!(
        field(&fields, "skins")
            .array_items("the skins of an observed scene")
            .len(),
        1,
    );
    assert_eq!(
        field(&fields, "animations")
            .array_items("the animations of an observed scene")
            .len(),
        1,
    );

    let mut instance = None;
    let mut materialized = 0;
    for node in field(&fields, "nodes").array_items("the nodes of an observed scene") {
        let node = node.object_fields("an observed node");
        let id = field(&node, "id").string_value("the id of an observed node");
        if id == "prop" {
            instance = Some(node);
        } else if id.starts_with("prop_") {
            materialized += 1;
            assert_eq!(
                field(&node, "owner").string_value("the owner of an observed node"),
                "prop",
            );
        }
    }
    assert_eq!(
        materialized, 2,
        "an observed scene holds the nodes its instance materialized",
    );
    let instance = instance.expect("an observed scene holds the prefab instance");
    let additions = field(&instance, "additions").array_items("the additions of an instance");
    assert_eq!(additions.len(), 1);
    let addition = additions
        .into_iter()
        .next()
        .expect("an observed instance holds one addition")
        .object_fields("an observed addition");
    assert_eq!(
        field(&addition, "id").string_value("the id of an observed addition"),
        "antenna",
    );
    assert!(
        field(&addition, "transform").is_table(),
        "an observed addition is a node of components",
    );
    let _ = fs::remove_dir_all(&root);
}

fn field(fields: &[(String, Toml)], name: &str) -> Toml {
    fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
        .unwrap_or_else(|| panic!("the observed document holds no {name}"))
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

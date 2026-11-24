use dome_scene::{Scene, Shape, Target};
use dome_schema::NodeComponents;
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

#[test]
fn an_edit_transaction_returns_the_scene_it_edited() {
    let root = directory("edit_revert");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("prefabs")).expect("a test opens its directory");
    fs::write(root.join("prefabs/prop.toml"), PROP).expect("a test writes its prefab");
    fs::write(root.join("scene.toml"), INSTANCE).expect("a test writes its scene");
    let mut scene = Scene::load(root.join("scene.toml"), &root, schema());
    let before = scene.write().expect("a scene writes its text");
    let (_, delta) = scene
        .edit(|edit| {
            edit.insert_child(
                "prop",
                dome_scene::Node::new("antenna").with(dome_schema::MeshRef::new(
                    "shell",
                    "skin",
                    dome_math::Vec3::ONE,
                )),
            )?;
            edit.set_property(
                &Target::Node(String::from("prop_body")),
                "mesh.tint",
                &dome_scene::Value::Vector(dome_math::Vec3::new(0.5, 0.5, 0.5)),
            )
        })
        .expect("an instance node joins the prefab instance");
    assert!(scene.node_index("prop_antenna").is_some());
    assert_eq!(scene.instance_changes("prop"), 2);
    let after = scene.write().expect("a scene writes its text");
    assert_ne!(after, before);

    delta.revert(&mut scene);
    assert_eq!(
        scene.write().expect("a scene writes its text"),
        before,
        "an undone edit returns every node, asset and instance the scene stood on",
    );
    assert_eq!(scene.instance_changes("prop"), 0);
    delta.apply(&mut scene);
    assert_eq!(
        scene.write().expect("a scene writes its text"),
        after,
        "a redone edit returns every change it recorded",
    );
    delta.revert(&mut scene);
    assert_eq!(scene.write().expect("a scene writes its text"), before);

    let failed = scene.edit(|edit| {
        edit.add_mesh("extra", Shape::Cube)?;
        edit.add_node("holder")?;
        edit.set_property(
            &Target::Node(String::from("ghost")),
            "mesh.tint",
            &dome_scene::Value::Vector(dome_math::Vec3::ONE),
        )
    });
    assert!(
        failed.is_err(),
        "an edit of a node the scene holds not fails",
    );
    assert_eq!(
        scene.write().expect("a scene writes its text"),
        before,
        "a failed edit leaves no mesh and no node it added",
    );
    let (_, delta) = scene
        .edit(|edit| edit.add_mesh("extra", Shape::Cube))
        .expect("a mesh joins the scene");
    assert_eq!(
        scene.meshes().len(),
        2,
        "the scene holds the mesh of its prefab and the mesh it added",
    );
    delta.revert(&mut scene);
    assert_eq!(scene.meshes().len(), 1, "an undone mesh leaves the scene");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_edit_that_breaks_a_scene_invariant_returns_the_scene_it_started_on() {
    let root = directory("edit_gate");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("prefabs")).expect("a test opens its directory");
    fs::write(root.join("prefabs/prop.toml"), PROP).expect("a test writes its prefab");
    fs::write(root.join("scene.toml"), INSTANCE).expect("a test writes its scene");
    let mut scene = Scene::load(root.join("scene.toml"), &root, schema());
    let before = scene.write().expect("a scene writes its text");
    let index = scene
        .node_index("prop")
        .expect("the scene holds its prefab instance");
    let failed = edit(&mut scene, |edit| {
        edit.set_transform(
            index,
            dome_math::Transform::from_scale_rotation_translation(
                dome_math::Vec3::new(0.0, 1.0, 1.0),
                dome_math::Quat::IDENTITY,
                dome_math::Vec3::ZERO,
            ),
        )
    });
    assert!(
        failed.is_err(),
        "an edit that collapses the scale of a node fails",
    );
    assert_eq!(
        scene.write().expect("a scene writes its text"),
        before,
        "a refused edit leaves the scene it started on",
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_unpacked_instance_keeps_the_nodes_and_assets_it_materialized() {
    let root = directory("edit_unpack");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("prefabs")).expect("a test opens its directory");
    fs::write(root.join("prefabs/prop.toml"), PROP).expect("a test writes its prefab");
    fs::write(root.join("scene.toml"), INSTANCE).expect("a test writes its scene");
    let mut scene = Scene::load(root.join("scene.toml"), &root, schema());
    let tint = dome_scene::Value::Vector(dome_math::Vec3::new(0.25, 0.5, 0.75));
    edit(&mut scene, |edit| {
        edit.set_property(&Target::Node(String::from("prop_body")), "mesh.tint", &tint)
    })
    .expect("an instance node overrides its tint");
    edit(&mut scene, |edit| edit.unpack("prop")).expect("a prefab instance unpacks");
    assert!(
        scene.instance_prefab("prop").is_none(),
        "the unpacked node holds no prefab",
    );
    assert_eq!(
        scene.instance_changes("prop"),
        0,
        "the unpacked node holds no override",
    );
    assert_eq!(
        scene.property(&Target::Node(String::from("prop_body")), "mesh.tint"),
        Some(tint),
        "the unpacked node keeps the value its override wrote",
    );
    assert!(
        scene
            .asset_owner(dome_scene::ReferenceKind::Mesh, "shell")
            .is_none(),
        "the unpacked instance leaves its assets to the scene",
    );
    let written = scene.write().expect("a scene writes its text");
    assert!(
        !written.contains("prefab = "),
        "no instance survives unpacking"
    );
    assert_eq!(
        Scene::parse(&written, &root, "the unpacked scene", schema()),
        scene,
        "the unpacked scene stands on its own",
    );
    let _ = fs::remove_dir_all(&root);
}

const OVERRIDING: &str = r#"format = "dome_scene"
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

[[nodes.overrides]]
node = "ghost"
path = "transform.translation"
value = [1.0, 2.0, 3.0]
"#;

#[test]
#[should_panic(
    expected = "the prefab instance prop overrides transform.translation of ghost, which it does not hold"
)]
fn an_override_of_a_node_the_instance_lacks_is_refused() {
    let root = directory("edit_override");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("prefabs")).expect("a test opens its directory");
    fs::write(root.join("prefabs/prop.toml"), PROP).expect("a test writes its prefab");
    fs::write(root.join("scene.toml"), OVERRIDING).expect("a test writes its scene");
    Scene::load(root.join("scene.toml"), &root, schema());
}

const REPEATING: &str = r#"format = "dome_scene"
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

[[nodes.overrides]]
node = "body"
path = "mesh.tint"
value = [1.0, 1.0, 1.0]
"#;

#[test]
fn an_instance_keeps_the_overrides_its_prefab_does_not_hold() {
    let root = directory("edit_repeat");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("prefabs")).expect("a test opens its directory");
    fs::write(root.join("prefabs/prop.toml"), PROP).expect("a test writes its prefab");
    fs::write(root.join("scene.toml"), REPEATING).expect("a test writes its scene");
    let mut scene = Scene::load(root.join("scene.toml"), &root, schema());
    assert_eq!(
        scene.instance_changes("prop"),
        0,
        "the instance drops the override that repeats its prefab",
    );
    scene
        .edit(|edit| {
            edit.set_property(
                &Target::Node(String::from("prop_body")),
                "mesh.tint",
                &dome_scene::Value::Vector(dome_math::Vec3::new(0.5, 0.5, 0.5)),
            )
        })
        .expect("an instance node overrides the tint of its prefab");
    assert_eq!(
        scene.instance_changes("prop"),
        1,
        "the instance keeps the override that departs from its prefab",
    );
    assert_eq!(
        scene.property(&Target::Node(String::from("prop_body")), "mesh.tint"),
        Some(dome_scene::Value::Vector(dome_math::Vec3::new(
            0.5, 0.5, 0.5
        ))),
        "the instance paints the tint its override writes",
    );
    let _ = fs::remove_dir_all(&root);
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

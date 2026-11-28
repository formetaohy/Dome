use dome_asset::Import;
use dome_render::{AddressMode, FilterMode};
use dome_scene::{Scene, Target};
use dome_schema::NodeComponents;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

#[test]
fn an_imported_model_expands_into_the_assets_and_nodes_it_holds() {
    let root = directory("import");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("assets")).expect("a test opens its directory");
    let texture = dome_image::encode(1, 1, &[255, 0, 0, 255]);
    fs::write(root.join("assets/texture.png"), &texture).expect("a test writes a texture");
    let binary = triangle();
    fs::write(root.join("assets/model.bin"), &binary).expect("a test writes a buffer");
    let document = json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [0] }],
        "nodes": [{ "mesh": 0 }],
        "meshes": [{
            "primitives": [{
                "attributes": { "POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2 },
                "indices": 3,
                "material": 0,
            }],
        }],
        "materials": [{
            "pbrMetallicRoughness": {
                "baseColorFactor": [0.5, 0.5, 0.5, 0.5],
                "baseColorTexture": { "index": 0 },
                "metallicFactor": 0.1,
                "roughnessFactor": 0.9,
                "metallicRoughnessTexture": { "index": 0 },
            },
            "normalTexture": { "index": 0, "scale": 1.5 },
            "occlusionTexture": { "index": 0, "strength": 0.25 },
            "emissiveFactor": [0.1, 0.2, 0.3],
            "emissiveTexture": { "index": 0 },
            "alphaMode": "BLEND",
            "alphaCutoff": 0.25,
        }],
        "textures": [{ "source": 0, "sampler": 0 }],
        "samplers": [{ "minFilter": 9728, "magFilter": 9729, "wrapS": 33071 }],
        "images": [{ "uri": "texture.png" }],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" },
            { "bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3" },
            { "bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC2" },
            { "bufferView": 3, "componentType": 5123, "count": 3, "type": "SCALAR" },
        ],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
            { "buffer": 0, "byteOffset": 36, "byteLength": 36 },
            { "buffer": 0, "byteOffset": 72, "byteLength": 24 },
            { "buffer": 0, "byteOffset": 96, "byteLength": 6 },
        ],
        "buffers": [{ "uri": "model.bin", "byteLength": binary.len() }],
    });
    fs::write(
        root.join("assets/model.gltf"),
        serde_json::to_vec(&document).expect("a test writes JSON"),
    )
    .expect("a test writes a model");
    let rig = skinned_triangle();
    fs::write(root.join("assets/rig.bin"), &rig).expect("a test writes a buffer");
    let document = json!({
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
        "buffers": [{ "uri": "rig.bin", "byteLength": rig.len() }],
    });
    fs::write(
        root.join("assets/rig.gltf"),
        serde_json::to_vec(&document).expect("a test writes JSON"),
    )
    .expect("a test writes a model");

    let mut scene = Scene::new(&root, schema());
    let target = edit(&mut scene, |edit| {
        edit.import_model("box", "assets/model.gltf", None)
    })
    .expect("a scene imports a model");
    assert_eq!(target, Target::Node(String::from("box")));
    let target = edit(&mut scene, |edit| {
        edit.import_model("rig", "assets/rig.gltf", None)
    })
    .expect("a scene imports a skinned model");
    assert_eq!(target, Target::Node(String::from("rig")));
    assert_eq!(scene.models().len(), 2);
    assert_eq!(scene.images().len(), 1);
    assert_eq!(scene.materials().len(), 2);
    assert_eq!(scene.meshes().len(), 2);
    assert_eq!(scene.skins().len(), 1);
    assert_eq!(scene.animations().len(), 1);
    assert_eq!(scene.nodes().len(), 5);
    assert_eq!(scene.nodes()[0].id, "box");
    assert_eq!(scene.nodes()[1].id, "box_node0");
    assert_eq!(scene.materials()[0].asset.metallic, 0.1);
    assert_eq!(scene.materials()[0].asset.roughness, 0.9);
    assert_eq!(
        scene.materials()[0]
            .asset
            .base_color_image
            .as_ref()
            .map(|image| image.as_str()),
        Some("box_image_0"),
    );
    assert_eq!(
        scene.materials()[0]
            .asset
            .metallic_roughness_image
            .as_ref()
            .map(|image| image.as_str()),
        Some("box_image_0"),
    );
    assert_eq!(scene.materials()[0].asset.normal_scale, 1.5);
    assert_eq!(scene.materials()[0].asset.occlusion_strength, 0.25);
    assert_eq!(scene.images()[0].sampler.min_filter, FilterMode::Nearest,);
    assert_eq!(scene.images()[0].sampler.mag_filter, FilterMode::Linear);
    assert_eq!(scene.images()[0].sampler.mipmap_filter, FilterMode::Nearest);
    assert_eq!(
        scene.images()[0].sampler.address_mode_u,
        AddressMode::ClampToEdge,
    );
    assert_eq!(
        scene.images()[0].sampler.address_mode_v,
        AddressMode::Repeat
    );
    assert_eq!(
        scene.materials()[0].asset.base_color,
        dome_math::Vec3::splat(0.5),
    );
    assert_eq!(scene.materials()[0].asset.alpha, 0.5);
    assert_eq!(
        scene.materials()[0].asset.alpha_mode,
        dome_render::AlphaMode::Blend,
    );
    assert_eq!(scene.materials()[0].asset.alpha_cutoff, 0.25);
    assert_eq!(
        scene.nodes()[1]
            .component::<dome_schema::MeshRef>()
            .map(|mesh| { (mesh.mesh.as_str(), mesh.material.as_str(), mesh.tint) }),
        Some(("box_mesh_0_0", "box_material_0", dome_math::Vec3::ONE)),
    );
    assert_eq!(scene.skins()[0].id, "rig_skin_0");
    assert_eq!(
        scene.skins()[0]
            .asset
            .joints()
            .iter()
            .map(|joint| joint.as_str())
            .collect::<Vec<_>>(),
        vec!["rig_joint"],
    );
    assert_eq!(scene.animations()[0].id, "rig_animation_0");
    assert_eq!(
        scene.animations()[0]
            .asset
            .targets()
            .iter()
            .map(|target| target.as_str())
            .collect::<Vec<_>>(),
        vec!["rig_joint"],
    );
    let mesh_node = &scene.nodes()[scene
        .node_index("rig_node0")
        .expect("the imported mesh node")];
    assert!(mesh_node.component::<dome_schema::MeshRef>().is_some());
    assert!(mesh_node.component::<dome_schema::SkinRef>().is_some());
    let holder = &scene.nodes()[scene.node_index("rig").expect("the imported model root")];
    assert_eq!(
        holder
            .component::<dome_schema::AnimationRef>()
            .map(|animation| animation.graph.as_str()),
        Some("rig_graph"),
    );
    let written = scene.write().expect("a scene writes its text");
    let mut parsed = Scene::parse(&written, &root, "the written scene", schema())
        .expect("the written scene parses");
    assert_eq!(parsed, scene);
    assert!(
        edit(&mut parsed, |edit| edit.import_model(
            "box",
            "assets/model.gltf",
            None
        ))
        .is_err()
    );
    assert!(
        edit(&mut parsed, |edit| edit
            .remove(&Target::Model(String::from("box"))))
        .is_err()
    );
    assert!(
        edit(&mut parsed, |edit| edit
            .remove(&Target::Node(String::from("rig_joint"))))
        .is_err()
    );
    assert!(
        edit(&mut parsed, |edit| edit
            .remove(&Target::Model(String::from("rig"))))
        .is_err()
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_imported_model_reaches_the_deepest_node_of_a_chain() {
    const DEPTH: usize = 50_000;
    let root = directory("deep");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("assets")).expect("a test opens its directory");
    let nodes = (0..DEPTH)
        .map(|index| {
            if index + 1 < DEPTH {
                json!({ "children": [index + 1] })
            } else {
                json!({})
            }
        })
        .collect::<Vec<_>>();
    let document = json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [0] }],
        "nodes": nodes,
    });
    fs::write(
        root.join("assets/deep.gltf"),
        serde_json::to_vec(&document).expect("a test writes JSON"),
    )
    .expect("a test writes a model");

    let mut scene = Scene::new(&root, schema());
    let target = edit(&mut scene, |edit| {
        edit.import_model("deep", "assets/deep.gltf", None)
    })
    .expect("a scene imports a deep model");
    assert_eq!(target, Target::Node(String::from("deep")));
    assert_eq!(scene.nodes().len(), DEPTH + 1);
    let holder = scene.node_index("deep").expect("the imported model root");
    assert_eq!(scene.subtree(holder).len(), DEPTH + 1);
    let deepest = scene
        .node_index(&format!("deep_node{}", DEPTH - 1))
        .expect("the deepest imported node");
    assert_eq!(
        scene
            .parent_index(deepest)
            .map(|parent| scene.nodes()[parent].id.clone()),
        Some(format!("deep_node{}", DEPTH - 2)),
    );
    let _ = fs::remove_dir_all(&root);
}

fn triangle() -> Vec<u8> {
    let mut binary = Vec::new();
    for value in [-1.0f32, -1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0, 0.0] {
        binary.extend(value.to_le_bytes());
    }
    for value in [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0] {
        binary.extend(value.to_le_bytes());
    }
    for value in [0.0f32, 0.0, 1.0, 0.0, 0.5, 1.0] {
        binary.extend(value.to_le_bytes());
    }
    for index in [0u16, 1, 2] {
        binary.extend(index.to_le_bytes());
    }
    binary
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
    for value in [0.0f32, 0.0, 0.0, 0.0, 0.0, 1.0] {
        binary.extend(value.to_le_bytes());
    }
    binary
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

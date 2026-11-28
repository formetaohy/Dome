use dome_gltf::Model;
use dome_gltf::{AlphaMode, Filter, Wrap};
use dome_math::Vec3;
use serde_json::json;
use std::path::Path;

const WIDTH: u32 = 2;
const HEIGHT: u32 = 2;
const PIXELS: [u8; WIDTH as usize * HEIGHT as usize * 4] = [
    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
];

#[test]
fn a_model_reads_a_binary_container_of_triangles_and_textures() {
    let image = dome_image::encode(WIDTH, HEIGHT, &PIXELS);
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
    let index_offset = binary.len();
    for index in [0u16, 1, 2] {
        binary.extend(index.to_le_bytes());
    }
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let image_offset = binary.len();
    binary.extend(&image);

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
                "baseColorFactor": [0.8, 0.5, 0.2, 0.4],
                "baseColorTexture": { "index": 0 },
                "metallicFactor": 0.3,
                "roughnessFactor": 0.4,
                "metallicRoughnessTexture": { "index": 0 },
            },
            "normalTexture": { "index": 0, "scale": 2.0 },
            "occlusionTexture": { "index": 0, "strength": 0.5 },
            "emissiveFactor": [0.2, 0.1, 0.0],
            "emissiveTexture": { "index": 0 },
            "alphaMode": "MASK",
            "alphaCutoff": 0.3,
        }],
        "textures": [{ "source": 0, "sampler": 0 }],
        "samplers": [{
            "minFilter": 9987,
            "magFilter": 9729,
            "wrapS": 33071,
            "wrapT": 33648,
        }],
        "images": [{ "bufferView": 4, "mimeType": "image/png" }],
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
            { "buffer": 0, "byteOffset": index_offset, "byteLength": 6 },
            { "buffer": 0, "byteOffset": image_offset, "byteLength": image.len() },
        ],
        "buffers": [{ "byteLength": binary.len() }],
    });
    let glb = container(
        &serde_json::to_vec(&document).expect("a test writes JSON"),
        &binary,
    );
    let model = Model::parse(&glb, Path::new("model.glb")).expect("the model parses");

    assert_eq!(model.roots, vec![0]);
    assert_eq!(model.nodes.len(), 1);
    assert_eq!(model.nodes[0].mesh, Some(0));
    assert_eq!(model.meshes.len(), 1);
    let primitive = &model.meshes[0].primitives[0];
    assert_eq!(primitive.vertices.len(), 3);
    assert_eq!(primitive.vertices[0].position, [-1.0, -1.0, 0.0]);
    assert_eq!(primitive.vertices[1].uv, [1.0, 0.0]);
    assert_eq!(model.materials[0].base_color_image, Some(0));
    assert_eq!(model.materials[0].alpha, 0.4);
    assert_eq!(model.materials[0].alpha_mode, AlphaMode::Mask);
    assert_eq!(model.materials[0].alpha_cutoff, 0.3);
    assert_eq!(model.materials[0].metallic, 0.3);
    assert_eq!(model.materials[0].roughness, 0.4);
    assert_eq!(model.materials[0].metallic_roughness_image, Some(0));
    assert_eq!(model.materials[0].normal_image, Some(0));
    assert_eq!(model.materials[0].normal_scale, 2.0);
    assert_eq!(model.materials[0].occlusion_image, Some(0));
    assert_eq!(model.materials[0].occlusion_strength, 0.5);
    assert_eq!(model.materials[0].emissive, Vec3::new(0.2, 0.1, 0.0));
    assert_eq!(model.materials[0].emissive_image, Some(0));
    assert_eq!(model.images[0].sampler().min_filter, Filter::Linear);
    assert_eq!(model.images[0].sampler().mag_filter, Filter::Linear);
    assert_eq!(model.images[0].sampler().mip_filter, Filter::Linear);
    assert_eq!(model.images[0].sampler().wrap_u, Wrap::ClampToEdge);
    assert_eq!(model.images[0].sampler().wrap_v, Wrap::MirroredRepeat);
    assert_eq!(model.images[0].width(), WIDTH);
    assert_eq!(model.images[0].height(), HEIGHT);
    assert_eq!(model.images[0].rgba(), PIXELS);
}

fn container(json: &[u8], binary: &[u8]) -> Vec<u8> {
    let mut json = json.to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut binary = binary.to_vec();
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let length = 12 + 8 + json.len() + 8 + binary.len();
    let mut glb = Vec::with_capacity(length);
    glb.extend(0x4654_6C67u32.to_le_bytes());
    glb.extend(2u32.to_le_bytes());
    glb.extend((length as u32).to_le_bytes());
    glb.extend((json.len() as u32).to_le_bytes());
    glb.extend(0x4E4F_534Au32.to_le_bytes());
    glb.extend(&json);
    glb.extend((binary.len() as u32).to_le_bytes());
    glb.extend(0x004E_4942u32.to_le_bytes());
    glb.extend(&binary);
    glb
}

#[test]
fn a_model_refuses_a_buffer_view_past_the_bytes_its_buffer_names() {
    let mut binary = Vec::new();
    for value in [-1.0f32, -1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0, 0.0] {
        binary.extend(value.to_le_bytes());
    }
    let document = json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [0] }],
        "nodes": [{ "mesh": 0 }],
        "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 } }] }],
        "accessors": [{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" }],
        "bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": 36 }],
        "buffers": [{ "byteLength": 32 }],
    });
    let glb = container(
        &serde_json::to_vec(&document).expect("a test writes JSON"),
        &binary,
    );
    let error = Model::parse(&glb, Path::new("model.glb"))
        .err()
        .expect("a buffer view past the buffer it names holds no model");
    assert!(
        error.contains("beyond its buffer"),
        "the model names the buffer view it cannot read: {error}",
    );
}

#[test]
fn a_model_reads_skins_and_animations() {
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
    let identity = [
        1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for value in identity {
        binary.extend(value.to_le_bytes());
    }
    for value in [0.0f32, 1.0] {
        binary.extend(value.to_le_bytes());
    }
    for value in [0.0f32, 0.0, 0.0, 0.0, 0.0, 1.0] {
        binary.extend(value.to_le_bytes());
    }

    let document = json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [0] }],
        "nodes": [
            { "mesh": 0, "skin": 0, "children": [1] },
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
        "buffers": [{ "byteLength": binary.len() }],
    });
    let glb = container(
        &serde_json::to_vec(&document).expect("a test writes JSON"),
        &binary,
    );
    let model = Model::parse(&glb, Path::new("skinned.glb")).expect("the skinned model parses");

    assert_eq!(model.nodes[0].skin, Some(0));
    assert_eq!(model.skins.len(), 1);
    assert_eq!(model.skins[0].joints, vec![1]);
    assert_eq!(model.skins[0].inverse_binds.len(), 1);
    assert_eq!(model.skins[0].inverse_binds[0], dome_math::Mat4::IDENTITY);
    let primitive = &model.meshes[0].primitives[0];
    let skin = primitive.skin.as_ref().expect("a skinned primitive");
    assert_eq!(skin.len(), 3);
    assert_eq!(skin[0].joints[0], 0);
    assert_eq!(skin[0].weights[0], 1.0);
    assert_eq!(model.animations.len(), 1);
    assert_eq!(model.animations[0].name.as_deref(), Some("bend"));
    assert_eq!(model.animations[0].channels.len(), 1);
    let channel = &model.animations[0].channels[0];
    assert_eq!(channel.node, 1);
    assert_eq!(channel.property, dome_animation::Property::Translation);
    assert_eq!(channel.interpolation, dome_animation::Interpolation::Linear);
    assert_eq!(channel.times, vec![0.0, 1.0]);
    assert_eq!(channel.values, vec![0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
}

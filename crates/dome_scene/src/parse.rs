use crate::animation::{AnimationAsset, AnimationEntry};
use crate::environment::EnvironmentEntry;
use crate::graph::{GraphEntry, parse as graph};
use crate::image::{ImageEntry, address_of, filter_of};
use crate::material::{MaterialAsset, MaterialEntry};
use crate::mesh::{MeshAsset, MeshEntry};
use crate::model::ModelEntry;
use crate::node::{InstanceOverride, Node};
use crate::nodes::Nodes;
use crate::reference::{ImageId, ModelId, NodeId};
use crate::registry::Registry;
use crate::scene::{FORMAT, Scene};
use crate::shape::Shape;
use crate::skin::{SkinAsset, SkinEntry};
use crate::sound::SoundEntry;
use crate::value::Value;
use dome_math::Vec3;
use dome_render::AlphaMode;
use dome_render::SamplerDesc;
use dome_render::Vertex;
use dome_render::environment::Atmosphere;
use dome_toml::{Fields, Toml, floats};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn document(text: &str, root: PathBuf, what: &str, registry: Arc<Registry>) -> Scene {
    let mut scene = Fields::of(Toml::parse(text, what), what);
    let format = scene
        .need("format")
        .string_value(&format!("the format of {what}"));
    assert!(
        format == FORMAT || format == crate::prefab::FORMAT,
        "{what} holds the format {format}",
    );
    let prefab = format == crate::prefab::FORMAT;
    let models = match scene.take("models") {
        Some(value) => read_models(value, what),
        None => Vec::new(),
    };
    let images = images(scene.need("images"), what);
    let sounds = match scene.take("sounds") {
        Some(value) => sounds(value, what),
        None => Vec::new(),
    };
    let environments = match scene.take("environments") {
        Some(value) => environments(value, what),
        None => Vec::new(),
    };
    let meshes = meshes(scene.need("meshes"), what);
    let materials = materials(scene.need("materials"), what);
    let skins = match scene.take("skins") {
        Some(value) => skins(value, what),
        None => Vec::new(),
    };
    let animations = match scene.take("animations") {
        Some(value) => animations(value, what),
        None => Vec::new(),
    };
    let graphs = match scene.take("graphs") {
        Some(value) => value
            .array_items(&format!("the graphs of {what}"))
            .into_iter()
            .enumerate()
            .map(|(index, item)| graph(item, &format!("graph {index} of {what}")))
            .collect::<Vec<GraphEntry>>(),
        None => Vec::new(),
    };
    let nodes = nodes(scene.need("nodes"), what, &registry);
    scene.done();
    Scene {
        root,
        registry,
        prefab,
        owners: Arc::new(Vec::new()),
        instances: Arc::new(BTreeMap::new()),
        models: Arc::new(models),
        images: Arc::new(images),
        sounds: Arc::new(sounds),
        environments: Arc::new(environments),
        meshes: Arc::new(meshes),
        materials: Arc::new(materials),
        skins: Arc::new(skins),
        animations: Arc::new(animations),
        graphs: Arc::new(graphs),
        nodes: Nodes::from(nodes),
    }
}

fn read_models(value: Toml, what: &str) -> Vec<ModelEntry> {
    value
        .array_items(&format!("the models of {what}"))
        .into_iter()
        .enumerate()
        .map(|(index, item)| model(item, &format!("model {index} of {what}")))
        .collect()
}

fn model(value: Toml, what: &str) -> ModelEntry {
    let mut fields = Fields::of(value, what);
    let id = fields.need("id").string_value(&format!("the id of {what}"));
    let path = fields
        .need("path")
        .string_value(&format!("the path of {what}"));
    fields.done();
    ModelEntry::new(id, PathBuf::from(path))
}

fn sounds(value: Toml, what: &str) -> Vec<SoundEntry> {
    value
        .array_items(&format!("the sounds of {what}"))
        .into_iter()
        .enumerate()
        .map(|(index, item)| sound(item, &format!("sound {index} of {what}")))
        .collect()
}

fn sound(value: Toml, what: &str) -> SoundEntry {
    let mut fields = Fields::of(value, what);
    let id = fields.need("id").string_value(&format!("the id of {what}"));
    let path = fields
        .need("path")
        .string_value(&format!("the path of {what}"));
    let stream = fields
        .take("stream")
        .is_some_and(|stream| stream.boolean_value(&format!("the stream of {what}")));
    fields.done();
    let mut entry = SoundEntry::new(id, PathBuf::from(path));
    entry.stream = stream;
    entry
}

fn environments(value: Toml, what: &str) -> Vec<EnvironmentEntry> {
    value
        .array_items(&format!("the environments of {what}"))
        .into_iter()
        .enumerate()
        .map(|(index, item)| environment(item, &format!("environment {index} of {what}")))
        .collect()
}

fn environment(value: Toml, what: &str) -> EnvironmentEntry {
    let mut fields = Fields::of(value, what);
    let id = fields.need("id").string_value(&format!("the id of {what}"));
    let path = fields.take("path");
    let atmosphere = fields.take("atmosphere");
    let capture = fields
        .take("capture")
        .is_some_and(|value| value.boolean_value(&format!("the capture of {what}")));
    fields.done();
    match (path, atmosphere, capture) {
        (Some(path), None, false) => {
            let path = path.string_value(&format!("the path of {what}"));
            EnvironmentEntry::new(id, PathBuf::from(path))
        }
        (None, Some(value), false) => {
            let what = format!("the atmosphere of {what}");
            EnvironmentEntry::with_atmosphere(id, atmosphere_asset(value, &what))
        }
        (None, None, true) => EnvironmentEntry::captured(id),
        (Some(_), Some(_), _) => panic!("{what} holds a panorama and an atmosphere"),
        (Some(_), None, true) => panic!("{what} holds a panorama and a capture"),
        (None, Some(_), true) => panic!("{what} holds an atmosphere and a capture"),
        (None, None, false) => panic!("{what} holds no panorama, atmosphere or capture"),
    }
}

fn atmosphere_asset(value: Toml, what: &str) -> Atmosphere {
    let mut fields = Fields::of(value, what);
    let sun_elevation = fields
        .need("sun_elevation")
        .f32_value(&format!("the sun elevation of {what}"));
    let sun_azimuth = fields
        .need("sun_azimuth")
        .f32_value(&format!("the sun azimuth of {what}"));
    let sun_intensity = fields
        .need("sun_intensity")
        .f32_value(&format!("the sun intensity of {what}"));
    let rayleigh = fields
        .need("rayleigh")
        .f32_value(&format!("the Rayleigh scale of {what}"));
    let mie = fields
        .need("mie")
        .f32_value(&format!("the Mie scale of {what}"));
    let mie_anisotropy = fields
        .need("mie_anisotropy")
        .f32_value(&format!("the Mie anisotropy of {what}"));
    let ozone = fields
        .need("ozone")
        .f32_value(&format!("the ozone scale of {what}"));
    let ground_albedo = Vec3::from_array(floats::<3>(
        fields.need("ground_albedo"),
        &format!("the ground albedo of {what}"),
    ));
    let altitude = fields
        .need("altitude")
        .f32_value(&format!("the altitude of {what}"));
    fields.done();
    Atmosphere {
        sun_elevation,
        sun_azimuth,
        sun_intensity,
        rayleigh,
        mie,
        mie_anisotropy,
        ozone,
        ground_albedo,
        altitude,
    }
}

fn images(value: Toml, what: &str) -> Vec<ImageEntry> {
    value
        .array_items(&format!("the images of {what}"))
        .into_iter()
        .enumerate()
        .map(|(index, item)| image(item, &format!("image {index} of {what}")))
        .collect()
}

fn image(value: Toml, what: &str) -> ImageEntry {
    let mut fields = Fields::of(value, what);
    let id = fields.need("id").string_value(&format!("the id of {what}"));
    let path = fields.take("path");
    let model = fields.take("model");
    let index = fields.take("image");
    let min_filter = filter(
        fields.need("min_filter"),
        &format!("the min filter of {what}"),
    );
    let mag_filter = filter(
        fields.need("mag_filter"),
        &format!("the mag filter of {what}"),
    );
    let mip_filter = filter(
        fields.need("mip_filter"),
        &format!("the mip filter of {what}"),
    );
    let wrap_u = address(fields.need("wrap_u"), &format!("the wrap u of {what}"));
    let wrap_v = address(fields.need("wrap_v"), &format!("the wrap v of {what}"));
    fields.done();
    let sampler = SamplerDesc {
        min_filter,
        mag_filter,
        mip_filter,
        address_u: wrap_u,
        address_v: wrap_v,
    };
    let entry = match (path, model, index) {
        (Some(path), None, None) => ImageEntry::new(
            id,
            PathBuf::from(path.string_value(&format!("the path of {what}"))),
        ),
        (None, Some(model), Some(index)) => ImageEntry::embedded(
            id,
            ModelId::new(model.string_value(&format!("the model of {what}"))),
            index_count(index, &format!("the image of {what}")),
        ),
        _ => panic!("{what} holds no single source"),
    };
    entry.with_sampler(sampler)
}

fn meshes(value: Toml, what: &str) -> Vec<MeshEntry> {
    value
        .array_items(&format!("the meshes of {what}"))
        .into_iter()
        .enumerate()
        .map(|(index, item)| mesh(item, &format!("mesh {index} of {what}")))
        .collect()
}

fn mesh(value: Toml, what: &str) -> MeshEntry {
    let mut fields = Fields::of(value, what);
    let id = fields.need("id").string_value(&format!("the id of {what}"));
    let shape = fields.take("shape");
    let vertices = fields.take("vertices");
    let model = fields.take("model");
    let mesh = fields.take("mesh");
    let primitive = fields.take("primitive");
    fields.done();
    let asset = match (shape, vertices, model, mesh, primitive) {
        (Some(shape), None, None, None, None) => MeshAsset::shape(Shape::of(
            &shape.string_value(&format!("the shape of {what}")),
            what,
        )),
        (None, Some(vertices), None, None, None) => {
            let items = vertices.array_items(&format!("the vertices of {what}"));
            assert!(
                !items.is_empty() && items.len() % 3 == 0,
                "{what} holds {} vertices where triangle vertices stand in threes",
                items.len(),
            );
            MeshAsset::vertices(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| vertex(item, &format!("vertex {index} of {what}")))
                    .collect(),
            )
        }
        (None, None, Some(model), Some(mesh), Some(primitive)) => MeshAsset::model(
            ModelId::new(model.string_value(&format!("the model of {what}"))),
            index_count(mesh, &format!("the mesh of {what}")),
            index_count(primitive, &format!("the primitive of {what}")),
        ),
        _ => panic!("{what} holds no single geometry"),
    };
    MeshEntry::new(id, asset)
}

fn index_count(value: Toml, what: &str) -> u32 {
    let count = value.u64_value(what);
    u32::try_from(count).unwrap_or_else(|_| panic!("{what} holds {count}"))
}

fn vertex(value: Toml, what: &str) -> Vertex {
    let mut fields = Fields::of(value, what);
    let position = floats(fields.need("position"), &format!("the position of {what}"));
    let normal = floats(fields.need("normal"), &format!("the normal of {what}"));
    let uv = floats(fields.need("uv"), &format!("the uv of {what}"));
    fields.done();
    Vertex {
        position,
        normal,
        uv,
    }
}

fn skins(value: Toml, what: &str) -> Vec<SkinEntry> {
    value
        .array_items(&format!("the skins of {what}"))
        .into_iter()
        .enumerate()
        .map(|(index, item)| skin(item, &format!("skin {index} of {what}")))
        .collect()
}

fn skin(value: Toml, what: &str) -> SkinEntry {
    let mut fields = Fields::of(value, what);
    let id = fields.need("id").string_value(&format!("the id of {what}"));
    let model = ModelId::new(
        fields
            .need("model")
            .string_value(&format!("the model of {what}")),
    );
    let index = index_count(fields.need("skin"), &format!("the skin of {what}"));
    let joints = fields
        .need("joints")
        .array_items(&format!("the joints of {what}"))
        .into_iter()
        .enumerate()
        .map(|(at, joint)| NodeId::new(joint.string_value(&format!("joint {at} of {what}"))))
        .collect::<Vec<_>>();
    fields.done();
    SkinEntry::new(id, SkinAsset::model(model, index, joints))
}

fn animations(value: Toml, what: &str) -> Vec<AnimationEntry> {
    value
        .array_items(&format!("the animations of {what}"))
        .into_iter()
        .enumerate()
        .map(|(index, item)| animation(item, &format!("animation {index} of {what}")))
        .collect()
}

fn animation(value: Toml, what: &str) -> AnimationEntry {
    let mut fields = Fields::of(value, what);
    let id = fields.need("id").string_value(&format!("the id of {what}"));
    let model = ModelId::new(
        fields
            .need("model")
            .string_value(&format!("the model of {what}")),
    );
    let index = index_count(
        fields.need("animation"),
        &format!("the animation of {what}"),
    );
    let targets = fields
        .need("targets")
        .array_items(&format!("the targets of {what}"))
        .into_iter()
        .enumerate()
        .map(|(at, target)| NodeId::new(target.string_value(&format!("target {at} of {what}"))))
        .collect::<Vec<_>>();
    fields.done();
    AnimationEntry::new(id, AnimationAsset::model(model, index, targets))
}

fn materials(value: Toml, what: &str) -> Vec<MaterialEntry> {
    value
        .array_items(&format!("the materials of {what}"))
        .into_iter()
        .enumerate()
        .map(|(index, item)| material(item, &format!("material {index} of {what}")))
        .collect()
}

fn material(value: Toml, what: &str) -> MaterialEntry {
    let mut fields = Fields::of(value, what);
    let id = fields.need("id").string_value(&format!("the id of {what}"));
    let base_color = color(
        fields.need("base_color"),
        &format!("the base color of {what}"),
    );
    let alpha = fields
        .need("alpha")
        .f32_value(&format!("the alpha of {what}"));
    assert!(
        (0.0..=1.0).contains(&alpha),
        "the alpha of {what} lies outside the unit range",
    );
    let alpha_mode = alpha_mode(
        fields.need("alpha_mode"),
        &format!("the alpha mode of {what}"),
    );
    let alpha_cutoff = fields
        .need("alpha_cutoff")
        .f32_value(&format!("the alpha cutoff of {what}"));
    assert!(
        (0.0..=1.0).contains(&alpha_cutoff),
        "the alpha cutoff of {what} lies outside the unit range",
    );
    let base_color_image = image_field(fields.take("base_color_image"), what);
    let metallic = fields
        .need("metallic")
        .f32_value(&format!("the metallic of {what}"));
    let roughness = fields
        .need("roughness")
        .f32_value(&format!("the roughness of {what}"));
    let metallic_roughness_image = image_field(fields.take("metallic_roughness_image"), what);
    let normal_image = image_field(fields.take("normal_image"), what);
    let normal_scale = fields
        .need("normal_scale")
        .f32_value(&format!("the normal scale of {what}"));
    let occlusion_image = image_field(fields.take("occlusion_image"), what);
    let occlusion_strength = fields
        .need("occlusion_strength")
        .f32_value(&format!("the occlusion strength of {what}"));
    let emissive = color(fields.need("emissive"), &format!("the emissive of {what}"));
    let emissive_image = image_field(fields.take("emissive_image"), what);
    fields.done();
    MaterialEntry::new(
        id,
        MaterialAsset {
            base_color,
            alpha,
            alpha_mode,
            alpha_cutoff,
            base_color_image,
            metallic,
            roughness,
            metallic_roughness_image,
            normal_image,
            normal_scale,
            occlusion_image,
            occlusion_strength,
            emissive,
            emissive_image,
        },
    )
}

fn image_field(value: Option<Toml>, what: &str) -> Option<ImageId> {
    value.map(|value| ImageId::new(value.string_value(&format!("the image of {what}"))))
}

fn alpha_mode(value: Toml, what: &str) -> AlphaMode {
    let name = value.string_value(what);
    AlphaMode::of(&name).unwrap_or_else(|| panic!("{what} holds the mode {name}"))
}

fn color(value: Toml, what: &str) -> Vec3 {
    let color = vec3(value, what);
    assert!(
        (0.0..=1.0).contains(&color.x)
            && (0.0..=1.0).contains(&color.y)
            && (0.0..=1.0).contains(&color.z),
        "{what} holds the color {color:?} outside the unit cube",
    );
    color
}

fn filter(value: Toml, what: &str) -> dome_render::Filter {
    let name = value.string_value(what);
    filter_of(&name).unwrap_or_else(|| panic!("{what} holds the filter {name}"))
}

fn address(value: Toml, what: &str) -> dome_render::AddressMode {
    let name = value.string_value(what);
    address_of(&name).unwrap_or_else(|| panic!("{what} holds the wrap {name}"))
}

fn nodes(value: Toml, what: &str, registry: &Registry) -> Vec<Node> {
    value
        .array_items(&format!("the nodes of {what}"))
        .into_iter()
        .enumerate()
        .map(|(index, item)| node(item, &format!("node {index} of {what}"), registry))
        .collect()
}

fn node(value: Toml, what: &str, registry: &Registry) -> Node {
    let mut id = None;
    let mut parent = None;
    let mut prefab = None;
    let mut overrides = Vec::new();
    let mut additions = Vec::new();
    let mut removals = Vec::new();
    let mut components = BTreeMap::new();
    for (key, value) in value.object_fields(what) {
        match key.as_str() {
            "id" => id = Some(value.string_value(&format!("the id of {what}"))),
            "parent" => {
                parent = Some(value.string_value(&format!("the parent of {what}")));
            }
            "prefab" => {
                prefab = Some(PathBuf::from(
                    value.string_value(&format!("the prefab of {what}")),
                ));
            }
            "overrides" => {
                overrides = instance_overrides(value, &format!("the overrides of {what}"));
            }
            "additions" => {
                additions = nodes(value, &format!("the additions of {what}"), registry);
            }
            "removals" => {
                removals = value
                    .array_items(&format!("the removals of {what}"))
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| item.string_value(&format!("removal {index} of {what}")))
                    .collect();
            }
            other => {
                let component = registry.parse(other, value, &format!("the {other} of {what}"));
                components.insert(component.name(), component);
            }
        }
    }
    let id = id.unwrap_or_else(|| panic!("{what} holds no id"));
    assert!(!id.is_empty(), "{what} holds no id");
    Node {
        id,
        parent,
        prefab,
        owner: None,
        overrides,
        additions,
        removals,
        components,
    }
}

fn instance_overrides(value: Toml, what: &str) -> Vec<InstanceOverride> {
    value
        .array_items(what)
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let what = format!("override {index} of {what}");
            let mut node = None;
            let mut path = None;
            let mut value = None;
            for (key, held) in item.object_fields(&what) {
                match key.as_str() {
                    "node" => node = Some(held.string_value(&format!("the node of {what}"))),
                    "path" => path = Some(held.string_value(&format!("the path of {what}"))),
                    "value" => value = Some(Value::parse(held, &format!("the value of {what}"))),
                    other => panic!("{what} holds the unknown field {other}"),
                }
            }
            InstanceOverride {
                node: node.unwrap_or_else(|| panic!("{what} names no node")),
                path: path.unwrap_or_else(|| panic!("{what} names no path")),
                value: value.unwrap_or_else(|| panic!("{what} holds no value")),
            }
        })
        .collect()
}

fn vec3(value: Toml, what: &str) -> Vec3 {
    Vec3::from_array(floats(value, what))
}

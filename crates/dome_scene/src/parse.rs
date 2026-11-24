use crate::animation::{AnimationAsset, AnimationEntry};
use crate::component::{ReferenceKind, Schema};
use crate::environment::EnvironmentEntry;
use crate::graph::{GraphEntry, parse as graph};
use crate::image::{ImageEntry, address_of, filter_of};
use crate::material::{MaterialAsset, MaterialEntry};
use crate::mesh::{MeshAsset, MeshEntry};
use crate::model::ModelEntry;
use crate::node::{InstanceOverride, Node};
use crate::nodes::Nodes;
use crate::reference::{ImageId, ModelId, NodeId};
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

fn checked(id: String, kind: ReferenceKind) -> Result<String, String> {
    if crate::valid_id(&id) {
        Ok(id)
    } else {
        Err(format!(
            "the {} id {id} holds no letter, digit, _ or -",
            kind.name(),
        ))
    }
}

pub(crate) fn document(
    text: &str,
    root: PathBuf,
    what: &str,
    schema: Arc<dyn Schema>,
) -> Result<Scene, String> {
    let mut scene = Fields::of(Toml::parse(text, what)?, what)?;
    let format = scene
        .need("format")?
        .string_value(&format!("the format of {what}"))?;
    if format != FORMAT && format != crate::prefab::FORMAT {
        return Err(format!("{what} holds the format {format}"));
    }
    let prefab = format == crate::prefab::FORMAT;
    let models = match scene.take("models") {
        Some(value) => read_models(value, what)?,
        None => Vec::new(),
    };
    let images = images(scene.need("images")?, what)?;
    let sounds = match scene.take("sounds") {
        Some(value) => sounds(value, what)?,
        None => Vec::new(),
    };
    let environments = match scene.take("environments") {
        Some(value) => environments(value, what)?,
        None => Vec::new(),
    };
    let meshes = meshes(scene.need("meshes")?, what)?;
    let materials = materials(scene.need("materials")?, what)?;
    let skins = match scene.take("skins") {
        Some(value) => skins(value, what)?,
        None => Vec::new(),
    };
    let animations = match scene.take("animations") {
        Some(value) => animations(value, what)?,
        None => Vec::new(),
    };
    let graphs = match scene.take("graphs") {
        Some(value) => value
            .array_items(&format!("the graphs of {what}"))?
            .into_iter()
            .enumerate()
            .map(|(index, item)| graph(item, &format!("graph {index} of {what}")))
            .collect::<Result<Vec<GraphEntry>, String>>()?,
        None => Vec::new(),
    };
    let nodes = nodes(scene.need("nodes")?, what, schema.as_ref())?;
    scene.done()?;
    Ok(Scene {
        root,
        schema,
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
        revision: 0,
    })
}

fn read_models(value: Toml, what: &str) -> Result<Vec<ModelEntry>, String> {
    value
        .array_items(&format!("the models of {what}"))?
        .into_iter()
        .enumerate()
        .map(|(index, item)| model(item, &format!("model {index} of {what}")))
        .collect()
}

fn model(value: Toml, what: &str) -> Result<ModelEntry, String> {
    let mut fields = Fields::of(value, what)?;
    let id = fields
        .need("id")?
        .string_value(&format!("the id of {what}"))?;
    let path = fields
        .need("path")?
        .string_value(&format!("the path of {what}"))?;
    fields.done()?;
    Ok(ModelEntry::new(id, crate::folder::relative_path(&path)))
}

fn sounds(value: Toml, what: &str) -> Result<Vec<SoundEntry>, String> {
    value
        .array_items(&format!("the sounds of {what}"))?
        .into_iter()
        .enumerate()
        .map(|(index, item)| sound(item, &format!("sound {index} of {what}")))
        .collect()
}

fn sound(value: Toml, what: &str) -> Result<SoundEntry, String> {
    let mut fields = Fields::of(value, what)?;
    let id = fields
        .need("id")?
        .string_value(&format!("the id of {what}"))?;
    let path = fields
        .need("path")?
        .string_value(&format!("the path of {what}"))?;
    let stream = match fields.take("stream") {
        Some(stream) => stream.boolean_value(&format!("the stream of {what}"))?,
        None => false,
    };
    fields.done()?;
    let mut entry = SoundEntry::new(id, crate::folder::relative_path(&path));
    entry.stream = stream;
    Ok(entry)
}

fn environments(value: Toml, what: &str) -> Result<Vec<EnvironmentEntry>, String> {
    value
        .array_items(&format!("the environments of {what}"))?
        .into_iter()
        .enumerate()
        .map(|(index, item)| environment(item, &format!("environment {index} of {what}")))
        .collect()
}

fn environment(value: Toml, what: &str) -> Result<EnvironmentEntry, String> {
    let mut fields = Fields::of(value, what)?;
    let id = fields
        .need("id")?
        .string_value(&format!("the id of {what}"))?;
    let path = fields.take("path");
    let atmosphere = fields.take("atmosphere");
    let capture = match fields.take("capture") {
        Some(value) => value.boolean_value(&format!("the capture of {what}"))?,
        None => false,
    };
    fields.done()?;
    match (path, atmosphere, capture) {
        (Some(path), None, false) => {
            let path = path.string_value(&format!("the path of {what}"))?;
            Ok(EnvironmentEntry::new(
                id,
                crate::folder::relative_path(&path),
            ))
        }
        (None, Some(value), false) => {
            let what = format!("the atmosphere of {what}");
            Ok(EnvironmentEntry::with_atmosphere(
                id,
                atmosphere_asset(value, &what)?,
            ))
        }
        (None, None, true) => Ok(EnvironmentEntry::captured(id)),
        (Some(_), Some(_), _) => Err(format!("{what} holds a panorama and an atmosphere")),
        (Some(_), None, true) => Err(format!("{what} holds a panorama and a capture")),
        (None, Some(_), true) => Err(format!("{what} holds an atmosphere and a capture")),
        (None, None, false) => Err(format!("{what} holds no panorama, atmosphere or capture")),
    }
}

fn atmosphere_asset(value: Toml, what: &str) -> Result<Atmosphere, String> {
    let mut fields = Fields::of(value, what)?;
    let sun_elevation = fields
        .need("sun_elevation")?
        .f32_value(&format!("the sun elevation of {what}"))?;
    let sun_azimuth = fields
        .need("sun_azimuth")?
        .f32_value(&format!("the sun azimuth of {what}"))?;
    let sun_intensity = fields
        .need("sun_intensity")?
        .f32_value(&format!("the sun intensity of {what}"))?;
    let rayleigh = fields
        .need("rayleigh")?
        .f32_value(&format!("the Rayleigh scale of {what}"))?;
    let mie = fields
        .need("mie")?
        .f32_value(&format!("the Mie scale of {what}"))?;
    let mie_anisotropy = fields
        .need("mie_anisotropy")?
        .f32_value(&format!("the Mie anisotropy of {what}"))?;
    let ozone = fields
        .need("ozone")?
        .f32_value(&format!("the ozone scale of {what}"))?;
    let ground_albedo = Vec3::from_array(floats::<3>(
        fields.need("ground_albedo")?,
        &format!("the ground albedo of {what}"),
    )?);
    let altitude = fields
        .need("altitude")?
        .f32_value(&format!("the altitude of {what}"))?;
    fields.done()?;
    Ok(Atmosphere {
        sun_elevation,
        sun_azimuth,
        sun_intensity,
        rayleigh,
        mie,
        mie_anisotropy,
        ozone,
        ground_albedo,
        altitude,
    })
}

fn images(value: Toml, what: &str) -> Result<Vec<ImageEntry>, String> {
    value
        .array_items(&format!("the images of {what}"))?
        .into_iter()
        .enumerate()
        .map(|(index, item)| image(item, &format!("image {index} of {what}")))
        .collect()
}

fn image(value: Toml, what: &str) -> Result<ImageEntry, String> {
    let mut fields = Fields::of(value, what)?;
    let id = fields
        .need("id")?
        .string_value(&format!("the id of {what}"))?;
    let path = fields.take("path");
    let model = fields.take("model");
    let index = fields.take("image");
    let min_filter = filter(
        fields.need("min_filter")?,
        &format!("the min filter of {what}"),
    )?;
    let mag_filter = filter(
        fields.need("mag_filter")?,
        &format!("the mag filter of {what}"),
    )?;
    let mip_filter = filter(
        fields.need("mip_filter")?,
        &format!("the mip filter of {what}"),
    )?;
    let wrap_u = address(fields.need("wrap_u")?, &format!("the wrap u of {what}"))?;
    let wrap_v = address(fields.need("wrap_v")?, &format!("the wrap v of {what}"))?;
    fields.done()?;
    let sampler = SamplerDesc {
        min_filter,
        mag_filter,
        mipmap_filter: mip_filter,
        address_mode_u: wrap_u,
        address_mode_v: wrap_v,
        compare: None,
    };
    let entry = match (path, model, index) {
        (Some(path), None, None) => ImageEntry::new(
            id,
            crate::folder::relative_path(&path.string_value(&format!("the path of {what}"))?),
        ),
        (None, Some(model), Some(index)) => ImageEntry::embedded(
            id,
            ModelId::new(checked(
                model.string_value(&format!("the model of {what}"))?,
                ReferenceKind::Model,
            )?),
            index_count(index, &format!("the image of {what}"))?,
        ),
        _ => return Err(format!("{what} holds no single source")),
    };
    Ok(entry.with_sampler(sampler))
}

fn meshes(value: Toml, what: &str) -> Result<Vec<MeshEntry>, String> {
    value
        .array_items(&format!("the meshes of {what}"))?
        .into_iter()
        .enumerate()
        .map(|(index, item)| mesh(item, &format!("mesh {index} of {what}")))
        .collect()
}

fn mesh(value: Toml, what: &str) -> Result<MeshEntry, String> {
    let mut fields = Fields::of(value, what)?;
    let id = fields
        .need("id")?
        .string_value(&format!("the id of {what}"))?;
    let shape = fields.take("shape");
    let vertices = fields.take("vertices");
    let model = fields.take("model");
    let mesh = fields.take("mesh");
    let primitive = fields.take("primitive");
    fields.done()?;
    let asset = match (shape, vertices, model, mesh, primitive) {
        (Some(shape), None, None, None, None) => {
            let shape = shape.string_value(&format!("the shape of {what}"))?;
            MeshAsset::shape(
                Shape::from_name(&shape)
                    .ok_or_else(|| format!("{what} holds the shape {shape}"))?,
            )
        }
        (None, Some(vertices), None, None, None) => {
            let items = vertices.array_items(&format!("the vertices of {what}"))?;
            if items.is_empty() || items.len() % 3 != 0 {
                return Err(format!(
                    "{what} holds {} vertices where triangle vertices stand in threes",
                    items.len(),
                ));
            }
            MeshAsset::vertices(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| vertex(item, &format!("vertex {index} of {what}")))
                    .collect::<Result<Vec<Vertex>, String>>()?,
            )
        }
        (None, None, Some(model), Some(mesh), Some(primitive)) => MeshAsset::model(
            ModelId::new(checked(
                model.string_value(&format!("the model of {what}"))?,
                ReferenceKind::Model,
            )?),
            index_count(mesh, &format!("the mesh of {what}"))?,
            index_count(primitive, &format!("the primitive of {what}"))?,
        ),
        _ => return Err(format!("{what} holds no single geometry")),
    };
    Ok(MeshEntry::new(id, asset))
}

fn index_count(value: Toml, what: &str) -> Result<u32, String> {
    let count = value.u64_value(what)?;
    u32::try_from(count).map_err(|_| format!("{what} holds {count}"))
}

fn vertex(value: Toml, what: &str) -> Result<Vertex, String> {
    let mut fields = Fields::of(value, what)?;
    let position = floats(fields.need("position")?, &format!("the position of {what}"))?;
    let normal = floats(fields.need("normal")?, &format!("the normal of {what}"))?;
    let uv = floats(fields.need("uv")?, &format!("the uv of {what}"))?;
    fields.done()?;
    Ok(Vertex {
        position,
        normal,
        uv,
    })
}

fn skins(value: Toml, what: &str) -> Result<Vec<SkinEntry>, String> {
    value
        .array_items(&format!("the skins of {what}"))?
        .into_iter()
        .enumerate()
        .map(|(index, item)| skin(item, &format!("skin {index} of {what}")))
        .collect()
}

fn skin(value: Toml, what: &str) -> Result<SkinEntry, String> {
    let mut fields = Fields::of(value, what)?;
    let id = fields
        .need("id")?
        .string_value(&format!("the id of {what}"))?;
    let model = ModelId::new(checked(
        fields
            .need("model")?
            .string_value(&format!("the model of {what}"))?,
        ReferenceKind::Model,
    )?);
    let index = index_count(fields.need("skin")?, &format!("the skin of {what}"))?;
    let joints = fields
        .need("joints")?
        .array_items(&format!("the joints of {what}"))?
        .into_iter()
        .enumerate()
        .map(|(at, joint)| {
            Ok(NodeId::new(checked(
                joint.string_value(&format!("joint {at} of {what}"))?,
                ReferenceKind::Node,
            )?))
        })
        .collect::<Result<Vec<NodeId>, String>>()?;
    fields.done()?;
    if joints.is_empty() {
        return Err(format!("the skin asset {model} {index} names no joint",));
    }
    Ok(SkinEntry::new(id, SkinAsset::model(model, index, joints)))
}

fn animations(value: Toml, what: &str) -> Result<Vec<AnimationEntry>, String> {
    value
        .array_items(&format!("the animations of {what}"))?
        .into_iter()
        .enumerate()
        .map(|(index, item)| animation(item, &format!("animation {index} of {what}")))
        .collect()
}

fn animation(value: Toml, what: &str) -> Result<AnimationEntry, String> {
    let mut fields = Fields::of(value, what)?;
    let id = fields
        .need("id")?
        .string_value(&format!("the id of {what}"))?;
    let model = ModelId::new(checked(
        fields
            .need("model")?
            .string_value(&format!("the model of {what}"))?,
        ReferenceKind::Model,
    )?);
    let index = index_count(
        fields.need("animation")?,
        &format!("the animation of {what}"),
    )?;
    let targets = fields
        .need("targets")?
        .array_items(&format!("the targets of {what}"))?
        .into_iter()
        .enumerate()
        .map(|(at, target)| {
            Ok(NodeId::new(checked(
                target.string_value(&format!("target {at} of {what}"))?,
                ReferenceKind::Node,
            )?))
        })
        .collect::<Result<Vec<NodeId>, String>>()?;
    fields.done()?;
    if targets.is_empty() {
        return Err(format!(
            "the animation asset {model} {index} targets no node",
        ));
    }
    Ok(AnimationEntry::new(
        id,
        AnimationAsset::model(model, index, targets),
    ))
}

fn materials(value: Toml, what: &str) -> Result<Vec<MaterialEntry>, String> {
    value
        .array_items(&format!("the materials of {what}"))?
        .into_iter()
        .enumerate()
        .map(|(index, item)| material(item, &format!("material {index} of {what}")))
        .collect()
}

fn material(value: Toml, what: &str) -> Result<MaterialEntry, String> {
    let mut fields = Fields::of(value, what)?;
    let id = fields
        .need("id")?
        .string_value(&format!("the id of {what}"))?;
    let base_color = color(
        fields.need("base_color")?,
        &format!("the base color of {what}"),
    )?;
    let alpha = fields
        .need("alpha")?
        .f32_value(&format!("the alpha of {what}"))?;
    if !(0.0..=1.0).contains(&alpha) {
        return Err(format!("the alpha of {what} lies outside the unit range"));
    }
    let alpha_mode = alpha_mode(
        fields.need("alpha_mode")?,
        &format!("the alpha mode of {what}"),
    )?;
    let alpha_cutoff = fields
        .need("alpha_cutoff")?
        .f32_value(&format!("the alpha cutoff of {what}"))?;
    if !(0.0..=1.0).contains(&alpha_cutoff) {
        return Err(format!(
            "the alpha cutoff of {what} lies outside the unit range",
        ));
    }
    let base_color_image = image_field(fields.take("base_color_image"), what)?;
    let metallic = fields
        .need("metallic")?
        .f32_value(&format!("the metallic of {what}"))?;
    let roughness = fields
        .need("roughness")?
        .f32_value(&format!("the roughness of {what}"))?;
    let metallic_roughness_image = image_field(fields.take("metallic_roughness_image"), what)?;
    let normal_image = image_field(fields.take("normal_image"), what)?;
    let normal_scale = fields
        .need("normal_scale")?
        .f32_value(&format!("the normal scale of {what}"))?;
    let occlusion_image = image_field(fields.take("occlusion_image"), what)?;
    let occlusion_strength = fields
        .need("occlusion_strength")?
        .f32_value(&format!("the occlusion strength of {what}"))?;
    let emissive = color(fields.need("emissive")?, &format!("the emissive of {what}"))?;
    let emissive_image = image_field(fields.take("emissive_image"), what)?;
    fields.done()?;
    Ok(MaterialEntry::new(
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
    ))
}

fn image_field(value: Option<Toml>, what: &str) -> Result<Option<ImageId>, String> {
    match value {
        Some(value) => Ok(Some(ImageId::new(checked(
            value.string_value(&format!("the image of {what}"))?,
            ReferenceKind::Image,
        )?))),
        None => Ok(None),
    }
}

fn alpha_mode(value: Toml, what: &str) -> Result<AlphaMode, String> {
    let name = value.string_value(what)?;
    AlphaMode::of(&name).ok_or_else(|| format!("{what} holds the mode {name}"))
}

fn color(value: Toml, what: &str) -> Result<Vec3, String> {
    let color = vec3(value, what)?;
    if !(0.0..=1.0).contains(&color.x)
        || !(0.0..=1.0).contains(&color.y)
        || !(0.0..=1.0).contains(&color.z)
    {
        return Err(format!(
            "{what} holds the color {color:?} outside the unit cube",
        ));
    }
    Ok(color)
}

fn filter(value: Toml, what: &str) -> Result<dome_render::FilterMode, String> {
    let name = value.string_value(what)?;
    filter_of(&name).ok_or_else(|| format!("{what} holds the filter {name}"))
}

fn address(value: Toml, what: &str) -> Result<dome_render::AddressMode, String> {
    let name = value.string_value(what)?;
    address_of(&name).ok_or_else(|| format!("{what} holds the wrap {name}"))
}

fn nodes(value: Toml, what: &str, schema: &dyn Schema) -> Result<Vec<Node>, String> {
    value
        .array_items(&format!("the nodes of {what}"))?
        .into_iter()
        .enumerate()
        .map(|(index, item)| node(item, &format!("node {index} of {what}"), schema))
        .collect()
}

fn node(value: Toml, what: &str, schema: &dyn Schema) -> Result<Node, String> {
    let mut id = None;
    let mut parent = None;
    let mut transform = None;
    let mut prefab = None;
    let mut overrides = Vec::new();
    let mut additions = Vec::new();
    let mut removals = Vec::new();
    let mut components = BTreeMap::new();
    for (key, value) in value.object_fields(what)? {
        match key.as_str() {
            "id" => id = Some(value.string_value(&format!("the id of {what}"))?),
            "parent" => {
                parent = Some(value.string_value(&format!("the parent of {what}"))?);
            }
            "transform" => {
                transform = Some(crate::transform::parse(
                    value,
                    &format!("the transform of {what}"),
                )?);
            }
            "prefab" => {
                prefab = Some(crate::folder::relative_path(
                    &value.string_value(&format!("the prefab of {what}"))?,
                ));
            }
            "overrides" => {
                overrides = instance_overrides(value, &format!("the overrides of {what}"))?;
            }
            "additions" => {
                additions = nodes(value, &format!("the additions of {what}"), schema)?;
            }
            "removals" => {
                removals = value
                    .array_items(&format!("the removals of {what}"))?
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| item.string_value(&format!("removal {index} of {what}")))
                    .collect::<Result<Vec<_>, String>>()?;
            }
            other => {
                let component = schema.parse(other, value, &format!("the {other} of {what}"))?;
                components.insert(component.name(), component);
            }
        }
    }
    let id = id.ok_or_else(|| format!("{what} holds no id"))?;
    if id.is_empty() {
        return Err(format!("{what} holds no id"));
    }
    let id = checked(id, ReferenceKind::Node)?;
    Ok(Node {
        id,
        parent,
        transform: transform.unwrap_or(dome_math::Transform::IDENTITY),
        prefab,
        owner: None,
        overrides,
        additions,
        removals,
        components,
    })
}

fn instance_overrides(value: Toml, what: &str) -> Result<Vec<InstanceOverride>, String> {
    value
        .array_items(what)?
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let what = format!("override {index} of {what}");
            let mut node = None;
            let mut path = None;
            let mut value = None;
            for (key, held) in item.object_fields(&what)? {
                match key.as_str() {
                    "node" => node = Some(held.string_value(&format!("the node of {what}"))?),
                    "path" => path = Some(held.string_value(&format!("the path of {what}"))?),
                    "value" => value = Some(Value::parse(held, &format!("the value of {what}"))?),
                    other => return Err(format!("{what} holds the unknown field {other}")),
                }
            }
            Ok(InstanceOverride {
                node: node.ok_or_else(|| format!("{what} names no node"))?,
                path: path.ok_or_else(|| format!("{what} names no path"))?,
                value: value.ok_or_else(|| format!("{what} holds no value"))?,
            })
        })
        .collect()
}

fn vec3(value: Toml, what: &str) -> Result<Vec3, String> {
    Ok(Vec3::from_array(floats(value, what)?))
}

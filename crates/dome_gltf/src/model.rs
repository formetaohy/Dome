use crate::accessor::Store;
use crate::container::Container;
use crate::document::Document;
use crate::image::Image;
use crate::material::{self, Material};
use crate::node::{self, Node};
use crate::sampler::Sampler;
use crate::uri;
use crate::vertex::{SkinVertex, Vertex};
use dome_animation::{Interpolation, Property};
use dome_math::Mat4;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Default)]
pub struct ModelSources {
    pub buffers: Vec<PathBuf>,
    pub images: Vec<PathBuf>,
}

pub struct Model {
    pub images: Vec<Image>,
    pub materials: Vec<Material>,
    pub meshes: Vec<Mesh>,
    pub skins: Vec<Skin>,
    pub animations: Vec<Animation>,
    pub nodes: Vec<Node>,
    pub roots: Vec<usize>,
}

pub struct Skin {
    pub joints: Vec<usize>,
    pub skeleton: Option<usize>,
    pub inverse_binds: Vec<Mat4>,
}

pub struct Animation {
    pub name: Option<String>,
    pub channels: Vec<Channel>,
}

pub struct Channel {
    pub node: usize,
    pub property: Property,
    pub interpolation: Interpolation,
    pub times: Vec<f32>,
    pub values: Vec<f32>,
}

pub struct Mesh {
    pub primitives: Vec<Primitive>,
}

pub struct Primitive {
    pub vertices: Vec<Vertex>,
    pub skin: Option<Vec<SkinVertex>>,
    pub material: Option<usize>,
}

impl Model {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let what = format!("the model {}", path.display());
        let bytes = fs::read(path).map_err(|error| format!("{what} reads no bytes: {error}"))?;
        Self::parse(&bytes, path)
    }

    pub fn sources(path: impl AsRef<Path>) -> Result<ModelSources, String> {
        let path = path.as_ref();
        let what = format!("the model {}", path.display());
        let bytes = fs::read(path).map_err(|error| format!("{what} reads no bytes: {error}"))?;
        let container = Container::open(&bytes).map_err(|error| format!("{what} {error}"))?;
        let document: Document = serde_json::from_slice(container.json)
            .map_err(|error| format!("{what} holds no glTF text: {error}"))?;
        let base = base_of(path);
        let mut buffers = Vec::new();
        for buffer in &document.buffers {
            if let Some(uri) = &buffer.uri
                && let Some(source) =
                    uri::path(uri, base).map_err(|error| format!("{what} {error}"))?
            {
                buffers.push(source);
            }
        }
        let mut images = Vec::new();
        for image in &document.images {
            if let Some(uri) = &image.uri
                && let Some(source) =
                    uri::path(uri, base).map_err(|error| format!("{what} {error}"))?
            {
                images.push(source);
            }
        }
        buffers.sort();
        buffers.dedup();
        images.sort();
        images.dedup();
        Ok(ModelSources { buffers, images })
    }

    pub fn parse(bytes: &[u8], path: &Path) -> Result<Self, String> {
        let what = format!("the model {}", path.display());
        let container = Container::open(bytes).map_err(|error| format!("{what} {error}"))?;
        let document: Document = serde_json::from_slice(container.json)
            .map_err(|error| format!("{what} holds no glTF text: {error}"))?;
        check_version(&document, &what)?;
        let base = base_of(path);
        let store = Store::new(&document, base, container.binary, &what)?;
        let samplers = crate::sampler::image_samplers(&document, &what)?;
        let images = images(&document, &store, base, &what, &samplers)?;
        let materials = material::build(&document, &what)?;
        let meshes = meshes(&document, &store, &materials, &what)?;
        let skins = skins(&document, &store, &what)?;
        let animations = animations(&document, &store, &what)?;
        let nodes = node::build(&document, &what)?;
        validate_nodes(&nodes, &document, &skins, &what)?;
        validate_skins(&nodes, &meshes, &skins, &what)?;
        let roots = roots(&document, &nodes)?;
        Ok(Self {
            images,
            materials,
            meshes,
            skins,
            animations,
            nodes,
            roots,
        })
    }
}

fn base_of(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn check_version(document: &Document, what: &str) -> Result<(), String> {
    let version = &document.asset.version;
    let major = version
        .split_once('.')
        .map(|(major, _)| major)
        .unwrap_or(version);
    if major != "2" {
        return Err(format!("{what} holds the glTF version {version}"));
    }
    if let Some(required) = &document.extensions_required
        && !required.is_empty()
    {
        return Err(format!("{what} requires the extension {}", required[0]));
    }
    Ok(())
}

fn images(
    document: &Document,
    store: &Store<'_>,
    base: &Path,
    what: &str,
    samplers: &[Sampler],
) -> Result<Vec<Image>, String> {
    document
        .images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let held = format!("image {index} of {what}");
            let bytes = match (&image.uri, image.buffer_view) {
                (Some(uri), None) => Cow::Owned(uri::read(uri, base, &held)?),
                (None, Some(view)) => Cow::Borrowed(store.view_bytes(view)?),
                _ => return Err(format!("{held} holds no single source")),
            };
            Image::decode(&bytes, &held, samplers[index])
        })
        .collect()
}

fn meshes(
    document: &Document,
    store: &Store<'_>,
    materials: &[Material],
    what: &str,
) -> Result<Vec<Mesh>, String> {
    document
        .meshes
        .iter()
        .enumerate()
        .map(|(index, mesh)| {
            let held = format!("mesh {index} of {what}");
            let primitives = mesh
                .primitives
                .iter()
                .enumerate()
                .map(|(primitive, definition)| {
                    let at = format!("primitive {primitive} of {held}");
                    let material = definition.material;
                    if let Some(material) = material
                        && material >= materials.len()
                    {
                        return Err(format!(
                            "{at} names the material {material}, which the model does not hold"
                        ));
                    }
                    let uses_image =
                        material.is_some_and(|material| materials[material].uses_image());
                    let (vertices, skin) = crate::mesh::build(definition, store, uses_image, &at)?;
                    Ok(Primitive {
                        vertices,
                        skin,
                        material,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Mesh { primitives })
        })
        .collect()
}

fn skins(document: &Document, store: &Store<'_>, what: &str) -> Result<Vec<Skin>, String> {
    document
        .skins
        .iter()
        .enumerate()
        .map(|(index, skin)| {
            let held = format!("skin {index} of {what}");
            if skin.joints.is_empty() {
                return Err(format!("{held} names no joint"));
            }
            for (at, joint) in skin.joints.iter().enumerate() {
                if *joint >= document.nodes.len() {
                    return Err(format!(
                        "{held} names the node {joint}, which the model does not hold"
                    ));
                }
                if skin.joints[..at].contains(joint) {
                    return Err(format!("{held} names the node {joint} twice"));
                }
            }
            if let Some(skeleton) = skin.skeleton
                && skeleton >= document.nodes.len()
            {
                return Err(format!(
                    "{held} roots at the node {skeleton}, which the model does not hold"
                ));
            }
            let inverse_binds = match skin.inverse_bind_matrices {
                Some(accessor) => {
                    let accessor = store.accessor(accessor)?;
                    if accessor.count() != skin.joints.len() {
                        return Err(format!(
                            "{held} holds {} inverse bind matrices for {} joints",
                            accessor.count(),
                            skin.joints.len(),
                        ));
                    }
                    let mut matrices = Vec::with_capacity(accessor.count());
                    for at in 0..accessor.count() {
                        let matrix = accessor.matrix4(at)?;
                        if !finite_matrix(matrix) {
                            return Err(format!(
                                "{held} holds an inverse bind matrix of no finite number"
                            ));
                        }
                        matrices.push(matrix);
                    }
                    matrices
                }
                None => vec![Mat4::IDENTITY; skin.joints.len()],
            };
            Ok(Skin {
                joints: skin.joints.clone(),
                skeleton: skin.skeleton,
                inverse_binds,
            })
        })
        .collect()
}

fn animations(
    document: &Document,
    store: &Store<'_>,
    what: &str,
) -> Result<Vec<Animation>, String> {
    document
        .animations
        .iter()
        .enumerate()
        .map(|(index, animation)| {
            let held = format!("animation {index} of {what}");
            if animation.channels.is_empty() {
                return Err(format!("{held} holds no channel"));
            }
            let mut channels = Vec::with_capacity(animation.channels.len());
            let mut seen = BTreeSet::new();
            for (at, channel) in animation.channels.iter().enumerate() {
                let channel_what = format!("channel {at} of {held}");
                let sampler = animation.samplers.get(channel.sampler).ok_or_else(|| {
                    format!(
                        "{channel_what} names the sampler {}, which the animation does not hold",
                        channel.sampler,
                    )
                })?;
                let node = channel
                    .target
                    .node
                    .ok_or_else(|| format!("{channel_what} animates no node"))?;
                if node >= document.nodes.len() {
                    return Err(format!(
                        "{channel_what} animates the node {node}, which the model does not hold"
                    ));
                }
                let property = match channel.target.path.as_str() {
                    "translation" => Property::Translation,
                    "rotation" => Property::Rotation,
                    "scale" => Property::Scale,
                    "weights" => return Err(format!("{channel_what} animates morph weights")),
                    other => return Err(format!("{channel_what} animates the property {other}")),
                };
                if !seen.insert((node, property)) {
                    return Err(format!(
                        "{channel_what} animates the {} of node {node} twice",
                        property.name(),
                    ));
                }
                let interpolation = match sampler.interpolation.as_deref().unwrap_or("LINEAR") {
                    "STEP" => Interpolation::Step,
                    "LINEAR" => Interpolation::Linear,
                    "CUBICSPLINE" => Interpolation::CubicSpline,
                    other => return Err(format!("{channel_what} interpolates by {other}")),
                };
                let input = store.accessor(sampler.input)?;
                let mut times = Vec::with_capacity(input.count());
                for key in 0..input.count() {
                    times.push(input.scalar_at(key)?);
                }
                if times.is_empty() {
                    return Err(format!("{channel_what} holds no key"));
                }
                if !times.iter().all(|time| time.is_finite()) {
                    return Err(format!("{channel_what} holds a time of no finite number"));
                }
                if !times.windows(2).all(|window| window[0] < window[1]) {
                    return Err(format!("{channel_what} holds times out of order"));
                }
                let output = store.accessor(sampler.output)?;
                let stride = match interpolation {
                    Interpolation::CubicSpline => 3,
                    Interpolation::Step | Interpolation::Linear => 1,
                };
                if output.count() != times.len() * stride {
                    return Err(format!(
                        "{channel_what} holds {} values for {} keys",
                        output.count(),
                        times.len(),
                    ));
                }
                let components = property.components();
                let mut values = Vec::with_capacity(output.count() * components);
                for key in 0..output.count() {
                    if components == 4 {
                        values.extend(output.vector4(key)?);
                    } else {
                        values.extend(output.vector3(key)?);
                    }
                }
                if !values.iter().all(|value| value.is_finite()) {
                    return Err(format!("{channel_what} holds a value of no finite number"));
                }
                channels.push(Channel {
                    node,
                    property,
                    interpolation,
                    times,
                    values,
                });
            }
            Ok(Animation {
                name: animation.name.clone(),
                channels,
            })
        })
        .collect()
}

fn validate_skins(
    nodes: &[Node],
    meshes: &[Mesh],
    skins: &[Skin],
    what: &str,
) -> Result<(), String> {
    for (index, node) in nodes.iter().enumerate() {
        let Some(skin) = node.skin else {
            continue;
        };
        let Some(mesh) = node.mesh else {
            continue;
        };
        for (at, primitive) in meshes[mesh].primitives.iter().enumerate() {
            let Some(held) = primitive.skin.as_ref() else {
                return Err(format!(
                    "primitive {at} of the mesh {mesh} on node {index} of {what} holds no skin weights"
                ));
            };
            if held.len() != primitive.vertices.len() {
                return Err(format!(
                    "primitive {at} of the mesh {mesh} on node {index} of {what} holds {} skin weights for {} vertices",
                    held.len(),
                    primitive.vertices.len(),
                ));
            }
            for vertex in held {
                for joint in vertex.joints {
                    if (joint as usize) >= skins[skin].joints.len() {
                        return Err(format!(
                            "primitive {at} of the mesh {mesh} on node {index} of {what} names the joint {joint} beyond {}",
                            skins[skin].joints.len(),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn finite_matrix(matrix: Mat4) -> bool {
    matrix.cols.iter().all(|column| {
        column.x.is_finite() && column.y.is_finite() && column.z.is_finite() && column.w.is_finite()
    })
}

fn validate_nodes(
    nodes: &[Node],
    document: &Document,
    skins: &[Skin],
    what: &str,
) -> Result<(), String> {
    for (index, node) in nodes.iter().enumerate() {
        if let Some(skin) = node.skin
            && skin >= skins.len()
        {
            return Err(format!(
                "node {index} of {what} names the skin {skin}, which the model does not hold"
            ));
        }
        if let Some(mesh) = node.mesh
            && mesh >= document.meshes.len()
        {
            return Err(format!(
                "node {index} of {what} names the mesh {mesh}, which the model does not hold"
            ));
        }
        for child in &node.children {
            if *child >= nodes.len() {
                return Err(format!(
                    "node {index} of {what} holds the child {child}, which the model does not hold"
                ));
            }
        }
    }
    let mut state = vec![0u8; nodes.len()];
    for index in 0..nodes.len() {
        visit(nodes, index, &mut state, what)?;
    }
    Ok(())
}

fn visit(nodes: &[Node], index: usize, state: &mut [u8], what: &str) -> Result<(), String> {
    if state[index] != 0 {
        return Ok(());
    }
    state[index] = 1;
    let mut stack = vec![(index, 0usize)];
    while let Some((node, child)) = stack.last_mut() {
        let node = *node;
        if *child == nodes[node].children.len() {
            state[node] = 2;
            stack.pop();
            continue;
        }
        let next = nodes[node].children[*child];
        *child += 1;
        match state[next] {
            0 => {
                state[next] = 1;
                stack.push((next, 0));
            }
            1 => return Err(format!("{what} closes the node {next} into a cycle")),
            _ => {}
        }
    }
    Ok(())
}

fn roots(document: &Document, nodes: &[Node]) -> Result<Vec<usize>, String> {
    if !document.scenes.is_empty() {
        let index = document.scene.unwrap_or(0);
        let scene = document
            .scenes
            .get(index)
            .ok_or_else(|| format!("the model holds no scene {index}"))?;
        let roots = scene.nodes.clone().unwrap_or_default();
        for root in &roots {
            if *root >= nodes.len() {
                return Err(format!(
                    "scene {index} of the model names the node {root}, which the model does not hold"
                ));
            }
        }
        return Ok(roots);
    }
    let mut held = vec![false; nodes.len()];
    for node in nodes {
        for child in &node.children {
            held[*child] = true;
        }
    }
    Ok((0..nodes.len()).filter(|index| !held[*index]).collect())
}

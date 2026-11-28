use dome_gltf::Model;
use dome_math::Vec3;
use dome_scene::Edit;
use dome_scene::ImageEntry;
use dome_scene::ModelEntry;
use dome_scene::Node;
use dome_scene::{AnimationAsset, AnimationEntry};
use dome_scene::{AnimationId, ModelId, NodeId};
use dome_scene::{GraphAsset, GraphEntry, GraphState};
use dome_scene::{MaterialAsset, MaterialEntry};
use dome_scene::{MeshAsset, MeshEntry};
use dome_scene::{SkinAsset, SkinEntry};
use dome_scene::{Target, valid_id};
use dome_schema::NodeComponents;
use dome_schema::{AnimationRef, MeshRef, SkinRef};
use std::collections::{HashMap, HashSet};

pub trait Import {
    fn import_model(&mut self, id: &str, path: &str, place: Option<&str>)
    -> Result<Target, String>;
}

impl Import for Edit<'_> {
    fn import_model(
        &mut self,
        id: &str,
        path: &str,
        parent: Option<&str>,
    ) -> Result<Target, String> {
        if !valid_id(id) {
            return Err(format!("the model id {id} holds no letter, digit, _ or -"));
        }
        if self.model_index(id).is_some() {
            return Err(format!("the scene holds the model {id}"));
        }
        let path = dome_scene::relative_path(path);
        if !dome_scene::Folder::Asset.holds(&path) {
            return Err(format!(
                "the model {id} holds the path {} outside the assets folder",
                path.display(),
            ));
        }
        let file = self.asset(&path);
        if !file.is_file() {
            return Err(format!(
                "the model {id} names {}, which the project {} does not hold",
                path.display(),
                self.root().display(),
            ));
        }
        let parent = match parent {
            Some(parent) => Some(
                self.node_index(parent)
                    .ok_or_else(|| format!("the scene holds no node {parent}"))?,
            ),
            None => None,
        };
        let model = Model::load(&file)
            .map_err(|error| format!("the model {} loads: {error}", path.display()))?;
        if model.roots.is_empty() {
            return Err(format!("the model {} holds no root node", path.display()));
        }
        let Reach { order, parents } = reach(&model)?;
        let mut node_ids = HashMap::new();
        let mut used = self
            .nodes()
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        let holder = unique(id, &mut used);
        for index in &order {
            let base = name_of(&model, *index, id);
            node_ids.insert(*index, unique(&base, &mut used));
        }
        let mut used_skins = Vec::new();
        for index in &order {
            if let Some(skin) = model.nodes[*index].skin
                && !used_skins.contains(&skin)
            {
                used_skins.push(skin);
            }
        }
        let mut skin_ids = HashMap::new();
        let mut used_skin_ids = self
            .skins()
            .iter()
            .map(|skin| skin.id.clone())
            .collect::<HashSet<_>>();
        for index in &used_skins {
            let base = format!("{id}_skin_{index}");
            skin_ids.insert(*index, unique(&base, &mut used_skin_ids));
        }
        let mut animation_ids = HashMap::new();
        let mut used_animation_ids = self
            .animations()
            .iter()
            .map(|animation| animation.id.clone())
            .collect::<HashSet<_>>();
        for index in 0..model.animations.len() {
            let base = format!("{id}_animation_{index}");
            animation_ids.insert(index, unique(&base, &mut used_animation_ids));
        }
        let graph_id = (!model.animations.is_empty()).then(|| {
            let mut used = self
                .graphs()
                .iter()
                .map(|graph| graph.id.clone())
                .collect::<HashSet<_>>();
            unique(&format!("{id}_graph"), &mut used)
        });
        let mut primitive_ids = HashMap::new();
        for index in &order {
            let Some(mesh) = model.nodes[*index].mesh else {
                continue;
            };
            let primitives = primitives_of(&model, mesh, &path)?;
            if primitives > 1 {
                for primitive in 0..primitives {
                    let base = format!("{}_p{primitive}", node_ids[index]);
                    primitive_ids.insert((*index, primitive), unique(&base, &mut used));
                }
            }
        }
        let model_id = ModelId::new(id);
        let mut mesh_ids = HashMap::new();
        let mut used_primitives = Vec::new();
        let mut used_meshes = self
            .meshes()
            .iter()
            .map(|mesh| mesh.id.clone())
            .collect::<HashSet<_>>();
        for index in &order {
            let Some(mesh) = model.nodes[*index].mesh else {
                continue;
            };
            let primitives = primitives_of(&model, mesh, &path)?;
            for primitive in 0..primitives {
                let key = (mesh, primitive);
                if mesh_ids.contains_key(&key) {
                    continue;
                }
                let base = format!("{id}_mesh_{mesh}_{primitive}");
                mesh_ids.insert(key, unique(&base, &mut used_meshes));
                used_primitives.push(key);
            }
        }
        let mut material_ids = HashMap::new();
        let mut used_materials = self
            .materials()
            .iter()
            .map(|material| material.id.clone())
            .collect::<HashSet<_>>();
        for index in 0..model.materials.len() {
            let base = format!("{id}_material_{index}");
            material_ids.insert(index, unique(&base, &mut used_materials));
        }
        let mut image_ids = HashMap::new();
        let mut used_images = self
            .images()
            .iter()
            .map(|image| image.id.clone())
            .collect::<HashSet<_>>();
        for index in 0..model.images.len() {
            let base = format!("{id}_image_{index}");
            image_ids.insert(index, unique(&base, &mut used_images));
        }
        let mut needs_default = false;
        for index in &order {
            let Some(mesh) = model.nodes[*index].mesh else {
                continue;
            };
            for primitive in &model.meshes[mesh].primitives {
                if primitive.material.is_none() {
                    needs_default = true;
                }
            }
        }
        let default_material =
            needs_default.then(|| unique(&format!("{id}_material"), &mut used_materials));
        let mut nodes = Vec::new();
        let mut wrapper = Node::new(holder.as_str());
        if let Some(parent) = parent {
            wrapper.parent = Some(self.nodes()[parent].id.clone());
        }
        if let Some(graph) = &graph_id {
            wrapper.insert(AnimationRef::new(
                graph.as_str(),
                true,
                AnimationRef::DEFAULT_FADE,
            ));
        }
        nodes.push(wrapper);
        for index in &order {
            let source = &model.nodes[*index];
            let mut node = Node::new(node_ids[index].clone());
            node.set_transform(source.transform);
            node.parent = Some(match parents.get(index).copied().flatten() {
                Some(parent) => node_ids[&parent].clone(),
                None => holder.clone(),
            });
            if let Some(mesh) = source.mesh {
                if model.meshes[mesh].primitives.is_empty() {
                    return Err(format!(
                        "the model {} holds the mesh {mesh} of no primitive",
                        path.display(),
                    ));
                }
                let primitives = &model.meshes[mesh].primitives;
                if primitives.len() == 1 {
                    node.insert(mesh_ref(
                        &model,
                        &mesh_ids,
                        &material_ids,
                        &default_material,
                        mesh,
                        0,
                    ));
                }
            }
            if let Some(skin) = source.skin
                && source.mesh.is_some()
            {
                node.insert(SkinRef::new(skin_ids[&skin].as_str()));
            }
            let parent = node.id.clone();
            nodes.push(node);
            if let Some(mesh) = source.mesh
                && model.meshes[mesh].primitives.len() > 1
            {
                for primitive in 0..model.meshes[mesh].primitives.len() {
                    let mut child = Node::new(primitive_ids[&(*index, primitive)].clone());
                    child.parent = Some(parent.clone());
                    child.insert(mesh_ref(
                        &model,
                        &mesh_ids,
                        &material_ids,
                        &default_material,
                        mesh,
                        primitive,
                    ));
                    if let Some(skin) = source.skin {
                        child.insert(SkinRef::new(skin_ids[&skin].as_str()));
                    }
                    nodes.push(child);
                }
            }
        }
        let image_id = |image: &Option<usize>| {
            image.map(|image| dome_scene::ImageId::new(image_ids[&image].clone()))
        };
        let mut materials = model
            .materials
            .iter()
            .enumerate()
            .map(|(index, material)| {
                MaterialEntry::new(
                    material_ids[&index].as_str(),
                    MaterialAsset {
                        base_color: material.base_color,
                        alpha: material.alpha,
                        alpha_mode: crate::convert::alpha_mode(material.alpha_mode),
                        alpha_cutoff: material.alpha_cutoff,
                        base_color_image: image_id(&material.base_color_image),
                        metallic: material.metallic,
                        roughness: material.roughness,
                        metallic_roughness_image: image_id(&material.metallic_roughness_image),
                        normal_image: image_id(&material.normal_image),
                        normal_scale: material.normal_scale,
                        occlusion_image: image_id(&material.occlusion_image),
                        occlusion_strength: material.occlusion_strength,
                        emissive: material.emissive,
                        emissive_image: image_id(&material.emissive_image),
                    },
                )
            })
            .collect::<Vec<_>>();
        if let Some(default_material) = &default_material {
            materials.push(MaterialEntry::new(
                default_material.as_str(),
                MaterialAsset::grey(),
            ));
        }
        let images = (0..model.images.len())
            .map(|image| {
                ImageEntry::embedded(
                    image_ids[&image].as_str(),
                    model_id.clone(),
                    u32::try_from(image).expect("a model holds fewer than four billion images"),
                )
                .with_sampler(crate::convert::sampler(model.images[image].sampler()))
            })
            .collect::<Vec<_>>();
        let meshes = used_primitives
            .into_iter()
            .map(|(mesh, primitive)| {
                MeshEntry::new(
                    mesh_ids[&(mesh, primitive)].as_str(),
                    MeshAsset::model(
                        model_id.clone(),
                        u32::try_from(mesh).expect("a model holds fewer than four billion meshes"),
                        u32::try_from(primitive)
                            .expect("a mesh holds fewer than four billion primitives"),
                    ),
                )
            })
            .collect::<Vec<_>>();
        let skins = used_skins
            .iter()
            .map(|index| {
                let held = &model.skins[*index];
                let joints = held
                    .joints
                    .iter()
                    .map(|joint| {
                        node_ids
                            .get(joint)
                            .cloned()
                            .map(NodeId::new)
                            .ok_or_else(|| {
                                format!(
                                    "the model {} holds a skin joint {joint} outside its scene",
                                    path.display(),
                                )
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(SkinEntry::new(
                    skin_ids[index].as_str(),
                    SkinAsset::model(
                        model_id.clone(),
                        u32::try_from(*index).expect("a model holds fewer than four billion skins"),
                        joints,
                    ),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let animations = model
            .animations
            .iter()
            .enumerate()
            .map(|(index, held)| {
                let targets = held
                    .channels
                    .iter()
                    .map(|channel| {
                        node_ids
                            .get(&channel.node)
                            .cloned()
                            .map(NodeId::new)
                            .ok_or_else(|| {
                                format!(
                                    "the model {} holds an animation on node {} outside its scene",
                                    path.display(),
                                    channel.node,
                                )
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(AnimationEntry::new(
                    animation_ids[&index].as_str(),
                    AnimationAsset::model(
                        model_id.clone(),
                        u32::try_from(index)
                            .expect("a model holds fewer than four billion animations"),
                        targets,
                    ),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        self.adopt(ModelEntry::new(id, path));
        for entry in images {
            self.adopt(entry);
        }
        for entry in materials {
            self.adopt(entry);
        }
        for entry in meshes {
            self.adopt(entry);
        }
        for entry in skins {
            self.adopt(entry);
        }
        for entry in animations {
            self.adopt(entry);
        }
        if let Some(graph) = graph_id {
            let mut used = HashSet::new();
            let mut entry = String::new();
            let states = (0..model.animations.len())
                .map(|index| {
                    let base = model.animations[index]
                        .name
                        .as_deref()
                        .map(sanitized)
                        .filter(|held| valid_id(held))
                        .unwrap_or_else(|| format!("animation_{index}"));
                    let id = unique(&base, &mut used);
                    if entry.is_empty() {
                        entry = id.clone();
                    }
                    GraphState::new(
                        &id,
                        AnimationId::new(animation_ids[&index].as_str()),
                        1.0,
                        true,
                    )
                    .unwrap_or_else(|error| panic!("{error}"))
                })
                .collect();
            self.adopt(GraphEntry::new(
                graph,
                GraphAsset::new(entry, Vec::new(), states, Vec::new()),
            ));
        }
        self.append_nodes(nodes);
        Ok(Target::Node(holder))
    }
}

fn mesh_ref(
    model: &Model,
    meshes: &HashMap<(usize, usize), String>,
    materials: &HashMap<usize, String>,
    default: &Option<String>,
    mesh: usize,
    index: usize,
) -> MeshRef {
    let primitive = &model.meshes[mesh].primitives[index];
    let mesh = meshes[&(mesh, index)].as_str();
    let material = match primitive.material {
        Some(index) => materials[&index].as_str(),
        None => default
            .as_ref()
            .expect("a default material stands")
            .as_str(),
    };
    MeshRef::new(mesh, material, Vec3::ONE)
}

fn primitives_of(model: &Model, mesh: usize, path: &std::path::Path) -> Result<usize, String> {
    let held = model
        .meshes
        .get(mesh)
        .ok_or_else(|| format!("the model {} holds no mesh {mesh}", path.display()))?;
    Ok(held.primitives.len())
}

fn name_of(model: &Model, index: usize, id: &str) -> String {
    let name = model.nodes[index]
        .name
        .as_deref()
        .map(sanitized)
        .unwrap_or_else(|| format!("node{index}"));
    format!("{id}_{name}")
}

fn sanitized(name: &str) -> String {
    let mut text = name
        .chars()
        .map(|piece| {
            if piece.is_ascii_alphanumeric() || piece == '_' || piece == '-' {
                piece
            } else {
                '_'
            }
        })
        .collect::<String>();
    if text.is_empty() {
        text.push_str("node");
    }
    text
}

fn unique(base: &str, used: &mut HashSet<String>) -> String {
    let mut id = String::from(base);
    let mut count = 0;
    while used.contains(&id) {
        count += 1;
        id = format!("{base}_{count}");
    }
    used.insert(id.clone());
    id
}

struct Reach {
    order: Vec<usize>,
    parents: HashMap<usize, Option<usize>>,
}

fn reach(model: &Model) -> Result<Reach, String> {
    let mut order = Vec::new();
    let mut parents = HashMap::new();
    let mut state = vec![0u8; model.nodes.len()];
    for root in &model.roots {
        visit(*root, None, model, &mut order, &mut parents, &mut state)?;
    }
    Ok(Reach { order, parents })
}

fn visit(
    index: usize,
    parent: Option<usize>,
    model: &Model,
    order: &mut Vec<usize>,
    parents: &mut HashMap<usize, Option<usize>>,
    state: &mut [u8],
) -> Result<(), String> {
    match state[index] {
        1 => return Err(format!("the model closes the node {index} into a cycle")),
        2 => return Err(format!("the model shares the node {index} between parents")),
        _ => {}
    }
    state[index] = 1;
    parents.insert(index, parent);
    order.push(index);
    for child in &model.nodes[index].children {
        visit(*child, Some(index), model, order, parents, state)?;
    }
    state[index] = 2;
    Ok(())
}

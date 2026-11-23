use crate::animation::AnimationEntry;
use crate::edit::Edit;
use crate::environment::EnvironmentEntry;
use crate::graph::GraphEntry;
use crate::image::ImageEntry;
use crate::material::MaterialEntry;
use crate::mesh::MeshEntry;
use crate::model::ModelEntry;
use crate::prefab::{Loader, describe};
use crate::scene::{Owned, Scene};
use crate::schema::ReferenceKind;
use crate::skin::SkinEntry;
use crate::sound::SoundEntry;
use crate::target::Target;
use std::collections::{BTreeSet, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(crate) struct Deletion {
    nodes: BTreeSet<usize>,
    assets: Vec<(ReferenceKind, String)>,
    instances: BTreeSet<String>,
    edits: Vec<MarkerEdit>,
}

struct MarkerEdit {
    marker: String,
    prefix: String,
    relative: String,
    redundant: HashSet<String>,
    addition: bool,
    removed: HashSet<String>,
}

pub(crate) fn plan(scene: &Scene, target: &Target) -> Result<Deletion, String> {
    let mut planner = Planner {
        scene,
        plan: Deletion::default(),
    };
    planner.remove(target)?;
    Ok(planner.plan)
}

pub(crate) fn apply(edit: &mut Edit<'_>, deletion: Deletion) {
    let Deletion {
        nodes,
        assets,
        instances,
        edits,
    } = deletion;
    let indices = nodes.into_iter().collect::<Vec<_>>();
    edit.remove_nodes(&indices);
    for (kind, id) in &assets {
        detach_entry(edit, *kind, id);
    }
    let owners = edit
        .owners
        .iter()
        .filter(|held| {
            !assets
                .iter()
                .any(|(kind, id)| *kind == held.kind && id == &held.id)
        })
        .cloned()
        .collect();
    edit.set_owners(owners);
    for id in &instances {
        edit.set_instance(id, None);
    }
    if edits.is_empty() {
        return;
    }
    for held in &edits {
        held.apply(edit);
    }
    edit.validate("a removed instance node");
}

fn detach_entry(edit: &mut Edit<'_>, kind: ReferenceKind, id: &str) {
    let index = match kind {
        ReferenceKind::Mesh => edit.mesh_index(id),
        ReferenceKind::Material => edit.material_index(id),
        ReferenceKind::Image => edit.image_index(id),
        ReferenceKind::Model => edit.model_index(id),
        ReferenceKind::Sound => edit.sound_index(id),
        ReferenceKind::Environment => edit.environment_index(id),
        ReferenceKind::Graph => edit.graph_index(id),
        ReferenceKind::Animation => edit.animation_index(id),
        ReferenceKind::Skin => edit.skin_index(id),
        ReferenceKind::Node | ReferenceKind::AudioBus => {
            panic!("an asset of the {kind} kind detaches", kind = kind.name())
        }
    }
    .unwrap_or_else(|| {
        panic!(
            "the deletion detaches the {kind} {id} the scene holds",
            kind = kind.name(),
        )
    });
    match kind {
        ReferenceKind::Mesh => edit.remove_asset::<MeshEntry>(index),
        ReferenceKind::Material => edit.remove_asset::<MaterialEntry>(index),
        ReferenceKind::Image => edit.remove_asset::<ImageEntry>(index),
        ReferenceKind::Model => edit.remove_asset::<ModelEntry>(index),
        ReferenceKind::Sound => edit.remove_asset::<SoundEntry>(index),
        ReferenceKind::Environment => edit.remove_asset::<EnvironmentEntry>(index),
        ReferenceKind::Graph => edit.remove_asset::<GraphEntry>(index),
        ReferenceKind::Animation => edit.remove_asset::<AnimationEntry>(index),
        ReferenceKind::Skin => edit.remove_asset::<SkinEntry>(index),
        ReferenceKind::Node | ReferenceKind::AudioBus => {
            panic!("an asset of the {kind} kind detaches", kind = kind.name())
        }
    }
}

impl MarkerEdit {
    fn apply(&self, edit: &mut Edit<'_>) {
        let marker = edit.node_index(&self.marker).unwrap_or_else(|| {
            panic!(
                "the deleted subtree keeps the prefab instance {}",
                self.marker,
            )
        });
        let removed = &self.removed;
        let prefix = &self.prefix;
        let relative = &self.relative;
        let redundant = &self.redundant;
        let addition = self.addition;
        edit.change_node(marker, |node, _| {
            node.additions
                .retain(|held| !removed.contains(&format!("{prefix}{}", held.id)));
            node.overrides
                .retain(|entry| !removed.contains(&format!("{prefix}{}", entry.node)));
            if !addition {
                node.removals
                    .retain(|removal| removal != relative && !redundant.contains(removal));
                node.removals.push(relative.clone());
            }
            Ok(())
        })
        .expect("a prefab instance keeps the structure a deletion leaves");
    }
}

struct Planner<'a> {
    scene: &'a Scene,
    plan: Deletion,
}

impl Planner<'_> {
    fn remove(&mut self, target: &Target) -> Result<(), String> {
        let removed = match target {
            Target::Node(id) => self.removed_subtree(id)?,
            _ => Vec::new(),
        };
        self.detach(target, false)?;
        for scope in self.removed_scopes(removed.iter()) {
            self.drop_scoped(&scope)?;
        }
        let keys = self.opened_instances(&removed);
        for id in removed {
            self.plan.instances.insert(id);
        }
        self.prune(&keys)
    }

    fn removed_subtree(&self, id: &str) -> Result<Vec<String>, String> {
        let index = self
            .scene
            .node_index(id)
            .ok_or_else(|| format!("the scene holds no node {id}"))?;
        Ok(self
            .scene
            .subtree(index)
            .into_iter()
            .map(|at| self.scene.nodes[at].id.clone())
            .collect())
    }

    fn removed_scopes<'b>(&self, removed: impl Iterator<Item = &'b String>) -> Vec<String> {
        removed
            .filter(|id| self.scene.instances.contains_key(id.as_str()))
            .filter(|id| !self.plan.instances.contains(*id))
            .cloned()
            .collect()
    }

    fn opened_instances(&self, removed: &[String]) -> BTreeSet<PathBuf> {
        removed
            .iter()
            .filter(|id| !self.plan.instances.contains(*id))
            .filter_map(|id| self.scene.instances.get(id))
            .flatten()
            .cloned()
            .collect()
    }

    fn detach(&mut self, target: &Target, owned: bool) -> Result<(), String> {
        if !owned {
            if let Target::Node(id) = target
                && let Some(index) = self.scene.node_index(id)
                && self.scene.nodes[index].owner.is_some()
            {
                return self.detach_instance(index);
            }
            if let Some(prefab) = self.scene.prefab_owner(target) {
                return Err(format!(
                    "{target} belongs to the prefab {}",
                    prefab.display(),
                ));
            }
        }
        match target {
            Target::Node(id) => self.detach_node(id),
            Target::Mesh(id) => self.detach_mesh(id),
            Target::Material(id) => self.detach_material(id),
            Target::Image(id) => self.detach_image(id),
            Target::Model(id) => self.detach_model(id),
            Target::Sound(id) => self.detach_sound(id),
            Target::Environment(id) => self.detach_environment(id),
            Target::Graph(id) => self.detach_graph(id),
        }
    }

    fn detach_node(&mut self, id: &str) -> Result<(), String> {
        let index = self
            .scene
            .node_index(id)
            .ok_or_else(|| format!("the scene holds no node {id}"))?;
        let held = self.scene.subtree(index);
        let removed = held
            .iter()
            .map(|at| self.scene.nodes[*at].id.clone())
            .collect::<HashSet<_>>();
        for name in &removed {
            if self.referenced(&Target::Node(name.clone()), Some(&held)) {
                return Err(format!("the scene references the node {name}"));
            }
        }
        self.detached_surface(ReferenceKind::Skin, "deforms a skin", &removed, id)?;
        self.detached_surface(ReferenceKind::Animation, "animates", &removed, id)?;
        for at in held {
            self.plan.nodes.insert(at);
        }
        Ok(())
    }

    fn detach_instance(&mut self, index: usize) -> Result<(), String> {
        let node = &self.scene.nodes[index];
        let id = node.id.clone();
        let owner = node
            .owner
            .clone()
            .expect("an instance node names its owner");
        let top = self
            .scene
            .instance_marker(&id)
            .expect("an instance node belongs to a prefab instance")
            .to_string();
        if owner != top {
            return Err(format!(
                "the prefab instance {top} keeps the structure of its nested prefab {owner}",
            ));
        }
        let subtree = self.scene.subtree(index);
        let removed = subtree
            .iter()
            .map(|at| self.scene.nodes[*at].id.clone())
            .collect::<HashSet<_>>();
        for held in &removed {
            if self.referenced(&Target::Node(held.clone()), Some(&subtree)) {
                return Err(format!("the scene references the node {held}"));
            }
        }
        self.detached_surface(ReferenceKind::Skin, "deforms a skin", &removed, &id)?;
        self.detached_surface(ReferenceKind::Animation, "animates", &removed, &id)?;
        let prefix = format!("{top}_");
        let relative = id
            .strip_prefix(&prefix)
            .unwrap_or_else(|| panic!("the node {id} belongs to the prefab instance {top}"))
            .to_string();
        let marker = self
            .scene
            .node_index(&top)
            .expect("a prefab instance names its marker");
        let addition = self.scene.nodes[marker]
            .additions
            .iter()
            .any(|held| held.id == relative);
        let redundant = if addition {
            HashSet::new()
        } else {
            self.instance_removal_nodes(&top, &relative)?
        };
        for at in subtree {
            self.plan.nodes.insert(at);
        }
        for held in &removed {
            self.plan.instances.insert(held.clone());
        }
        for scope in self.removed_scopes(removed.iter()) {
            self.drop_scoped(&scope)?;
        }
        self.plan.edits.push(MarkerEdit {
            marker: top,
            prefix,
            relative,
            redundant,
            addition,
            removed,
        });
        Ok(())
    }

    fn detached_surface(
        &self,
        kind: ReferenceKind,
        effect: &str,
        removed: &HashSet<String>,
        id: &str,
    ) -> Result<(), String> {
        let referenced = match kind {
            ReferenceKind::Skin => self.scene.skins.iter().any(|skin| {
                !self
                    .scene
                    .scoped_away(ReferenceKind::Skin, &skin.id, removed)
                    && skin
                        .asset
                        .joints()
                        .iter()
                        .any(|joint| removed.contains(joint.as_str()))
            }),
            ReferenceKind::Animation => self.scene.animations.iter().any(|animation| {
                !self
                    .scene
                    .scoped_away(ReferenceKind::Animation, &animation.id, removed)
                    && animation
                        .asset
                        .targets()
                        .iter()
                        .any(|target| removed.contains(target.as_str()))
            }),
            _ => panic!(
                "a detached surface of the {kind} kind exists",
                kind = kind.name(),
            ),
        };
        if referenced {
            return Err(format!("the scene {effect} through the node {id}"));
        }
        Ok(())
    }

    fn detach_mesh(&mut self, id: &str) -> Result<(), String> {
        self.scene
            .mesh_index(id)
            .ok_or_else(|| format!("the scene holds no mesh {id}"))?;
        if self.referenced(&Target::Mesh(String::from(id)), None) {
            return Err(format!("the scene references the mesh {id}"));
        }
        self.detached(ReferenceKind::Mesh, id);
        Ok(())
    }

    fn detach_material(&mut self, id: &str) -> Result<(), String> {
        self.scene
            .material_index(id)
            .ok_or_else(|| format!("the scene holds no material {id}"))?;
        if self.referenced(&Target::Material(String::from(id)), None) {
            return Err(format!("the scene references the material {id}"));
        }
        self.detached(ReferenceKind::Material, id);
        Ok(())
    }

    fn detach_image(&mut self, id: &str) -> Result<(), String> {
        self.scene
            .image_index(id)
            .ok_or_else(|| format!("the scene holds no image {id}"))?;
        let referenced = self.referenced(&Target::Image(String::from(id)), None)
            || self.scene.materials.iter().any(|material| {
                !self.planned(ReferenceKind::Material, &material.id)
                    && material.asset.references_image(id)
            });
        if referenced {
            return Err(format!("the scene references the image {id}"));
        }
        self.detached(ReferenceKind::Image, id);
        Ok(())
    }

    fn detach_sound(&mut self, id: &str) -> Result<(), String> {
        self.scene
            .sound_index(id)
            .ok_or_else(|| format!("the scene holds no sound {id}"))?;
        if self.referenced(&Target::Sound(String::from(id)), None) {
            return Err(format!("the scene references the sound {id}"));
        }
        self.detached(ReferenceKind::Sound, id);
        Ok(())
    }

    fn detach_environment(&mut self, id: &str) -> Result<(), String> {
        self.scene
            .environment_index(id)
            .ok_or_else(|| format!("the scene holds no environment {id}"))?;
        if self.referenced(&Target::Environment(String::from(id)), None) {
            return Err(format!("the scene references the environment {id}"));
        }
        self.detached(ReferenceKind::Environment, id);
        Ok(())
    }

    fn detach_model(&mut self, id: &str) -> Result<(), String> {
        self.scene
            .model_index(id)
            .ok_or_else(|| format!("the scene holds no model {id}"))?;
        let referenced = self.scene.meshes.iter().any(|mesh| {
            !self.planned(ReferenceKind::Mesh, &mesh.id)
                && mesh
                    .asset
                    .model_of()
                    .is_some_and(|(model, ..)| model.as_str() == id)
        }) || self.scene.images.iter().any(|image| {
            !self.planned(ReferenceKind::Image, &image.id)
                && image
                    .source
                    .model()
                    .is_some_and(|(model, _)| model.as_str() == id)
        }) || self.scene.skins.iter().any(|skin| {
            !self.planned(ReferenceKind::Skin, &skin.id) && skin.asset.model_of().as_str() == id
        }) || self.scene.animations.iter().any(|animation| {
            !self.planned(ReferenceKind::Animation, &animation.id)
                && animation.asset.model_of().as_str() == id
        });
        if referenced {
            return Err(format!("the scene references the model {id}"));
        }
        self.detached(ReferenceKind::Model, id);
        Ok(())
    }

    fn detach_graph(&mut self, id: &str) -> Result<(), String> {
        self.scene
            .graph_index(id)
            .ok_or_else(|| format!("the scene holds no graph {id}"))?;
        if self.referenced(&Target::Graph(String::from(id)), None) {
            return Err(format!("the scene references the graph {id}"));
        }
        self.detached(ReferenceKind::Graph, id);
        Ok(())
    }

    fn detached(&mut self, kind: ReferenceKind, id: &str) {
        self.plan.assets.push((kind, String::from(id)));
    }

    fn drop_owned(&mut self, kind: ReferenceKind, id: &str) -> Result<(), String> {
        match kind {
            ReferenceKind::Skin => {
                if self.scene.skin_index(id).is_none() {
                    panic!("the scene owns the skin {id}, which it does not hold");
                }
                if self.referenced_by(kind, id) {
                    return Err(format!("the scene references the skin {id}"));
                }
            }
            ReferenceKind::Animation => {
                if self.scene.animation_index(id).is_none() {
                    panic!("the scene owns the animation {id}, which it does not hold");
                }
                let referenced = self.referenced_by(kind, id)
                    || self.scene.graphs.iter().any(|graph| {
                        !self.planned(ReferenceKind::Graph, &graph.id)
                            && graph.asset.references_clip(id)
                    });
                if referenced {
                    return Err(format!("the scene references the animation {id}"));
                }
            }
            ReferenceKind::Graph => {
                if self.scene.graph_index(id).is_none() {
                    panic!("the scene owns the graph {id}, which it does not hold");
                }
                if self.referenced_by(kind, id) {
                    return Err(format!("the scene references the graph {id}"));
                }
            }
            _ => panic!("an instance owns the {kind} kind", kind = kind.name()),
        }
        self.detached(kind, id);
        Ok(())
    }

    fn drop_scoped(&mut self, scope: &str) -> Result<(), String> {
        for kind in [
            ReferenceKind::Graph,
            ReferenceKind::Animation,
            ReferenceKind::Skin,
        ] {
            for id in self.owned(kind, |held| held.scope.as_deref() == Some(scope)) {
                self.drop_owned(kind, &id)?;
            }
        }
        Ok(())
    }

    fn prune(&mut self, keys: &BTreeSet<PathBuf>) -> Result<(), String> {
        for key in keys {
            if self
                .scene
                .instances
                .iter()
                .any(|(id, held)| !self.plan.instances.contains(id) && held.contains(key))
            {
                continue;
            }
            self.drop_prefab(key)?;
        }
        Ok(())
    }

    fn drop_prefab(&mut self, prefab: &Path) -> Result<(), String> {
        for id in self.owned(ReferenceKind::Graph, |held| {
            held.prefab == prefab && held.scope.is_none()
        }) {
            self.detach(&Target::Graph(id), true)?;
        }
        for kind in [ReferenceKind::Animation, ReferenceKind::Skin] {
            for id in self.owned(kind, |held| held.prefab == prefab && held.scope.is_none()) {
                self.drop_owned(kind, &id)?;
            }
        }
        for kind in [
            ReferenceKind::Material,
            ReferenceKind::Mesh,
            ReferenceKind::Image,
            ReferenceKind::Model,
            ReferenceKind::Sound,
            ReferenceKind::Environment,
        ] {
            for id in self.owned(kind, |held| held.prefab == prefab && held.scope.is_none()) {
                let target = target_of(kind, &id);
                self.detach(&target, true)?;
            }
        }
        Ok(())
    }

    fn owned(&self, kind: ReferenceKind, held: impl Fn(&Owned) -> bool) -> Vec<String> {
        self.scene
            .owners
            .iter()
            .filter(|entry| entry.kind == kind && held(entry))
            .filter(|entry| !self.planned(kind, &entry.id))
            .map(|entry| entry.id.clone())
            .collect()
    }

    fn planned(&self, kind: ReferenceKind, id: &str) -> bool {
        self.plan
            .assets
            .iter()
            .any(|(held, name)| *held == kind && name == id)
    }

    fn referenced(&self, target: &Target, ignored: Option<&[usize]>) -> bool {
        let (kind, id) = kind_of(target);
        let node_target = matches!(target, Target::Node(_));
        self.scene.nodes.iter().enumerate().any(|(at, node)| {
            !self.plan.nodes.contains(&at)
                && !ignored.is_some_and(|held| held.contains(&at))
                && node.components().any(|(_, component)| {
                    component.references(kind, id)
                        || (node_target && component.references(ReferenceKind::AudioBus, id))
                })
        })
    }

    fn referenced_by(&self, kind: ReferenceKind, id: &str) -> bool {
        self.scene.nodes.iter().enumerate().any(|(at, node)| {
            !self.plan.nodes.contains(&at)
                && node
                    .components()
                    .any(|(_, component)| component.references(kind, id))
        })
    }

    fn instance_removal_nodes(&self, marker: &str, node: &str) -> Result<HashSet<String>, String> {
        let prefab = self
            .scene
            .instance_prefab(marker)
            .expect("a prefab instance names its prefab");
        let loaded = catch_unwind(AssertUnwindSafe(|| {
            Loader::new(self.scene.root.clone(), self.scene.registry_arc()).load(
                &self.scene.root,
                prefab,
                "the instance",
            )
        }))
        .map_err(|payload| {
            format!(
                "the prefab {} loads: {}",
                prefab.display(),
                describe(payload.as_ref()),
            )
        })?;
        let (template, _) = loaded?;
        let index = template
            .node_index(node)
            .ok_or_else(|| format!("the prefab {} holds no node {node}", prefab.display(),))?;
        Ok(template
            .subtree(index)
            .into_iter()
            .map(|at| template.nodes()[at].id.clone())
            .collect())
    }
}

fn kind_of(target: &Target) -> (ReferenceKind, &str) {
    match target {
        Target::Node(id) => (ReferenceKind::Node, id.as_str()),
        Target::Mesh(id) => (ReferenceKind::Mesh, id.as_str()),
        Target::Material(id) => (ReferenceKind::Material, id.as_str()),
        Target::Image(id) => (ReferenceKind::Image, id.as_str()),
        Target::Model(id) => (ReferenceKind::Model, id.as_str()),
        Target::Sound(id) => (ReferenceKind::Sound, id.as_str()),
        Target::Environment(id) => (ReferenceKind::Environment, id.as_str()),
        Target::Graph(id) => (ReferenceKind::Graph, id.as_str()),
    }
}

fn target_of(kind: ReferenceKind, id: &str) -> Target {
    match kind {
        ReferenceKind::Mesh => Target::Mesh(String::from(id)),
        ReferenceKind::Material => Target::Material(String::from(id)),
        ReferenceKind::Image => Target::Image(String::from(id)),
        ReferenceKind::Model => Target::Model(String::from(id)),
        ReferenceKind::Sound => Target::Sound(String::from(id)),
        ReferenceKind::Environment => Target::Environment(String::from(id)),
        ReferenceKind::Graph => Target::Graph(String::from(id)),
        ReferenceKind::Node
        | ReferenceKind::Skin
        | ReferenceKind::Animation
        | ReferenceKind::AudioBus => panic!(
            "an asset target of the {kind} kind exists",
            kind = kind.name()
        ),
    }
}

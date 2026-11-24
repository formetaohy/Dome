use crate::animation::AnimationEntry;
use crate::component::Schema;
use crate::component::{FieldKind, ReferenceKind};
use crate::delta::{Asset, owned};
use crate::edit::Edit;
use crate::graph::GraphEntry;
use crate::node::Node;
use crate::nodes::Nodes;
use crate::scene::Scene;
use crate::skin::SkinEntry;
use crate::value::Value;
use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) const FORMAT: &str = "dome_prefab";

pub(crate) struct Loader {
    root: PathBuf,
    schema: Arc<dyn Schema>,
    stack: Vec<PathBuf>,
    cache: HashMap<PathBuf, Scene>,
}

impl Loader {
    pub(crate) fn new(root: PathBuf, schema: Arc<dyn Schema>) -> Self {
        Self {
            root,
            schema,
            stack: Vec::new(),
            cache: HashMap::new(),
        }
    }

    pub(crate) fn document(&mut self, text: &str, what: &str) -> Scene {
        let mut scene =
            crate::parse::document(text, self.root.clone(), what, Arc::clone(&self.schema));
        self.expand(&mut scene, what);
        scene.validate(what);
        scene
    }

    pub(crate) fn load(&mut self, path: &Path, what: &str) -> Result<(Scene, PathBuf), String> {
        let file = crate::folder::Folder::Prefab.resolve(&self.root, path);
        let key = path.to_path_buf();
        assert!(
            !self.stack.iter().any(|held| held == &key),
            "{what} includes the prefab {} into itself",
            key.display(),
        );
        if let Some(cached) = self.cache.get(&key) {
            return Ok((cached.clone(), key));
        }
        let text = fs::read_to_string(&file)
            .map_err(|error| format!("the prefab {} reads no text: {error}", key.display()))?;
        self.stack.push(key.clone());
        let scene = self.document(&text, &format!("the prefab {}", key.display()));
        self.stack.pop();
        if !scene.is_prefab() {
            return Err(format!(
                "the prefab {} holds the scene format",
                key.display(),
            ));
        }
        self.cache.insert(key.clone(), scene.clone());
        Ok((scene, key))
    }

    fn expand(&mut self, scene: &mut Scene, what: &str) {
        let ids = scene
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        let mut materialization = Materialization {
            what,
            ids,
            nodes: Vec::with_capacity(scene.nodes.len()),
        };
        for node in scene.nodes().iter().cloned().collect::<Vec<_>>() {
            let prefab = node.prefab.clone();
            materialization.nodes.push(node.clone());
            let Some(path) = prefab else {
                continue;
            };
            let (template, key) = self
                .load(&path, what)
                .unwrap_or_else(|error| panic!("{what}: {error}"));
            scene
                .edit(|edit| {
                    import(edit, &template, &key, &node.id)?;
                    edit.set_instance(&node.id, Some(owners(&template, &key)));
                    for (nested, held) in template.instances.iter() {
                        edit.set_instance(&format!("{}_{nested}", node.id), Some(held.clone()));
                    }
                    Ok(())
                })
                .unwrap_or_else(|error| panic!("{what}: {error}"));
            let generated = instantiate(&node, &template, &mut materialization.ids)
                .unwrap_or_else(|error| panic!("{what}: {error}"));
            materialization.nodes.extend(generated);
            materialization.structure(self, scene, &node);
            apply_removals(scene, &node, &mut materialization.nodes, what);
        }
        scene.nodes = Nodes::from(materialization.nodes);
        scene
            .edit(|edit| {
                let candidates = 0..edit.nodes().len();
                apply_overrides(edit, candidates);
                Ok(())
            })
            .unwrap_or_else(|error| panic!("{what}: {error}"));
    }
}

struct Materialization<'a> {
    what: &'a str,
    ids: HashSet<String>,
    nodes: Vec<Node>,
}

impl Materialization<'_> {
    fn structure(&mut self, loader: &mut Loader, scene: &mut Scene, marker: &Node) {
        for addition in marker.additions() {
            self.addition(loader, scene, marker, addition);
        }
    }

    fn addition(&mut self, loader: &mut Loader, scene: &mut Scene, marker: &Node, addition: &Node) {
        let prefix = format!("{}_", marker.id);
        let mut held = addition.clone();
        held.id = format!("{prefix}{}", addition.id);
        assert!(
            self.ids.insert(held.id.clone()),
            "{} materializes the addition {} of {}, which the document holds",
            self.what,
            addition.id,
            marker.id,
        );
        held.parent = Some(match &addition.parent {
            Some(parent) => format!("{prefix}{parent}"),
            None => marker.id.clone(),
        });
        held.owner = Some(marker.id.clone());
        let prefab = held.prefab.clone();
        self.nodes.push(held.clone());
        let Some(path) = prefab else {
            return;
        };
        let (template, key) = loader
            .load(&path, self.what)
            .unwrap_or_else(|error| panic!("{}: {error}", self.what));
        let keys = owners(&template, &key);
        let marker_id = marker.id.clone();
        scene
            .edit(|edit| {
                import(edit, &template, &key, &held.id)?;
                edit.set_instance(&held.id, Some(keys.clone()));
                for (nested, held_keys) in template.instances.iter() {
                    edit.set_instance(&format!("{}_{nested}", held.id), Some(held_keys.clone()));
                }
                if let Some(outer) = edit.instances.get(&marker_id).cloned() {
                    let mut held = outer;
                    held.extend(keys);
                    edit.set_instance(&marker_id, Some(held));
                }
                Ok(())
            })
            .unwrap_or_else(|error| panic!("{}: {error}", self.what));
        let generated = instantiate(&held, &template, &mut self.ids)
            .unwrap_or_else(|error| panic!("{}: {error}", self.what));
        self.nodes.extend(generated);
    }
}

fn apply_removals(scene: &mut Scene, marker: &Node, nodes: &mut Vec<Node>, what: &str) {
    if marker.removals().is_empty() {
        return;
    }
    let prefix = format!("{}_", marker.id);
    let parent = nodes
        .iter()
        .filter_map(|node| {
            node.parent
                .as_deref()
                .map(|parent| (node.id.as_str(), parent))
        })
        .collect::<HashMap<_, _>>();
    let mut targets = Vec::new();
    let mut removed = HashSet::new();
    for removal in marker.removals() {
        let id = format!("{prefix}{removal}");
        if !nodes.iter().any(|node| node.id == id) {
            continue;
        }
        targets.push(id.clone());
        let mut stack = vec![id];
        while let Some(held) = stack.pop() {
            if !removed.insert(held.clone()) {
                continue;
            }
            stack.extend(
                nodes
                    .iter()
                    .filter(|node| node.parent.as_deref() == Some(held.as_str()))
                    .map(|node| node.id.clone()),
            );
        }
    }
    if removed.is_empty() {
        return;
    }
    let mut kept = Vec::new();
    for removal in marker.removals() {
        let id = format!("{prefix}{removal}");
        let subsumed = targets
            .iter()
            .any(|target| target != &id && descendant(&parent, &id, target));
        if targets.contains(&id) && !subsumed {
            kept.push(removal.clone());
        }
    }
    nodes.retain(|node| !removed.contains(&node.id));
    let ids = removed.into_iter().collect::<Vec<_>>();
    scene
        .edit(|edit| {
            for id in &ids {
                edit.set_instance(id, None);
            }
            Ok(())
        })
        .unwrap_or_else(|error| panic!("{what}: {error}"));
    let at = nodes
        .iter()
        .position(|node| node.id == marker.id)
        .unwrap_or_else(|| panic!("{what} removes the prefab instance {}", marker.id));
    let held = &mut nodes[at];
    held.removals = kept;
    held.additions
        .retain(|addition| !ids.contains(&format!("{prefix}{}", addition.id)));
    held.overrides
        .retain(|entry| !ids.contains(&format!("{prefix}{}", entry.node)));
}

fn descendant(parent: &HashMap<&str, &str>, of: &str, ancestor: &str) -> bool {
    let mut at = parent.get(of).copied();
    while let Some(held) = at {
        if held == ancestor {
            return true;
        }
        at = parent.get(held).copied();
    }
    false
}

pub(crate) fn apply_overrides(edit: &mut Edit<'_>, candidates: impl IntoIterator<Item = usize>) {
    let mut markers = candidates
        .into_iter()
        .filter(|index| {
            let node = &edit.nodes()[*index];
            node.prefab().is_some() && !node.overrides().is_empty()
        })
        .map(|index| (depth(edit, index), index))
        .collect::<Vec<_>>();
    markers.sort_by_key(|(depth, index)| (Reverse(*depth), *index));
    markers.dedup_by_key(|(_, index)| *index);
    for (_, marker) in markers {
        let marker_id = edit.nodes()[marker].id.clone();
        let overrides = edit.nodes()[marker].overrides().to_vec();
        for entry in overrides {
            let id = format!("{marker_id}_{}", entry.node);
            let index = edit.node_index(&id).unwrap_or_else(|| {
                panic!(
                    "the prefab instance {marker_id} overrides {} of {}, which it does not hold",
                    entry.path, entry.node,
                )
            });
            edit.apply_override(index, &entry.path, &entry.value)
                .unwrap_or_else(|error| {
                    panic!(
                        "the prefab instance {marker_id} overrides {} of {}: {error}",
                        entry.path, entry.node,
                    )
                });
        }
    }
}

fn depth(scene: &Scene, index: usize) -> usize {
    let mut depth = 0;
    let mut at = Some(index);
    while let Some(current) = at {
        depth += 1;
        at = scene.nodes()[current]
            .owner()
            .and_then(|owner| scene.node_index(owner));
    }
    depth
}

pub(crate) fn instantiate(
    marker: &Node,
    template: &Scene,
    ids: &mut HashSet<String>,
) -> Result<Vec<Node>, String> {
    let prefix = format!("{}_", marker.id);
    let mut nodes = Vec::with_capacity(template.nodes.len());
    for node in template.nodes() {
        let mut held = node.clone();
        held.id = format!("{prefix}{}", node.id);
        if !ids.insert(held.id.clone()) {
            return Err(format!(
                "the prefab instance {} generates the node {}, which the document holds",
                marker.id, held.id,
            ));
        }
        held.parent = match &node.parent {
            Some(parent) => Some(format!("{prefix}{parent}")),
            None => Some(marker.id.clone()),
        };
        held.owner = Some(match &node.owner {
            Some(owner) => format!("{prefix}{owner}"),
            None => marker.id.clone(),
        });
        for component in held.components.values_mut() {
            component.remap(&prefix);
        }
        remap_overrides(template, &mut held, &prefix);
        nodes.push(held);
    }
    Ok(nodes)
}

fn remap_overrides(template: &Scene, node: &mut Node, prefix: &str) {
    for entry in &mut node.overrides {
        let Some((component, rest)) = entry.path.split_once('.') else {
            continue;
        };
        let field = rest.split('.').next().unwrap_or(rest);
        let Some(spec) = template.schema().spec(component) else {
            continue;
        };
        let Some(field) = spec.fields.iter().find(|held| held.name == field) else {
            continue;
        };
        let kind = match field.kind {
            FieldKind::Reference(kind) | FieldKind::OptionalReference(kind) => kind,
            _ => continue,
        };
        if !matches!(
            kind,
            ReferenceKind::Node
                | ReferenceKind::Skin
                | ReferenceKind::Animation
                | ReferenceKind::Graph
                | ReferenceKind::AudioBus
        ) {
            continue;
        }
        let Value::Text(id) = &mut entry.value else {
            continue;
        };
        if !id.is_empty() {
            *id = format!("{prefix}{id}");
        }
    }
}

pub(crate) fn owners(template: &Scene, key: &Path) -> BTreeSet<PathBuf> {
    let mut keys = template
        .owners
        .iter()
        .map(|held| held.prefab.clone())
        .collect::<BTreeSet<_>>();
    keys.insert(key.to_path_buf());
    keys
}

pub(crate) fn import(
    edit: &mut Edit<'_>,
    template: &Scene,
    key: &Path,
    scope: &str,
) -> Result<(), String> {
    let prefix = format!("{scope}_");
    for entry in template.images() {
        let source = String::from(entry.id());
        import_entry(
            edit,
            Import {
                template,
                key,
                kind: ReferenceKind::Image,
                source: &source,
                scope: None,
            },
            entry.clone(),
        )?;
    }
    for entry in template.sounds() {
        let source = String::from(entry.id());
        import_entry(
            edit,
            Import {
                template,
                key,
                kind: ReferenceKind::Sound,
                source: &source,
                scope: None,
            },
            entry.clone(),
        )?;
    }
    for entry in template.environments() {
        let source = String::from(entry.id());
        import_entry(
            edit,
            Import {
                template,
                key,
                kind: ReferenceKind::Environment,
                source: &source,
                scope: None,
            },
            entry.clone(),
        )?;
    }
    for entry in template.meshes() {
        let source = String::from(entry.id());
        import_entry(
            edit,
            Import {
                template,
                key,
                kind: ReferenceKind::Mesh,
                source: &source,
                scope: None,
            },
            entry.clone(),
        )?;
    }
    for entry in template.materials() {
        let source = String::from(entry.id());
        import_entry(
            edit,
            Import {
                template,
                key,
                kind: ReferenceKind::Material,
                source: &source,
                scope: None,
            },
            entry.clone(),
        )?;
    }
    for entry in template.models() {
        let source = String::from(entry.id());
        import_entry(
            edit,
            Import {
                template,
                key,
                kind: ReferenceKind::Model,
                source: &source,
                scope: None,
            },
            entry.clone(),
        )?;
    }
    for entry in template.skins() {
        let source = entry.id.clone();
        let held = SkinEntry::new(format!("{prefix}{source}"), entry.asset.rebased(&prefix));
        import_entry(
            edit,
            Import {
                template,
                key,
                kind: ReferenceKind::Skin,
                source: &source,
                scope: Some(scope),
            },
            held,
        )?;
    }
    for entry in template.animations() {
        let source = entry.id.clone();
        let held = AnimationEntry::new(format!("{prefix}{source}"), entry.asset.rebased(&prefix));
        import_entry(
            edit,
            Import {
                template,
                key,
                kind: ReferenceKind::Animation,
                source: &source,
                scope: Some(scope),
            },
            held,
        )?;
    }
    for entry in template.graphs() {
        let source = entry.id.clone();
        let held = GraphEntry::new(format!("{prefix}{source}"), entry.asset.rebased(&prefix));
        import_entry(
            edit,
            Import {
                template,
                key,
                kind: ReferenceKind::Graph,
                source: &source,
                scope: Some(scope),
            },
            held,
        )?;
    }
    Ok(())
}

struct Import<'a> {
    template: &'a Scene,
    key: &'a Path,
    kind: ReferenceKind,
    source: &'a str,
    scope: Option<&'a str>,
}

fn import_entry<A: Asset>(edit: &mut Edit<'_>, import: Import<'_>, entry: A) -> Result<(), String> {
    let Import {
        template,
        key,
        kind,
        source,
        scope,
    } = import;
    let id = String::from(entry.id());
    let owner = template
        .asset_owner(kind, source)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| key.to_path_buf());
    match edit.asset_owner(kind, &id).map(Path::to_path_buf) {
        Some(held) if held == owner => {
            let at = A::held(edit)
                .iter()
                .position(|held| held.id() == id)
                .expect("an imported asset keeps the document entry it names");
            edit.set_asset(at, entry);
            edit.change_owner(kind, &id, |record| record.scope = scope.map(String::from));
        }
        Some(held) => {
            return Err(format!(
                "the {} {id} of the prefab {} already belongs to {}",
                kind.name(),
                owner.display(),
                held.display(),
            ));
        }
        None => {
            if let Some(at) = A::held(edit).iter().position(|held| held.id() == id) {
                if A::held(edit)[at] == entry {
                    return Ok(());
                }
                return Err(format!(
                    "the {} {id} of the prefab {} differs from the entry of the document",
                    kind.name(),
                    owner.display(),
                ));
            }
            edit.add_asset(entry, owned(kind, id, &owner, scope));
        }
    }
    Ok(())
}

pub(crate) fn describe(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        return String::from(*text);
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    String::from("an unknown panic")
}

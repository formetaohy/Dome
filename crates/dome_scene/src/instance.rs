use crate::edit::Edit;
use crate::node::{InstanceOverride, Node};
use crate::prefab::{Loader, describe};
use crate::scene::Scene;
use crate::target::{Target, valid_id};
use crate::value::{Effect, Value};
use std::collections::{BTreeSet, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

impl Scene {
    pub fn structurally_editable(&self, index: usize) -> bool {
        let node = self
            .nodes
            .get(index)
            .unwrap_or_else(|| panic!("the scene holds no node {index}"));
        match node.owner.as_deref() {
            None => true,
            Some(owner) => self.instance_marker(&node.id) == Some(owner),
        }
    }

    pub fn child_id(&self, parent: &str, base: &str) -> String {
        let prefix = self
            .node_index(parent)
            .filter(|index| self.structurally_editable(*index))
            .and_then(|_| self.instance_marker(parent))
            .map(|marker| format!("{marker}_"));
        let base = match prefix {
            Some(prefix) if !base.starts_with(&prefix) => format!("{prefix}{base}"),
            _ => String::from(base),
        };
        let mut index = 0usize;
        loop {
            let candidate = if index == 0 {
                base.clone()
            } else {
                format!("{base}_{index}")
            };
            if self.node_index(&candidate).is_none() {
                return candidate;
            }
            index += 1;
        }
    }

    pub fn instance_marker(&self, id: &str) -> Option<&str> {
        let index = self.node_index(id)?;
        if self.nodes[index].owner.is_none() {
            return self.nodes[index]
                .prefab
                .is_some()
                .then(|| self.nodes[index].id.as_str());
        }
        let marker = self.instance_marker_index(index)?;
        Some(self.nodes[marker].id.as_str())
    }

    pub fn instance_overrides(&self, marker: &str) -> &[InstanceOverride] {
        self.node_index(marker)
            .map_or(&[], |index| self.nodes[index].overrides.as_slice())
    }

    pub fn instance_additions(&self, marker: &str) -> &[Node] {
        self.node_index(marker)
            .map_or(&[], |index| self.nodes[index].additions.as_slice())
    }

    pub fn instance_removals(&self, marker: &str) -> &[String] {
        self.node_index(marker)
            .map_or(&[], |index| self.nodes[index].removals.as_slice())
    }

    pub fn instance_changes(&self, marker: &str) -> usize {
        let Some(index) = self.node_index(marker) else {
            return 0;
        };
        let node = &self.nodes[index];
        node.overrides.len() + node.additions.len() + node.removals.len()
    }

    pub(crate) fn instance_marker_index(&self, index: usize) -> Option<usize> {
        let mut marker = self.nodes.get(index)?.owner.as_deref()?;
        loop {
            let at = self.node_index(marker)?;
            match self.nodes[at].owner.as_deref() {
                Some(outer) => marker = outer,
                None => return Some(at),
            }
        }
    }
}

impl Edit<'_> {
    pub fn insert_child(&mut self, parent: &str, node: Node) -> Result<Target, String> {
        let parent_index = self
            .node_index(parent)
            .ok_or_else(|| format!("the scene holds no node {parent}"))?;
        assert!(
            node.owner.is_none(),
            "the added node {} belongs to a prefab instance",
            node.id,
        );
        if !node.additions.is_empty() || !node.removals.is_empty() {
            return Err(format!(
                "the added node {} holds the structure of another instance",
                node.id,
            ));
        }
        let instance = if self.nodes[parent_index].owner.is_some() {
            let top = self
                .instance_marker(parent)
                .expect("an instance node belongs to a prefab instance")
                .to_string();
            let owner = self.nodes[parent_index]
                .owner
                .as_deref()
                .expect("an instance node names its owner");
            if owner != top {
                return Err(format!(
                    "the prefab instance {top} keeps the structure of its nested prefab {owner}",
                ));
            }
            Some(top)
        } else if self.nodes[parent_index].prefab.is_some() {
            Some(String::from(parent))
        } else {
            None
        };
        let Some(top) = instance else {
            if !valid_id(&node.id) {
                return Err(format!(
                    "the node id {} holds no letter, digit, _ or -",
                    node.id,
                ));
            }
            if self.node_index(&node.id).is_some() {
                return Err(format!("the scene holds the node {}", node.id));
            }
            let mut node = node;
            node.parent = Some(String::from(parent));
            if node.prefab.is_some() {
                return self.attach_instance(node);
            }
            let id = node.id.clone();
            self.append_nodes(vec![node]);
            return Ok(Target::Node(id));
        };
        let prefix = format!("{top}_");
        let id = if node.id.starts_with(&prefix) {
            node.id.clone()
        } else {
            format!("{prefix}{}", node.id)
        };
        if !valid_id(&id) {
            return Err(format!("the node id {id} holds no letter, digit, _ or -",));
        }
        if self.node_index(&id).is_some() {
            return Err(format!("the scene holds the node {id}"));
        }
        let relative = id
            .strip_prefix(&prefix)
            .expect("an instance node id holds its marker prefix")
            .to_string();
        let marker_index = self
            .node_index(&top)
            .expect("a prefab instance names its marker");
        if self.nodes[marker_index]
            .removals
            .iter()
            .any(|removal| removal == &relative)
        {
            return Err(format!(
                "the prefab instance {top} removes the node {relative}",
            ));
        }
        let parent_relative = if parent == top {
            None
        } else {
            Some(
                parent
                    .strip_prefix(&prefix)
                    .unwrap_or_else(|| {
                        panic!("the node {parent} belongs to the prefab instance {top}")
                    })
                    .to_string(),
            )
        };
        let mut addition = node.clone();
        addition.id = relative;
        addition.parent = parent_relative;
        addition.owner = None;
        let mut placed = addition.clone();
        placed.id = id.clone();
        placed.parent = Some(String::from(parent));
        placed.owner = Some(top.clone());
        let anchor = self
            .subtree(marker_index)
            .into_iter()
            .max()
            .expect("a prefab instance holds its marker")
            + 1;
        let (placed, generated) = if placed.prefab.is_some() {
            self.grow_instance(placed, Some(&top))?
        } else {
            (placed, Vec::new())
        };
        let mut nodes = Vec::with_capacity(generated.len() + 1);
        nodes.push(placed);
        nodes.extend(generated);
        self.insert_nodes(anchor, nodes);
        self.change_node(marker_index, |node, _| {
            node.additions.push(addition);
            Ok(())
        })?;
        let candidates = self.subtree(marker_index);
        crate::prefab::apply_overrides(self, candidates);
        self.validate("an added instance node");
        Ok(Target::Node(id))
    }

    pub(crate) fn attach_instance(&mut self, marker: Node) -> Result<Target, String> {
        assert!(
            marker.prefab.is_some(),
            "the prefab instance {} names its prefab",
            marker.id,
        );
        assert!(
            marker.owner.is_none()
                && marker.overrides.is_empty()
                && marker.additions.is_empty()
                && marker.removals.is_empty(),
            "the prefab instance {} holds its prefab alone",
            marker.id,
        );
        let id = marker.id.clone();
        let (marker, generated) = self.grow_instance(marker, None)?;
        let mut nodes = Vec::with_capacity(generated.len() + 1);
        nodes.push(marker);
        nodes.extend(generated);
        self.append_nodes(nodes);
        self.validate("an added prefab instance");
        Ok(Target::Node(id))
    }

    pub(crate) fn grow_instance(
        &mut self,
        marker: Node,
        outer: Option<&str>,
    ) -> Result<(Node, Vec<Node>), String> {
        let path = marker
            .prefab
            .clone()
            .expect("a prefab instance names its prefab");
        let mut loader = Loader::new(self.root.clone(), Arc::clone(&self.registry));
        let loaded = catch_unwind(AssertUnwindSafe(|| {
            loader.load(&self.root, &path, "the instance")
        }))
        .map_err(|payload| {
            format!(
                "the prefab {} loads: {}",
                path.display(),
                describe(payload.as_ref()),
            )
        })?;
        let (template, key) = loaded?;
        let mut ids = self
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        let keys = crate::prefab::owners(&template, &key);
        let from = crate::prefab::root_of(&self.root, &path);
        let root = self.root.clone();
        crate::prefab::import(self, &template, &key, &from, &root, &marker.id)?;
        self.set_instance(&marker.id, Some(keys.clone()));
        for (nested, held) in template.instances.iter() {
            self.set_instance(&format!("{}_{nested}", marker.id), Some(held.clone()));
        }
        if let Some(outer) = outer {
            let mut held = self
                .instances
                .get(outer)
                .unwrap_or_else(|| panic!("the prefab instance {outer} holds the addition"))
                .clone();
            held.extend(keys);
            self.set_instance(outer, Some(held));
        }
        let generated = crate::prefab::instantiate(&marker, &template, &mut ids, &from, &root)?;
        Ok((marker, generated))
    }

    pub fn unpack(&mut self, marker: &str) -> Result<(), String> {
        let index = self
            .node_index(marker)
            .ok_or_else(|| format!("the scene holds no node {marker}"))?;
        if self.nodes[index].prefab.is_none() {
            return Err(format!("the node {marker} holds no prefab"));
        }
        let owner = self.nodes[index].owner.clone();
        let overrides = self.nodes[index].overrides.clone();
        let keys = self.instances.get(marker).cloned();
        if let Some(keys) = &keys {
            for (other, held) in self.instances.iter() {
                if other == marker {
                    continue;
                }
                let nested = self
                    .node_index(other)
                    .and_then(|at| self.instance_marker_index(at))
                    == Some(index);
                if nested {
                    continue;
                }
                if let Some(shared) = keys.iter().find(|key| held.contains(*key)) {
                    return Err(format!(
                        "the prefab instance {other} keeps the assets of {marker} through {}",
                        shared.display(),
                    ));
                }
            }
        }
        self.set_instance(marker, None);
        let materialized = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.owner.as_deref() == Some(marker))
            .map(|(at, _)| at)
            .collect::<Vec<_>>();
        for at in materialized {
            self.change_node(at, |node, _| {
                node.owner = owner.clone();
                Ok(())
            })?;
        }
        self.change_node(index, |node, _| {
            node.prefab = None;
            node.overrides.clear();
            node.additions.clear();
            node.removals.clear();
            Ok(())
        })?;
        for entry in overrides {
            let target = format!("{marker}_{}", entry.node);
            let target = self.node_index(&target).unwrap_or_else(|| {
                panic!(
                    "the unpacked prefab instance {marker} overrides the node {}, which it does not hold",
                    entry.node,
                )
            });
            if self.nodes[target].owner.is_none() {
                continue;
            }
            self.transfer_override(target, &entry.path, &entry.value)
                .unwrap_or_else(|error| {
                    panic!(
                        "the unpacked prefab instance {marker} overrides {}: {error}",
                        entry.path,
                    )
                });
        }
        match keys {
            Some(keys) => {
                let held = self
                    .instances
                    .values()
                    .flatten()
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let owners = self
                    .owners
                    .iter()
                    .filter(|owned| {
                        owned.scope.as_deref() != Some(marker)
                            && (held.contains(&owned.prefab) || !keys.contains(&owned.prefab))
                    })
                    .cloned()
                    .collect();
                self.set_owners(owners);
            }
            None => {
                let owners = self
                    .owners
                    .iter()
                    .filter(|owned| owned.scope.as_deref() != Some(marker))
                    .cloned()
                    .collect();
                self.set_owners(owners);
            }
        }
        self.validate("an unpacked prefab instance");
        Ok(())
    }

    pub(crate) fn apply_override(
        &mut self,
        index: usize,
        path: &str,
        value: &Value,
    ) -> Result<(), String> {
        if path == "parent" {
            return Err(String::from("an instance keeps the parents of its prefab"));
        }
        self.set_node(index, path, value).map(|_| ())
    }

    pub(crate) fn write_override(
        &mut self,
        index: usize,
        path: &str,
        value: &Value,
    ) -> Result<Effect, String> {
        let marker = self.instance_marker_index(index).ok_or_else(|| {
            format!(
                "the node {} belongs to no prefab instance",
                self.nodes[index].id
            )
        })?;
        if path == "parent" {
            return Err(format!(
                "the prefab instance keeps the parent of {}",
                self.nodes[index].id,
            ));
        }
        let marker_id = self.nodes[marker].id.clone();
        let prefix = format!("{marker_id}_");
        let node = self.nodes[index]
            .id
            .strip_prefix(&prefix)
            .unwrap_or_else(|| {
                panic!(
                    "the node {} belongs to the prefab instance {marker_id}",
                    self.nodes[index].id,
                )
            })
            .to_string();
        if !path.contains('.') {
            let wanted = value.boolean()?;
            let held = self.nodes[index].has_component(path);
            if wanted == held {
                return Ok(Effect::Structure);
            }
            if wanted {
                if self.has_override(marker, &node, path, &Value::Bool(false)) {
                    self.drop_override(marker, &node, path);
                    if self.restore_component(index, marker, &node, path)? {
                        self.reapply_component(index, marker, &node, path)?;
                    } else {
                        self.set_node(index, path, value)?;
                        self.record_override(marker, &node, path, value);
                    }
                    return Ok(Effect::Structure);
                }
            } else {
                let added = self.has_override(marker, &node, path, &Value::Bool(true));
                self.drop_override(marker, &node, path);
                if added {
                    self.drop_component_overrides(marker, &node, path);
                }
                self.set_node(index, path, value)?;
                self.record_override(marker, &node, path, value);
                return Ok(Effect::Structure);
            }
        }
        let effect = self.set_node(index, path, value)?;
        self.record_override(marker, &node, path, value);
        Ok(effect)
    }

    fn record_override(&mut self, marker: usize, node: &str, path: &str, value: &Value) {
        let entry = InstanceOverride {
            node: String::from(node),
            path: String::from(path),
            value: value.clone(),
        };
        self.change_node(marker, |held, _| {
            held.overrides
                .retain(|other| other.node != entry.node || other.path != entry.path);
            held.overrides.push(entry.clone());
            Ok(())
        })
        .expect("a prefab instance records its override");
    }

    pub(crate) fn transfer_override(
        &mut self,
        index: usize,
        path: &str,
        value: &Value,
    ) -> Result<(), String> {
        let marker = self.instance_marker_index(index).ok_or_else(|| {
            format!(
                "the node {} belongs to no prefab instance",
                self.nodes[index].id
            )
        })?;
        if path == "parent" {
            return Err(format!(
                "the prefab instance keeps the parent of {}",
                self.nodes[index].id,
            ));
        }
        let marker_id = self.nodes[marker].id.clone();
        let prefix = format!("{marker_id}_");
        let node = self.nodes[index]
            .id
            .strip_prefix(&prefix)
            .unwrap_or_else(|| {
                panic!(
                    "the node {} belongs to the prefab instance {marker_id}",
                    self.nodes[index].id,
                )
            })
            .to_string();
        self.record_override(marker, &node, path, value);
        Ok(())
    }

    fn has_override(&self, marker: usize, node: &str, path: &str, value: &Value) -> bool {
        self.nodes[marker]
            .overrides
            .iter()
            .any(|entry| entry.node == node && entry.path == path && &entry.value == value)
    }

    fn drop_override(&mut self, marker: usize, node: &str, path: &str) {
        self.change_node(marker, |held, _| {
            held.overrides
                .retain(|entry| entry.node != node || entry.path != path);
            Ok(())
        })
        .expect("a prefab instance drops its override");
    }

    fn drop_component_overrides(&mut self, marker: usize, node: &str, component: &str) {
        let prefix = format!("{component}.");
        self.change_node(marker, |held, _| {
            held.overrides
                .retain(|entry| entry.node != node || !entry.path.starts_with(&prefix));
            Ok(())
        })
        .expect("a prefab instance drops its component overrides");
    }

    fn restore_component(
        &mut self,
        index: usize,
        marker: usize,
        node: &str,
        component: &str,
    ) -> Result<bool, String> {
        let prefab = self.nodes[marker]
            .prefab
            .clone()
            .expect("a prefab instance names its prefab");
        let loaded = catch_unwind(AssertUnwindSafe(|| {
            Loader::new(self.root.clone(), Arc::clone(&self.registry)).load(
                &self.root,
                &prefab,
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
        let source = template
            .nodes()
            .iter()
            .find(|held| held.id == node)
            .ok_or_else(|| format!("the prefab {} holds no node {node}", prefab.display()))?;
        let Some(component) = source.component_by_name(component) else {
            return Ok(false);
        };
        let mut restored = component.clone_box();
        restored.remap(&format!("{}_", self.nodes[marker].id));
        restored.validate(self.scene, &self.nodes[index].id)?;
        self.change_node(index, |held, _| {
            held.insert_component_box(restored);
            Ok(())
        })?;
        Ok(true)
    }

    fn reapply_component(
        &mut self,
        index: usize,
        marker: usize,
        node: &str,
        component: &str,
    ) -> Result<(), String> {
        let prefix = format!("{component}.");
        let entries = self.nodes[marker]
            .overrides
            .iter()
            .filter(|entry| entry.node == node && entry.path.starts_with(&prefix))
            .cloned()
            .collect::<Vec<_>>();
        for entry in entries {
            self.apply_override(index, &entry.path, &entry.value)?;
        }
        Ok(())
    }
}

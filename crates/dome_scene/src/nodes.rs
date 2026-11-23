use crate::node::Node;
use std::collections::HashMap;
use std::ops::Index;
use std::sync::Arc;

pub struct Nodes {
    entries: Vec<Arc<Node>>,
    index: Arc<Lookup>,
}

struct Lookup {
    by_id: HashMap<String, usize>,
    children: HashMap<String, Vec<usize>>,
}

impl Lookup {
    fn build(entries: &[Arc<Node>]) -> Self {
        let mut by_id = HashMap::with_capacity(entries.len());
        let mut children: HashMap<String, Vec<usize>> = HashMap::new();
        for (at, node) in entries.iter().enumerate() {
            by_id.insert(node.id.clone(), at);
            if let Some(parent) = &node.parent {
                children.entry(parent.clone()).or_default().push(at);
            }
        }
        Self { by_id, children }
    }
}

impl Nodes {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            index: Arc::new(Lookup::build(&[])),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Node> {
        self.entries.get(index).map(|held| held.as_ref())
    }

    pub fn first(&self) -> Option<&Node> {
        self.get(0)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Node> {
        self.entries.iter().map(AsRef::as_ref)
    }

    pub(crate) fn index_of(&self, id: &str) -> Option<usize> {
        self.index.by_id.get(id).copied()
    }

    pub(crate) fn children_of(&self, index: usize) -> &[usize] {
        self.entries
            .get(index)
            .and_then(|node| self.index.children.get(&node.id))
            .map_or(&[], Vec::as_slice)
    }

    pub(crate) fn handles(&self) -> &[Arc<Node>] {
        &self.entries
    }

    pub(crate) fn splice(
        &mut self,
        at: usize,
        removed: usize,
        inserted: Vec<Arc<Node>>,
    ) -> Vec<Arc<Node>> {
        assert!(
            at <= self.entries.len() && at + removed <= self.entries.len(),
            "a spliced node range lands inside the scene",
        );
        if removed == 0 && inserted.is_empty() {
            return Vec::new();
        }
        let taken = self
            .entries
            .splice(at..at + removed, inserted)
            .collect::<Vec<_>>();
        self.reindex();
        taken
    }

    pub(crate) fn replace(&mut self, at: usize, node: Arc<Node>) -> Arc<Node> {
        let held = std::mem::replace(&mut self.entries[at], node);
        let held_parent = held.parent.as_deref();
        let held_id = held.id.as_str();
        if held_id != self.entries[at].id || held_parent != self.entries[at].parent.as_deref() {
            self.reindex();
        }
        held
    }

    pub(crate) fn reindex(&mut self) {
        self.index = Arc::new(Lookup::build(&self.entries));
    }
}

impl Clone for Nodes {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            index: Arc::clone(&self.index),
        }
    }
}

impl Default for Nodes {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for Nodes {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self
                .entries
                .iter()
                .zip(&other.entries)
                .all(|(left, right)| Arc::ptr_eq(left, right) || left == right)
    }
}

impl std::fmt::Debug for Nodes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.entries.fmt(formatter)
    }
}

impl Index<usize> for Nodes {
    type Output = Node;

    fn index(&self, index: usize) -> &Self::Output {
        self.entries[index].as_ref()
    }
}

impl From<Vec<Node>> for Nodes {
    fn from(entries: Vec<Node>) -> Self {
        let entries = entries.into_iter().map(Arc::new).collect::<Vec<_>>();
        Self {
            index: Arc::new(Lookup::build(&entries)),
            entries,
        }
    }
}

impl<'a> IntoIterator for &'a Nodes {
    type Item = &'a Node;
    type IntoIter = std::iter::Map<std::slice::Iter<'a, Arc<Node>>, fn(&Arc<Node>) -> &Node>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter().map(|held| held.as_ref())
    }
}

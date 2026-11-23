use crate::schema::ReferenceKind;
use std::collections::{BTreeMap, BTreeSet};

const VISUAL: [ReferenceKind; 8] = [
    ReferenceKind::Image,
    ReferenceKind::Material,
    ReferenceKind::Mesh,
    ReferenceKind::Model,
    ReferenceKind::Skin,
    ReferenceKind::Animation,
    ReferenceKind::Graph,
    ReferenceKind::Environment,
];

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Changed {
    entries: BTreeMap<ReferenceKind, BTreeSet<String>>,
}

impl Changed {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, kind: ReferenceKind, id: &str) {
        self.entries
            .entry(kind)
            .or_default()
            .insert(String::from(id));
    }

    pub fn any(&self) -> bool {
        self.entries.values().any(|ids| !ids.is_empty())
    }

    pub fn holds(&self, kind: ReferenceKind) -> bool {
        self.entries.get(&kind).is_some_and(|ids| !ids.is_empty())
    }

    pub fn ids(&self, kind: ReferenceKind) -> impl Iterator<Item = &str> {
        self.entries
            .get(&kind)
            .into_iter()
            .flatten()
            .map(String::as_str)
    }

    pub fn visual(&self) -> bool {
        VISUAL.iter().any(|kind| self.holds(*kind))
    }

    pub fn touches(&self, kind: ReferenceKind, id: &str) -> bool {
        self.entries.get(&kind).is_some_and(|ids| ids.contains(id))
    }

    pub fn references(&self, held: impl Fn(ReferenceKind, &str) -> bool) -> bool {
        self.entries
            .iter()
            .any(|(kind, ids)| ids.iter().any(|id| held(*kind, id)))
    }
}

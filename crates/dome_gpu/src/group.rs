use crate::buffer::BufferRange;
use crate::pipeline::BindGroup;
use crate::table::{SamplerTable, TextureTable};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

const BINDINGS: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ResourceKey {
    Buffer(BufferRange),
    Texture(usize),
    Sampler(usize),
    TextureTable(usize, u64),
    SamplerTable(usize, u64),
}

impl ResourceKey {
    pub(crate) fn buffer(range: BufferRange) -> Self {
        Self::Buffer(range)
    }

    pub(crate) fn texture(identity: usize) -> Self {
        Self::Texture(identity)
    }

    pub(crate) fn sampler(identity: usize) -> Self {
        Self::Sampler(identity)
    }

    pub(crate) fn textures(table: &TextureTable) -> Self {
        Self::TextureTable(table.identity(), table.generation())
    }

    pub(crate) fn samplers(table: &SamplerTable) -> Self {
        Self::SamplerTable(table.identity(), table.generation())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct GroupKey {
    owner: usize,
    group: u32,
    len: u8,
    bindings: [Option<ResourceKey>; BINDINGS],
}

impl GroupKey {
    pub(crate) fn new(
        owner: usize,
        group: u32,
        bindings: impl Iterator<Item = ResourceKey>,
    ) -> Self {
        let mut held = [None; BINDINGS];
        let mut len = 0usize;
        for key in bindings {
            held[len] = Some(key);
            len += 1;
            assert!(
                len <= BINDINGS,
                "a bind group outruns the {BINDINGS} resources a cached group holds",
            );
        }
        Self {
            owner,
            group,
            len: len as u8,
            bindings: held,
        }
    }

    fn entries(&self) -> &[Option<ResourceKey>] {
        &self.bindings[..self.len as usize]
    }
}

impl PartialEq for GroupKey {
    fn eq(&self, other: &Self) -> bool {
        self.owner == other.owner && self.group == other.group && self.entries() == other.entries()
    }
}

impl Eq for GroupKey {}

impl Hash for GroupKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.owner.hash(state);
        self.group.hash(state);
        self.entries().hash(state);
    }
}

struct Entry {
    submission: u64,
    group: BindGroup,
}

#[derive(Default)]
struct Held {
    groups: HashMap<GroupKey, Entry>,
}

#[derive(Default)]
pub(crate) struct GroupCache {
    held: Mutex<Held>,
}

impl GroupCache {
    pub(crate) fn fetch(
        &self,
        key: GroupKey,
        submission: u64,
        build: impl FnOnce() -> BindGroup,
    ) -> BindGroup {
        let mut held = self
            .held
            .lock()
            .expect("a bind group cache is never poisoned");
        if let Some(entry) = held.groups.get_mut(&key) {
            entry.submission = submission;
            return entry.group.clone();
        }
        drop(held);
        let group = build();
        let mut held = self
            .held
            .lock()
            .expect("a bind group cache is never poisoned");
        let entry = held.groups.entry(key).or_insert(Entry {
            submission,
            group: group.clone(),
        });
        entry.submission = submission;
        entry.group.clone()
    }

    pub(crate) fn retire(&self, completed: u64) {
        self.held
            .lock()
            .expect("a bind group cache is never poisoned")
            .groups
            .retain(|_, entry| entry.submission > completed);
    }
}

use crate::abi::InstanceRecord;
use crate::material::Material;
use crate::mesh::Mesh;
use crate::palette::Skinning;
use dome_ecs::Entity;
use dome_math::{Mat4, Transform, Vec3};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Source {
    pub(crate) transform: Transform,
    pub(crate) mesh: Mesh,
    pub(crate) material: Material,
    pub(crate) tint: Vec3,
    pub(crate) blended: bool,
}

pub(crate) struct Slot {
    pub(crate) entity: Entity,
    pub(crate) epoch: u32,
    pub(crate) staged: Option<Source>,
    pub(crate) skinning: Skinning,
    pub(crate) bounds: Option<(Vec3, f32)>,
    pub(crate) model: Option<Mat4>,
    pub(crate) record: Option<InstanceRecord>,
}

impl Slot {
    pub(crate) fn source(&self) -> Source {
        self.staged
            .expect("a claimed slot stages the instance its entity holds")
    }
}

pub(crate) struct Instances {
    slots: Vec<Slot>,
    lookup: HashMap<Entity, u32>,
    epoch: u32,
    dirty: Vec<u32>,
}

impl Instances {
    pub(crate) fn new() -> Self {
        Self {
            slots: Vec::new(),
            lookup: HashMap::new(),
            epoch: 0,
            dirty: Vec::new(),
        }
    }

    pub(crate) fn begin(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.dirty.clear();
    }

    pub(crate) fn claim(&mut self, entity: Entity) {
        let epoch = self.epoch;
        match self.lookup.get(&entity) {
            Some(index) => {
                let slot = &mut self.slots[*index as usize];
                slot.epoch = epoch;
                slot.staged = None;
            }
            None => {
                let index = self.slots.len() as u32;
                self.slots.push(Slot {
                    entity,
                    epoch,
                    staged: None,
                    skinning: crate::palette::NONE,
                    bounds: None,
                    model: None,
                    record: None,
                });
                self.lookup.insert(entity, index);
            }
        }
    }

    pub(crate) fn settle(&mut self) {
        let mut moved = Vec::new();
        let mut index = 0;
        while index < self.slots.len() {
            if self.slots[index].epoch == self.epoch {
                index += 1;
                continue;
            }
            let removed = self.slots.swap_remove(index);
            self.lookup.remove(&removed.entity);
            if let Some(held) = self.slots.get(index) {
                self.lookup.insert(held.entity, index as u32);
                moved.push(index as u32);
            }
        }
        let len = self.slots.len() as u32;
        self.dirty
            .extend(moved.into_iter().filter(|held| *held < len));
    }

    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    pub(crate) fn slots(&self) -> &[Slot] {
        &self.slots
    }

    #[track_caller]
    pub(crate) fn at(&self, index: usize) -> &Slot {
        assert!(
            index < self.slots.len(),
            "slot {index} of {}",
            self.slots.len()
        );
        &self.slots[index]
    }

    pub(crate) fn at_mut(&mut self, index: usize) -> &mut Slot {
        &mut self.slots[index]
    }

    pub(crate) fn slot_of(&self, entity: Entity) -> Option<u32> {
        self.lookup.get(&entity).copied()
    }

    pub(crate) fn dirty(&self) -> &[u32] {
        &self.dirty
    }

    pub(crate) fn mark(&mut self, index: usize) {
        self.dirty.push(index as u32);
    }

    pub(crate) fn restart(&mut self) {
        self.dirty.clear();
        self.dirty.extend(0..self.slots.len() as u32);
    }
}

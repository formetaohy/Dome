use crate::Entity;

#[derive(Default)]
pub(crate) struct Tree {
    slots: Vec<Slot>,
}

#[derive(Default)]
struct Slot {
    owner: Option<Entity>,
    children: Vec<Entity>,
}

impl Tree {
    pub(crate) fn children(&self, entity: Entity) -> &[Entity] {
        self.slot(entity)
            .map_or(&[], |slot| slot.children.as_slice())
    }

    pub(crate) fn attach(&mut self, child: Entity, parent: Entity) {
        let slot = self.slot_mut(parent);
        assert!(
            !slot.children.contains(&child),
            "entity {} parents entity {} twice",
            child.index(),
            parent.index(),
        );
        slot.children.push(child);
    }

    pub(crate) fn detach(&mut self, child: Entity, parent: Entity) {
        let slot = self.slot_mut(parent);
        let at = slot
            .children
            .iter()
            .position(|held| *held == child)
            .unwrap_or_else(|| {
                panic!(
                    "entity {} parents no entity {}",
                    parent.index(),
                    child.index(),
                )
            });
        slot.children.remove(at);
    }

    pub(crate) fn release(&mut self, entity: Entity) {
        let slot = self.slot_mut(entity);
        slot.children.clear();
        slot.owner = None;
    }

    fn slot(&self, entity: Entity) -> Option<&Slot> {
        let slot = self.slots.get(entity.index() as usize)?;
        (slot.owner == Some(entity)).then_some(slot)
    }

    fn slot_mut(&mut self, entity: Entity) -> &mut Slot {
        let index = entity.index() as usize;
        if self.slots.len() <= index {
            self.slots.resize_with(index + 1, Slot::default);
        }
        let slot = &mut self.slots[index];
        match slot.owner {
            Some(owner) => assert!(
                owner == entity,
                "the hierarchy of entity {} outlived its slot",
                entity.index(),
            ),
            None => slot.owner = Some(entity),
        }
        slot
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug)]
struct Slot {
    generation: u32,
    alive: bool,
}

#[derive(Default)]
pub struct EntityAllocator {
    slots: Vec<Slot>,
    free: Vec<u32>,
    len: usize,
}

impl EntityAllocator {
    pub fn allocate(&mut self) -> Entity {
        self.len += 1;
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.alive = true;
            return Entity {
                index,
                generation: slot.generation,
            };
        }
        let index = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 0,
            alive: true,
        });
        Entity {
            index,
            generation: 0,
        }
    }

    pub fn release(&mut self, entity: Entity) -> bool {
        let Some(slot) = self.slots.get_mut(entity.index as usize) else {
            return false;
        };
        if !slot.alive || slot.generation != entity.generation {
            return false;
        }
        slot.alive = false;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(entity.index);
        self.len -= 1;
        true
    }

    pub fn is_alive(&self, entity: Entity) -> bool {
        self.slots
            .get(entity.index as usize)
            .is_some_and(|slot| slot.alive && slot.generation == entity.generation)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.alive)
            .map(|(index, slot)| Entity {
                index: index as u32,
                generation: slot.generation,
            })
    }
}

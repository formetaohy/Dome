use super::world::children;
use crate::{Entity, LocalTransform, Parent, World};
use dome_math::Transform;

const VACANT: u32 = u32::MAX;

#[derive(Default)]
pub(crate) struct Propagation {
    applied: u64,
    entities: Vec<Entity>,
    locals: Vec<Transform>,
    parents: Vec<Option<Entity>>,
    slots: Vec<u32>,
    touched: Vec<u32>,
    worlds: Vec<Transform>,
    adopted: Vec<bool>,
    stack: Vec<u32>,
}

pub fn propagate(world: &mut World) {
    let mut held = world.remove_resource::<Propagation>().unwrap_or_default();
    held.run(world);
    world.insert_resource(held);
}

impl Propagation {
    fn run(&mut self, world: &mut World) {
        let version = world.hierarchy_change();
        if self.applied == version {
            return;
        }
        self.gather(world);
        self.index();
        self.order(world);
        self.applied = version;
    }

    fn gather(&mut self, world: &World) {
        self.entities.clear();
        self.locals.clear();
        self.parents.clear();
        for (entity, local) in world.iter::<LocalTransform>() {
            self.entities.push(entity);
            self.locals.push(local.0);
            self.parents
                .push(world.get::<Parent>(entity).map(Parent::entity));
        }
    }

    fn index(&mut self) {
        for index in self.touched.drain(..) {
            self.slots[index as usize] = VACANT;
        }
        let top = self
            .entities
            .iter()
            .map(|entity| entity.index())
            .max()
            .unwrap_or(0) as usize;
        self.slots.resize(top + 1, VACANT);
        for (at, entity) in self.entities.iter().enumerate() {
            let index = entity.index();
            self.slots[index as usize] = at as u32;
            self.touched.push(index);
        }
    }

    fn order(&mut self, world: &mut World) {
        let count = self.entities.len();
        self.worlds.clear();
        self.worlds.resize(count, Transform::IDENTITY);
        self.adopted.clear();
        self.adopted.resize(count, false);
        self.stack.clear();
        for at in 0..count {
            if self.parent(self.parents[at], at).is_none() {
                self.stack.push(at as u32);
            }
        }
        while let Some(at) = self.stack.pop() {
            let at = at as usize;
            let held = self.locals[at];
            let world_at = match self.parent(self.parents[at], at) {
                None => held,
                Some(parent) => self.worlds[parent as usize].compose(held),
            };
            self.worlds[at] = world_at;
            self.adopted[at] = true;
            for child in children(world, self.entities[at]) {
                self.stack.push(self.slot(*child, self.entities[at]));
            }
        }
        if let Some(at) = self.adopted.iter().position(|held| !held) {
            panic!(
                "the hierarchy closes the parent chain of entity {} into a cycle",
                self.entities[at].index(),
            );
        }
        for (at, entity) in self.entities.iter().enumerate() {
            world.insert(*entity, self.worlds[at]);
        }
    }

    fn slot(&self, child: Entity, parent: Entity) -> u32 {
        let slot = self
            .slots
            .get(child.index() as usize)
            .copied()
            .unwrap_or(VACANT);
        assert!(
            slot != VACANT && self.entities[slot as usize] == child,
            "the node below entity {} holds no local transform",
            parent.index(),
        );
        slot
    }

    fn parent(&self, parent: Option<Entity>, at: usize) -> Option<u32> {
        let parent = parent?;
        let slot = self
            .slots
            .get(parent.index() as usize)
            .copied()
            .unwrap_or(VACANT);
        assert!(
            slot != VACANT && self.entities[slot as usize] == parent,
            "the hierarchy parents entity {} to entity {}, which holds no local transform",
            self.entities[at].index(),
            parent.index(),
        );
        assert!(
            slot as usize != at,
            "entity {} parents itself",
            self.entities[at].index(),
        );
        Some(slot)
    }
}

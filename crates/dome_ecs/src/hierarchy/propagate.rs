use crate::{Entity, LocalTransform, Parent, World};
use dome_math::Transform;

const VACANT: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviousTransform(pub Transform);

#[derive(Default)]
pub(crate) struct Propagation {
    order: Vec<Entity>,
    rank: Vec<u32>,
    ranked: Vec<u32>,
    dirty: Vec<Entity>,
    pending: Vec<Entity>,
    stamp: Vec<Option<(Entity, u32)>>,
    poses: Vec<(Entity, Transform)>,
    epoch: u32,
    structure: bool,
    all: bool,
    snap: bool,
    walk: Vec<Entity>,
    spread: Vec<(Entity, Transform)>,
}

pub fn propagate(world: &mut World) {
    let mut held = std::mem::take(&mut world.propagation);
    held.snap = true;
    held.run(world);
    world.propagation = held;
}

pub fn propagate_step(world: &mut World) {
    let mut held = std::mem::take(&mut world.propagation);
    held.snap = false;
    held.run(world);
    world.propagation = held;
}

pub fn snapshot(world: &mut World) {
    let mut held = std::mem::take(&mut world.propagation);
    held.remember(world);
    world.propagation = held;
}

impl Propagation {
    pub(crate) fn mark(&mut self, entity: Entity) {
        self.dirty.push(entity);
    }

    pub(crate) fn restructure(&mut self) {
        self.structure = true;
    }

    pub(crate) fn mark_all(&mut self) {
        self.all = true;
    }

    fn remember(&mut self, world: &mut World) {
        self.poses.clear();
        self.poses.extend(
            world
                .iter::<Transform>()
                .map(|(entity, transform)| (entity, *transform)),
        );
        for (entity, transform) in self.poses.drain(..) {
            world.insert(entity, PreviousTransform(transform));
        }
    }

    fn run(&mut self, world: &mut World) {
        if self.structure {
            self.structure = false;
            self.reorder(world);
        }
        let all = std::mem::take(&mut self.all);
        if !all && self.dirty.is_empty() {
            return;
        }
        self.epoch = self.epoch.wrapping_add(1);
        if all {
            for at in 0..self.order.len() {
                let entity = self.order[at];
                let world_transform = local_root(world, entity);
                self.spread(world, entity, world_transform);
            }
            self.dirty.clear();
            return;
        }
        std::mem::swap(&mut self.dirty, &mut self.pending);
        self.pending
            .retain(|entity| world.contains::<LocalTransform>(*entity));
        let rank = &self.rank;
        self.pending.sort_unstable_by_key(|entity| {
            rank.get(entity.index() as usize).copied().unwrap_or(VACANT)
        });
        self.pending.dedup();
        for at in 0..self.pending.len() {
            let entity = self.pending[at];
            let world_transform = local_root(world, entity);
            self.spread(world, entity, world_transform);
        }
        self.pending.clear();
    }

    fn reorder(&mut self, world: &World) {
        for index in self.ranked.drain(..) {
            self.rank[index as usize] = VACANT;
        }
        self.order.clear();
        self.walk.clear();
        for (entity, _) in world.iter::<LocalTransform>() {
            match world.get::<Parent>(entity) {
                Some(parent) => assert!(
                    world.contains::<LocalTransform>(parent.entity()),
                    "the hierarchy parents entity {} to entity {}, which holds no local transform",
                    entity.index(),
                    parent.entity().index(),
                ),
                None => self.walk.push(entity),
            }
        }
        let mut roots = std::mem::take(&mut self.walk);
        while let Some(entity) = roots.pop() {
            let at = self.order.len() as u32;
            self.order.push(entity);
            self.set_rank(entity, at);
            for child in world.tree.children(entity) {
                assert!(
                    world.contains::<LocalTransform>(*child),
                    "the node below entity {} holds no local transform",
                    entity.index(),
                );
                roots.push(*child);
            }
        }
        assert_eq!(
            self.order.len(),
            world.count::<LocalTransform>(),
            "the hierarchy closes the parent chain of an entity into a cycle",
        );
        self.walk = roots;
    }

    fn spread(&mut self, world: &mut World, root: Entity, world_transform: Transform) {
        let epoch = self.epoch;
        self.spread.push((root, world_transform));
        while let Some((entity, held)) = self.spread.pop() {
            if self.stamped(entity, epoch) {
                continue;
            }
            self.stamp(entity, epoch);
            if self.snap {
                world.insert(entity, PreviousTransform(held));
            }
            world.insert(entity, held);
            for child in world.tree.children(entity) {
                if self.stamped(*child, epoch) {
                    continue;
                }
                let local = world.get::<LocalTransform>(*child).unwrap_or_else(|| {
                    panic!(
                        "the node below entity {} holds no local transform",
                        entity.index(),
                    )
                });
                self.spread.push((*child, held.compose(local.0)));
            }
        }
    }

    fn set_rank(&mut self, entity: Entity, at: u32) {
        let index = entity.index() as usize;
        if self.rank.len() <= index {
            self.rank.resize(index + 1, VACANT);
        }
        self.rank[index] = at;
        self.ranked.push(entity.index());
    }

    fn stamped(&self, entity: Entity, epoch: u32) -> bool {
        self.stamp
            .get(entity.index() as usize)
            .is_some_and(|held| *held == Some((entity, epoch)))
    }

    fn stamp(&mut self, entity: Entity, epoch: u32) {
        let index = entity.index() as usize;
        if self.stamp.len() <= index {
            self.stamp.resize(index + 1, None);
        }
        self.stamp[index] = Some((entity, epoch));
    }
}

fn local_root(world: &World, entity: Entity) -> Transform {
    let local = world.get::<LocalTransform>(entity).unwrap_or_else(|| {
        panic!(
            "the node of entity {} holds no local transform",
            entity.index(),
        )
    });
    match world.get::<Parent>(entity) {
        Some(parent) => world_transform(world, parent.entity()).compose(local.0),
        None => local.0,
    }
}

fn world_transform(world: &World, entity: Entity) -> Transform {
    *world.get::<Transform>(entity).unwrap_or_else(|| {
        panic!(
            "the node of entity {} holds no world transform",
            entity.index(),
        )
    })
}

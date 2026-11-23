use crate::body::{Body, under_body};
use crate::tree::Tree;
use crate::{Collider, Joint, RigidBody};
use dome_ecs::{Entity, Parent, World};
use dome_math::Aabb;

const VACANT: u32 = u32::MAX;

#[derive(Clone, Copy)]
struct Slot {
    generation: u32,
    body: u32,
}

pub(crate) struct Table {
    bodies: Vec<Body>,
    slots: Vec<Slot>,
    tree: Tree,
    active: Vec<(u32, Aabb)>,
    pairs: Vec<(u32, u32)>,
    found: Vec<u32>,
    stack: Vec<usize>,
    stale: Vec<Entity>,
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

impl Table {
    pub(crate) fn new() -> Self {
        Self {
            bodies: Vec::new(),
            slots: Vec::new(),
            tree: Tree::new(),
            active: Vec::new(),
            pairs: Vec::new(),
            found: Vec::new(),
            stack: Vec::new(),
            stale: Vec::new(),
        }
    }

    pub(crate) fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    pub(crate) fn bodies_mut(&mut self) -> &mut [Body] {
        &mut self.bodies
    }

    pub(crate) fn index(&self, entity: Entity) -> Option<usize> {
        let slot = self.slots.get(entity.index() as usize)?;
        (slot.generation == entity.generation() && slot.body != VACANT)
            .then_some(slot.body as usize)
    }

    pub(crate) fn sync(&mut self, world: &World, joints: &[(Entity, Joint)], delta: f32) {
        self.prune(world, joints);
        self.discover(world, joints);
        for at in 0..self.bodies.len() {
            if self.bodies[at].refresh(world, delta) {
                self.rebuild(at, world);
            }
        }
    }

    fn prune(&mut self, world: &World, joints: &[(Entity, Joint)]) {
        let mut at = 0;
        while at < self.bodies.len() {
            if self.alive(at, world, joints) {
                at += 1;
            } else {
                self.drop_body(at);
            }
        }
    }

    fn alive(&self, at: usize, world: &World, joints: &[(Entity, Joint)]) -> bool {
        let entity = self.bodies[at].entity;
        world.is_alive(entity)
            && (self.bodies[at].root_held(world)
                || joints.iter().any(|(_, held)| held.other() == entity))
    }

    fn discover(&mut self, world: &World, joints: &[(Entity, Joint)]) {
        for (entity, _) in world.iter::<RigidBody>() {
            match self.index(entity) {
                Some(at) => assert!(
                    self.bodies[at].entity == entity,
                    "the body of entity {} sits under another body",
                    entity.index(),
                ),
                None => {
                    assert!(
                        !under_body(world, entity),
                        "the body of entity {} sits under another body",
                        entity.index(),
                    );
                    self.push(Body::assemble(world, entity));
                }
            }
        }
        for (entity, _) in world.iter::<Collider>() {
            if self.index(entity).is_some() {
                continue;
            }
            let root = root_of(world, entity);
            match self.index(root) {
                Some(at) => self.rebuild(at, world),
                None => self.push(Body::assemble(world, root)),
            }
        }
        for (entity, held) in joints {
            for side in [*entity, held.other()] {
                match self.index(side) {
                    Some(at) => assert!(
                        self.bodies[at].entity == side,
                        "the joint of entity {} names the part of entity {}",
                        entity.index(),
                        side.index(),
                    ),
                    None => {
                        assert!(
                            !under_body(world, side),
                            "the joint of entity {} names the part of entity {}",
                            entity.index(),
                            side.index(),
                        );
                        self.push(Body::assemble(world, side));
                    }
                }
            }
        }
    }

    fn push(&mut self, body: Body) {
        self.bodies.push(body);
        let at = self.bodies.len() - 1;
        self.link(at);
    }

    fn drop_body(&mut self, at: usize) {
        let covered = self.bodies[at].aabb();
        if let Some(proxy) = self.bodies[at].proxy.take() {
            self.tree.remove(proxy);
        }
        if let Some(covered) = covered {
            self.wake_overlap(covered, at);
        }
        self.unlink(at);
        let last = self.bodies.len() - 1;
        self.bodies.swap_remove(at);
        if at != last {
            self.link(at);
            if let Some(proxy) = self.bodies[at].proxy {
                self.tree.reindex(proxy, at as u32);
            }
        }
    }

    fn wake_overlap(&mut self, covered: Aabb, skip: usize) {
        self.tree.query(covered, &mut self.found, &mut self.stack);
        for index in 0..self.found.len() {
            let at = self.found[index] as usize;
            if at != skip && self.bodies[at].sleeping() {
                self.bodies[at].wake();
            }
        }
    }

    fn rebuild(&mut self, at: usize, world: &World) {
        let covered = self.bodies[at].aabb();
        let mut stale = std::mem::take(&mut self.stale);
        stale.clear();
        stale.extend(self.bodies[at].nodes.iter().map(|node| node.entity));
        if let Some(proxy) = self.bodies[at].proxy {
            self.tree.remove(proxy);
            self.bodies[at].proxy = None;
        }
        self.bodies[at].rebuild(world);
        for entity in &stale {
            if self.bodies[at]
                .nodes
                .iter()
                .any(|node| node.entity == *entity)
            {
                continue;
            }
            self.clear(*entity, at);
        }
        self.link(at);
        self.stale = stale;
        let covered = match (covered, self.bodies[at].aabb()) {
            (Some(first), Some(second)) => Some(first.union(second)),
            (first, second) => first.or(second),
        };
        if let Some(covered) = covered {
            self.wake_overlap(covered, at);
        }
    }

    fn link(&mut self, at: usize) {
        let root = self.bodies[at].entity;
        for index in 0..self.bodies[at].nodes.len() {
            let entity = self.bodies[at].nodes[index].entity;
            self.write(entity, at);
        }
        self.write(root, at);
    }

    fn unlink(&mut self, at: usize) {
        self.stale.clear();
        for node in 0..self.bodies[at].nodes.len() {
            self.stale.push(self.bodies[at].nodes[node].entity);
        }
        self.stale.push(self.bodies[at].entity);
        for index in 0..self.stale.len() {
            let entity = self.stale[index];
            self.clear(entity, at);
        }
    }

    fn write(&mut self, entity: Entity, at: usize) {
        let index = entity.index() as usize;
        if self.slots.len() <= index {
            self.slots.resize(
                index + 1,
                Slot {
                    generation: 0,
                    body: VACANT,
                },
            );
        }
        self.slots[index] = Slot {
            generation: entity.generation(),
            body: at as u32,
        };
    }

    fn clear(&mut self, entity: Entity, at: usize) {
        let index = entity.index() as usize;
        if let Some(slot) = self.slots.get_mut(index)
            && slot.generation == entity.generation()
            && slot.body == at as u32
        {
            slot.body = VACANT;
        }
    }

    pub(crate) fn advance(&mut self, delta: f32) {
        self.active.clear();
        for at in 0..self.bodies.len() {
            let swept = self.bodies[at].swept_aabb(delta);
            self.bodies[at].bounds = swept.map(|(aabb, _)| aabb);
            match (swept, self.bodies[at].proxy) {
                (Some((aabb, _)), Some(proxy)) => {
                    let held = self.tree.refresh(proxy, aabb);
                    if held && !self.bodies[at].active() {
                        self.wake_overlap(aabb, at);
                    }
                }
                (Some((aabb, _)), None) => {
                    let proxy = self.tree.insert(aabb, at as u32);
                    self.bodies[at].proxy = Some(proxy);
                }
                (None, Some(proxy)) => {
                    self.tree.remove(proxy);
                    self.bodies[at].proxy = None;
                }
                (None, None) => {}
            }
            if let Some((aabb, _)) = swept
                && (self.bodies[at].active() || self.bodies[at].senses())
            {
                self.active.push((at as u32, aabb));
            }
        }
    }

    pub(crate) fn pairs(&mut self) {
        self.pairs.clear();
        for index in 0..self.active.len() {
            let (at, aabb) = self.active[index];
            self.tree.query(aabb, &mut self.found, &mut self.stack);
            for other in &self.found {
                if *other == at {
                    continue;
                }
                let pair = if *other < at {
                    (*other, at)
                } else {
                    (at, *other)
                };
                self.pairs.push(pair);
            }
        }
        self.pairs.sort_unstable();
        self.pairs.dedup();
    }

    pub(crate) fn split(&mut self) -> (&[Body], &[(u32, u32)]) {
        (&self.bodies, &self.pairs)
    }
}

fn root_of(world: &World, entity: Entity) -> Entity {
    let mut root = entity;
    let mut ancestor = world.get::<Parent>(entity).map(Parent::entity);
    let mut steps = 0;
    while let Some(held) = ancestor {
        steps += 1;
        assert!(
            steps <= world.len(),
            "the node of entity {} closes its hierarchy into a cycle",
            entity.index(),
        );
        if world.contains::<RigidBody>(held)
            || world.contains::<Collider>(held)
            || world.contains::<Joint>(held)
        {
            root = held;
        }
        ancestor = world.get::<Parent>(held).map(Parent::entity);
    }
    root
}

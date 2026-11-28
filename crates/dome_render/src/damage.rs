use crate::capture::CaptureProbe;
use crate::environment::Environment;
use crate::hidden::Hidden;
use dome_ecs::{Entity, World};
use dome_math::{Transform, Vec3};
use std::collections::{HashMap, HashSet};

pub struct SceneDamage {
    stamp: u64,
    nodes: Vec<Entity>,
    sky: bool,
}

impl SceneDamage {
    pub fn new(stamp: u64, nodes: Vec<Entity>, sky: bool) -> Self {
        assert!(
            !nodes.is_empty() || sky,
            "a scene damage names the nodes or the sky it changed",
        );
        Self { stamp, nodes, sky }
    }

    pub fn absorb(&mut self, nodes: impl IntoIterator<Item = Entity>, sky: bool) {
        let nodes = nodes.into_iter().collect::<Vec<_>>();
        assert!(
            !nodes.is_empty() || sky,
            "a scene damage names the nodes or the sky it changed",
        );
        self.stamp = self.stamp.wrapping_add(1);
        self.sky |= sky;
        self.nodes.extend(nodes);
        let mut known = HashSet::with_capacity(self.nodes.len());
        self.nodes.retain(|node| known.insert(*node));
    }

    pub fn stamp(&self) -> u64 {
        self.stamp
    }

    pub fn nodes(&self) -> &[Entity] {
        &self.nodes
    }

    pub fn sky(&self) -> bool {
        self.sky
    }
}

#[derive(Default)]
pub(crate) struct Damage {
    pending: HashMap<Entity, Pending>,
}

#[derive(Default, Clone, Copy)]
struct Pending {
    all: bool,
    changed: bool,
}

impl Damage {
    pub(crate) fn absorb(
        &mut self,
        world: &mut World,
        changed: &[(Entity, Option<(Vec3, f32)>)],
        current: (&[Entity], &[(Vec3, f32)]),
        previous: &[(Entity, (Vec3, f32))],
    ) {
        self.pending.retain(|probe, _| world.is_alive(*probe));
        let damage = world.remove_resource::<SceneDamage>();
        let probes = probes(world);
        let sky = damage.as_ref().is_some_and(SceneDamage::sky);
        if sky {
            for (probe, _, _) in &probes {
                self.pending.entry(*probe).or_default().all = true;
            }
            return;
        }
        let nodes = damage.as_ref().map_or(&[][..], SceneDamage::nodes);
        if probes.is_empty() || (changed.is_empty() && nodes.is_empty()) {
            return;
        }
        let (entities, spheres) = current;
        let current = entities
            .iter()
            .copied()
            .zip(spheres.iter().copied())
            .collect::<HashMap<_, _>>();
        let previous = previous.iter().copied().collect::<HashMap<_, _>>();
        for (entity, sphere) in changed {
            if let Some((center, radius)) = sphere {
                self.mark(&probes, *center, *radius);
            }
            if let Some((center, radius)) = previous.get(entity) {
                self.mark(&probes, *center, *radius);
            }
        }
        for node in nodes {
            for (center, radius) in [current.get(node).copied(), previous.get(node).copied()]
                .into_iter()
                .flatten()
            {
                self.mark(&probes, center, radius);
            }
        }
    }

    fn mark(&mut self, probes: &[(Entity, Transform, f32)], center: Vec3, radius: f32) {
        for (probe, transform, blend) in probes {
            if reaches(*transform, *blend, center, radius) {
                self.pending.entry(*probe).or_default().changed = true;
            }
        }
    }

    pub(crate) fn pending(&self, probe: Entity) -> bool {
        self.pending
            .get(&probe)
            .is_some_and(|held| held.all || held.changed)
    }

    pub(crate) fn covered(&mut self, probe: Entity) {
        self.pending.remove(&probe);
    }
}

fn probes(world: &World) -> Vec<(Entity, Transform, f32)> {
    world
        .iter::<CaptureProbe>()
        .filter(|(entity, _)| !world.contains::<Hidden>(*entity))
        .filter_map(|(entity, _)| {
            let transform = *world.get::<Transform>(entity)?;
            let blend = world.get::<Environment>(entity)?.blend;
            Some((entity, transform, blend))
        })
        .collect()
}

fn reaches(probe: Transform, blend: f32, center: Vec3, radius: f32) -> bool {
    let extent = probe.scale.abs();
    let local = probe
        .rotation
        .normalize()
        .inverse()
        .rotate_vec3(center - probe.translation)
        .abs();
    let outside = (local - extent).max(Vec3::ZERO);
    outside.x.max(outside.y).max(outside.z) <= radius + blend
}

use crate::Joint;
use crate::body::Body;
use dome_ecs::{Entity, World};

pub(crate) fn joints(world: &mut World) -> Vec<(Entity, Joint)> {
    let mut joints = world
        .iter::<Joint>()
        .map(|(entity, joint)| (entity, *joint))
        .collect::<Vec<_>>();
    joints.sort_by_key(|(entity, _)| (entity.index(), entity.generation()));
    let mut stale = Vec::new();
    for (entity, joint) in &joints {
        assert!(
            joint.other() != *entity,
            "the joint of entity {} joins itself",
            entity.index(),
        );
        if !world.is_alive(joint.other()) {
            stale.push(*entity);
        }
    }
    for entity in &stale {
        world.remove::<Joint>(*entity);
    }
    joints.retain(|(entity, _)| world.contains::<Joint>(*entity));
    joints
}

pub(crate) fn drive(
    bodies: &mut [Body],
    pairs: &[(usize, usize)],
    motors: &[bool],
    driven: &mut Vec<bool>,
) {
    driven.clear();
    driven.resize(bodies.len(), false);
    for ((first, second), motor) in pairs.iter().zip(motors) {
        if !motor {
            continue;
        }
        driven[*first] = true;
        driven[*second] = true;
        bodies[*first].wake();
        bodies[*second].wake();
    }
}

pub(crate) fn wake(
    bodies: &mut [Body],
    pairs: &[(usize, usize)],
    adjacency: &mut Vec<Vec<usize>>,
    pending: &mut Vec<usize>,
) {
    adjacency.resize_with(bodies.len(), Vec::new);
    for held in adjacency.iter_mut() {
        held.clear();
    }
    for (first, second) in pairs {
        adjacency[*first].push(*second);
        adjacency[*second].push(*first);
    }
    pending.clear();
    pending.extend(
        bodies
            .iter()
            .enumerate()
            .filter(|(_, body)| body.active())
            .map(|(index, _)| index),
    );
    while let Some(at) = pending.pop() {
        for next in &adjacency[at] {
            if bodies[*next].sleeping() {
                bodies[*next].wake();
                pending.push(*next);
            }
        }
    }
}

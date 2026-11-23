use crate::RigidBody;
use crate::body::{Body, LastPose, Velocity};
use crate::collider::Collider;
use crate::joint::Joint;
use crate::part::Part;
use dome_ecs::{Entity, LocalTransform, Parent, World, children, world_transform};
use dome_math::Transform;
use std::collections::BTreeSet;

pub(crate) fn gather(world: &World, delta: f32) -> Vec<Body> {
    let mut roots = BTreeSet::new();
    for (entity, _) in world.iter::<RigidBody>() {
        assert!(
            !under_body(world, entity),
            "the body of entity {} sits under another body",
            entity.index(),
        );
        roots.insert(entity);
    }
    for (entity, _) in world.iter::<Collider>() {
        if !under_body(world, entity) {
            roots.insert(entity);
        }
    }
    for (entity, joint) in world.iter::<Joint>() {
        assert!(
            !under_body(world, entity),
            "the joint of entity {} sits under another body",
            entity.index(),
        );
        assert!(
            !under_body(world, joint.other()),
            "the joint of entity {} names the part of entity {}",
            entity.index(),
            joint.other().index(),
        );
        roots.insert(entity);
        roots.insert(joint.other());
    }
    roots
        .into_iter()
        .map(|entity| assemble(world, entity, delta))
        .collect()
}

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

pub(crate) fn drive(bodies: &mut [Body], pairs: &[(usize, usize)], motors: &[bool]) -> Vec<bool> {
    let mut driven = vec![false; bodies.len()];
    for ((first, second), motor) in pairs.iter().zip(motors) {
        if !motor {
            continue;
        }
        driven[*first] = true;
        driven[*second] = true;
        bodies[*first].wake();
        bodies[*second].wake();
    }
    driven
}

pub(crate) fn wake(bodies: &mut [Body], pairs: &[(usize, usize)]) {
    let mut adjacency = vec![Vec::new(); bodies.len()];
    for (first, second) in pairs {
        adjacency[*first].push(*second);
        adjacency[*second].push(*first);
    }
    let mut pending = bodies
        .iter()
        .enumerate()
        .filter(|(_, body)| body.active())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    while let Some(at) = pending.pop() {
        for next in &adjacency[at] {
            if bodies[*next].sleeping() {
                bodies[*next].wake();
                pending.push(*next);
            }
        }
    }
}

fn assemble(world: &World, root: Entity, delta: f32) -> Body {
    let rigid = world.get::<RigidBody>(root).copied();
    let frame = world
        .get::<Parent>(root)
        .map(|parent| world_transform(world, parent.entity()));
    let transform = match frame {
        Some(frame) => frame.compose(local_transform(world, root)),
        None => local_transform(world, root),
    };
    let mut parts = Vec::new();
    let mut stack = vec![(root, Transform::IDENTITY, 0usize)];
    while let Some((entity, relative, depth)) = stack.pop() {
        assert!(
            depth <= world.len(),
            "the body of entity {} closes its hierarchy into a cycle",
            root.index(),
        );
        if entity != root {
            assert!(
                !world.contains::<RigidBody>(entity),
                "the body of entity {} holds the body of entity {}",
                root.index(),
                entity.index(),
            );
            assert!(
                !world.contains::<Joint>(entity),
                "the body of entity {} holds the joint of entity {}",
                root.index(),
                entity.index(),
            );
        }
        if let Some(collider) = world.get::<Collider>(entity) {
            parts.push(Part::new(
                entity,
                collider.clone(),
                relative,
                transform.scale,
            ));
        }
        for child in children(world, entity) {
            let local = world.get::<LocalTransform>(*child).unwrap_or_else(|| {
                panic!(
                    "the node of entity {} holds no local transform",
                    child.index(),
                )
            });
            stack.push((*child, relative.compose(local.0), depth + 1));
        }
    }
    Body::new(
        root,
        rigid,
        parts,
        transform,
        frame,
        carried(world, root, transform, delta),
    )
}

fn carried(world: &World, entity: Entity, transform: Transform, delta: f32) -> Velocity {
    match world.get::<LastPose>(entity) {
        Some(previous) => Velocity::of(previous.0, transform, delta),
        None => Velocity::STILL,
    }
}

fn local_transform(world: &World, entity: Entity) -> Transform {
    match world.get::<LocalTransform>(entity) {
        Some(held) => held.0,
        None => {
            assert!(
                world.get::<Parent>(entity).is_none(),
                "the body of entity {} follows a parent but holds no local transform",
                entity.index(),
            );
            world.get::<Transform>(entity).copied().unwrap_or_else(|| {
                panic!(
                    "the body of entity {} holds no local transform",
                    entity.index(),
                )
            })
        }
    }
}

fn under_body(world: &World, entity: Entity) -> bool {
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
            || world.contains::<Joint>(held)
            || world.contains::<Collider>(held)
        {
            return true;
        }
        ancestor = world.get::<Parent>(held).map(Parent::entity);
    }
    false
}

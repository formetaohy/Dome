use crate::AnimationMachine;
use dome_ecs::{LocalTransform, World};

pub fn apply(world: &mut World, machine: &AnimationMachine) {
    for (target, transform) in machine.pose(world) {
        world.insert(target, LocalTransform(transform));
    }
}

pub fn advance(world: &mut World, delta: f32) {
    assert!(delta.is_finite(), "an animation delta of {delta} is finite");
    let entities = world
        .iter::<AnimationMachine>()
        .map(|(entity, _)| entity)
        .collect::<Vec<_>>();
    for entity in entities {
        let Some(mut machine) = world.remove::<AnimationMachine>(entity) else {
            continue;
        };
        if machine.advance(delta) {
            apply(world, &machine);
        }
        world.insert(entity, machine);
    }
}

pub fn settle(world: &mut World) {
    let entities = world
        .iter::<AnimationMachine>()
        .map(|(entity, _)| entity)
        .collect::<Vec<_>>();
    for entity in entities {
        let Some(machine) = world.remove::<AnimationMachine>(entity) else {
            continue;
        };
        apply(world, &machine);
        world.insert(entity, machine);
    }
}

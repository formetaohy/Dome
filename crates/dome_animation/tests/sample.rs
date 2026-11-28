use dome_animation::{
    AnimationGraph, AnimationMachine, Clip, ClipSource, Comparison, ConditionDesc, Interpolation,
    Parameter, ParameterDesc, Property, StateDesc, Track, TransitionDesc, advance,
};
use dome_ecs::{Entity, LocalTransform, World};
use dome_math::Transform;
use std::sync::Arc;

fn clip(start: f32, target: Entity) -> Arc<ClipSource> {
    let track = Track::new(
        "bone",
        Property::Translation,
        Interpolation::Linear,
        vec![0.0, 1.0],
        vec![start, 0.0, 0.0, start + 10.0, 0.0, 0.0],
    );
    Arc::new(ClipSource::new(
        "clip",
        Arc::new(Clip::new(vec![track.expect("the track is valid")])),
        Arc::from([target]),
    ))
}

fn state(id: &str, source: Arc<ClipSource>, looped: bool) -> StateDesc {
    StateDesc {
        id: String::from(id),
        source,
        speed: 1.0,
        looped,
    }
}

fn target_of(world: &World, joint: Entity) -> f32 {
    world
        .get::<LocalTransform>(joint)
        .expect("a sampled pose")
        .0
        .translation
        .x
}

#[test]
fn a_state_samples_its_clip_into_its_targets() {
    let mut world = World::new();
    let joint = world.spawn();
    world.insert(joint, LocalTransform(Transform::IDENTITY));
    let graph = Arc::new(AnimationGraph::new(
        "single",
        Vec::new(),
        vec![state("idle", clip(0.0, joint), true)],
        Vec::new(),
        0,
    ));
    let machine = world.spawn();
    world.insert(machine, AnimationMachine::new(graph, true, 0.0));
    advance(&mut world, 0.5);
    assert!((target_of(&world, joint) - 5.0).abs() < 1e-3);
    advance(&mut world, 0.75);
    assert!((0.0..=10.0).contains(&target_of(&world, joint)));
}

#[test]
fn a_condition_transitions_and_blends_between_states() {
    let mut world = World::new();
    let joint = world.spawn();
    world.insert(joint, LocalTransform(Transform::IDENTITY));
    let graph = Arc::new(AnimationGraph::new(
        "locomotion",
        vec![ParameterDesc {
            name: String::from("speed"),
            default: Parameter::Number(0.0),
        }],
        vec![
            state("idle", clip(0.0, joint), false),
            state("walk", clip(100.0, joint), false),
        ],
        vec![TransitionDesc {
            from: String::from("idle"),
            to: String::from("walk"),
            fade: 1.0,
            exit: 0.0,
            conditions: vec![ConditionDesc {
                parameter: String::from("speed"),
                comparison: Comparison::Above,
                value: Parameter::Number(0.5),
            }],
        }],
        0,
    ));
    let machine = world.spawn();
    world.insert(machine, AnimationMachine::new(graph, true, 0.25));
    advance(&mut world, 0.25);
    assert!((target_of(&world, joint) - 2.5).abs() < 1e-3);
    let mut held = world
        .remove::<AnimationMachine>(machine)
        .expect("a machine");
    held.set_parameter("speed", Parameter::Number(1.0))
        .expect("the graph holds its parameter");
    world.insert(machine, held);
    advance(&mut world, 0.5);
    advance(&mut world, 0.5);
    assert!((target_of(&world, joint) - 57.5).abs() < 1e-3);
    assert_eq!(
        world
            .get::<AnimationMachine>(machine)
            .expect("a machine")
            .state()
            .id(),
        "walk",
    );
    advance(&mut world, 0.5);
    assert!((target_of(&world, joint) - 110.0).abs() < 1e-3);

    let refused = std::panic::catch_unwind(|| {
        AnimationGraph::new(
            "recursive",
            vec![ParameterDesc {
                name: String::from("speed"),
                default: Parameter::Number(0.0),
            }],
            vec![state("idle", clip(0.0, joint), true)],
            vec![TransitionDesc {
                from: String::from("idle"),
                to: String::from("idle"),
                fade: 0.0,
                exit: 0.0,
                conditions: vec![ConditionDesc {
                    parameter: String::from("speed"),
                    comparison: Comparison::Above,
                    value: Parameter::Number(0.0),
                }],
            }],
            0,
        )
    });
    let payload = refused
        .err()
        .expect("a graph refuses the state a transition leaves for itself");
    let message = payload
        .downcast_ref::<String>()
        .expect("a refused transition names the state it leaves");
    assert!(
        message.contains("transitions from idle to itself"),
        "{message}"
    );
}

#[test]
fn a_looping_state_exits_when_a_frame_crosses_its_exit_time() {
    let mut frequent = World::new();
    let machine = looping_machine(&mut frequent);
    for _ in 0..6 {
        advance(&mut frequent, 0.1);
    }
    assert_eq!(state_id(&frequent, machine), "walk");

    let mut dropped = World::new();
    let machine = looping_machine(&mut dropped);
    advance(&mut dropped, 1.25);
    assert_eq!(
        state_id(&dropped, machine),
        "walk",
        "a frame that crosses the exit time still exits after the clip wraps",
    );
}

fn looping_machine(world: &mut World) -> Entity {
    let joint = world.spawn();
    world.insert(joint, LocalTransform(Transform::IDENTITY));
    let graph = Arc::new(AnimationGraph::new(
        "looping",
        Vec::new(),
        vec![
            state("idle", clip(0.0, joint), true),
            state("walk", clip(100.0, joint), true),
        ],
        vec![TransitionDesc {
            from: String::from("idle"),
            to: String::from("walk"),
            fade: 0.0,
            exit: 0.5,
            conditions: Vec::new(),
        }],
        0,
    ));
    let machine = world.spawn();
    world.insert(machine, AnimationMachine::new(graph, true, 0.0));
    machine
}

fn state_id(world: &World, machine: Entity) -> String {
    String::from(
        world
            .get::<AnimationMachine>(machine)
            .expect("a machine")
            .state()
            .id(),
    )
}

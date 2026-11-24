use dome_ecs::{Entity, LocalTransform, World};
use dome_math::{Quat, Transform, Vec3};
use dome_physics::{BodyKind, Collider, Coordinate, Gravity, Joint, RigidBody, Shape, step};

const DELTA: f32 = 1.0 / 60.0;

fn world() -> World {
    let mut world = World::new();
    world.insert_resource(Gravity::default());
    world
}

fn node(world: &mut World, translation: Vec3) -> Entity {
    let entity = world.spawn();
    let transform =
        Transform::from_scale_rotation_translation(Vec3::ONE, Quat::IDENTITY, translation);
    world.insert(entity, LocalTransform(transform));
    world.insert(entity, transform);
    entity
}

fn ball(world: &mut World, translation: Vec3) -> Entity {
    let entity = node(world, translation);
    world.insert(entity, RigidBody::new(BodyKind::Dynamic));
    world.insert(entity, Collider::new(Shape::sphere(0.5)));
    entity
}

fn settle(world: &mut World, steps: usize) {
    for _ in 0..steps {
        step(world, DELTA);
    }
}

fn position(world: &World, entity: Entity) -> Vec3 {
    world
        .get::<Transform>(entity)
        .expect("a simulated body holds its world transform")
        .translation
}

#[test]
fn a_fixed_joint_holds_its_bodies_together() {
    let mut world = world();
    let anchor = node(&mut world, Vec3::new(0.0, 2.0, 0.0));
    let body = ball(&mut world, Vec3::new(0.0, 2.0, 0.0));
    world.insert(
        anchor,
        Joint::fixed(body, Vec3::ZERO, Vec3::ZERO, Quat::IDENTITY, Quat::IDENTITY),
    );
    settle(&mut world, 240);
    let held = position(&world, body);
    assert!(
        held.distance(position(&world, anchor)) < 0.05,
        "a fixed joint holds its body at {held:?}",
    );
    assert!(
        world
            .get::<RigidBody>(body)
            .expect("the jointed body holds its body")
            .is_sleeping(),
        "a body held by a fixed joint falls asleep",
    );
}

#[test]
fn a_moved_static_anchor_drags_its_sleeping_body() {
    let mut world = world();
    let anchor = node(&mut world, Vec3::new(0.0, 3.0, 0.0));
    let body = ball(&mut world, Vec3::new(0.0, 1.0, 0.0));
    world.insert(
        anchor,
        Joint::fixed(
            body,
            Vec3::ZERO,
            Vec3::new(0.0, 2.0, 0.0),
            Quat::IDENTITY,
            Quat::IDENTITY,
        ),
    );
    settle(&mut world, 300);
    assert!(
        world
            .get::<RigidBody>(body)
            .expect("the hanging body holds its body")
            .is_sleeping(),
        "a body hanging from a still anchor falls asleep",
    );
    let mut moved = 0.0;
    for round in 1..=30 {
        moved = 0.05 * round as f32;
        world.insert(
            anchor,
            LocalTransform(Transform::from_translation(Vec3::new(moved, 3.0, 0.0))),
        );
        step(&mut world, DELTA);
    }
    let held = position(&world, body);
    assert!(
        (held.x - moved).abs() < 0.1 && (held.y - 1.0).abs() < 0.1,
        "a sleeping body follows its static anchor to {moved} with the body at {held:?}",
    );
}

#[test]
fn a_hinge_swings_its_body_around_its_axis() {
    let mut world = world();
    let anchor = node(&mut world, Vec3::new(0.0, 3.0, 0.0));
    let body = ball(&mut world, Vec3::new(2.0, 3.0, 0.0));
    world.insert(
        anchor,
        Joint::hinge(
            body,
            Vec3::ZERO,
            Vec3::new(-2.0, 0.0, 0.0),
            Vec3::Z,
            Vec3::Z,
            Quat::IDENTITY,
            Quat::IDENTITY,
        ),
    );
    settle(&mut world, 120);
    let reach = position(&world, body).distance(position(&world, anchor));
    assert!(
        (reach - 2.0).abs() < 0.05,
        "a hinge holds its body at a reach of {reach}",
    );
    assert!(
        position(&world, body).y < 2.9,
        "a hinge lets gravity swing its body to {:?}",
        position(&world, body),
    );
    let joint = *world
        .get::<Joint>(anchor)
        .expect("the anchor holds its joint");
    assert!(
        joint.value(Coordinate::Twist) < -1.0,
        "a hinge reports the twist gravity swings it to at {}",
        joint.value(Coordinate::Twist),
    );
}

#[test]
fn a_fixed_joint_keeps_its_relative_rotation() {
    let mut world = world();
    let anchor = node(&mut world, Vec3::new(0.0, 3.0, 0.0));
    let rotation = Quat::from_rotation_y(0.75);
    let body = node(&mut world, Vec3::new(0.0, 2.0, 0.0));
    let transform =
        Transform::from_scale_rotation_translation(Vec3::ONE, rotation, Vec3::new(0.0, 2.0, 0.0));
    world.insert(body, LocalTransform(transform));
    world.insert(body, transform);
    world.insert(body, RigidBody::new(BodyKind::Dynamic));
    world.insert(body, Collider::new(Shape::sphere(0.5)));
    world.insert(
        anchor,
        Joint::fixed(body, Vec3::ZERO, Vec3::Y, Quat::IDENTITY, rotation),
    );
    settle(&mut world, 120);
    let held = world
        .get::<Transform>(body)
        .expect("the jointed body holds its transform");
    assert!(
        (held.rotation.to_yaw_pitch_roll().x - 0.75).abs() < 0.05,
        "a fixed joint keeps its relative rotation at {:?}",
        held.rotation.to_yaw_pitch_roll(),
    );
}

#[test]
fn a_prismatic_joint_slides_along_its_axis() {
    let mut world = world();
    let anchor = node(&mut world, Vec3::new(0.0, 2.0, 0.0));
    let body = ball(&mut world, Vec3::new(2.0, 2.0, 0.0));
    world.insert(
        anchor,
        Joint::prismatic(
            body,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::X,
            Vec3::X,
            Quat::IDENTITY,
            Quat::IDENTITY,
        ),
    );
    settle(&mut world, 120);
    let held = position(&world, body);
    assert!(
        (held.y - 2.0).abs() < 0.02 && held.z.abs() < 0.02,
        "a prismatic joint keeps its body on its axis at {held:?}",
    );
}

#[test]
fn a_distance_joint_holds_its_span() {
    let mut world = world();
    let anchor = node(&mut world, Vec3::new(0.0, 3.0, 0.0));
    let body = ball(&mut world, Vec3::new(2.0, 3.0, 0.0));
    world.insert(anchor, Joint::distance(body, Vec3::ZERO, Vec3::ZERO, 2.0));
    settle(&mut world, 120);
    let reach = position(&world, body).distance(position(&world, anchor));
    assert!(
        (reach - 2.0).abs() < 0.05,
        "a distance joint holds a span of {reach}",
    );
}

#[test]
fn a_spherical_joint_holds_its_anchor() {
    let mut world = world();
    let anchor = node(&mut world, Vec3::new(0.0, 3.0, 0.0));
    let body = ball(&mut world, Vec3::new(0.0, 3.0, 0.0));
    world.insert(
        anchor,
        Joint::spherical(
            body,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::Y,
            Quat::IDENTITY,
            Quat::IDENTITY,
        ),
    );
    settle(&mut world, 120);
    let held = position(&world, body);
    assert!(
        held.distance(position(&world, anchor)) < 0.05,
        "a spherical joint holds its body at {held:?}",
    );
}

#[test]
fn a_hinge_motor_drives_its_coordinate_into_its_limit() {
    let mut world = world();
    let anchor = node(&mut world, Vec3::new(0.0, 3.0, 0.0));
    let body = ball(&mut world, Vec3::new(2.0, 3.0, 0.0));
    world.insert(
        anchor,
        Joint::hinge(
            body,
            Vec3::ZERO,
            Vec3::new(-2.0, 0.0, 0.0),
            Vec3::Z,
            Vec3::Z,
            Quat::IDENTITY,
            Quat::IDENTITY,
        )
        .with_limit(0.0, 0.5)
        .with_motor(1.0, 100.0),
    );
    settle(&mut world, 180);
    let held = position(&world, body);
    let reach = held.distance(position(&world, anchor));
    assert!(
        (reach - 2.0).abs() < 0.05,
        "a driven hinge holds its body at a reach of {reach}",
    );
    assert!(
        (held.x - 2.0 * 0.5f32.cos()).abs() < 0.05
            && (held.y - (3.0 + 2.0 * 0.5f32.sin())).abs() < 0.05,
        "a hinge motor stops at its limit with its body at {held:?}",
    );
}

#[test]
fn a_spherical_joint_holds_its_swing_and_twist_inside_their_limits() {
    let mut coned = world();
    let anchor = node(&mut coned, Vec3::new(0.0, 3.0, 0.0));
    let body = ball(&mut coned, Vec3::new(2.0, 3.0, 0.0));
    coned.insert(
        anchor,
        Joint::spherical(
            body,
            Vec3::ZERO,
            Vec3::new(-2.0, 0.0, 0.0),
            Vec3::X,
            Quat::IDENTITY,
            Quat::IDENTITY,
        )
        .with_cone(0.3),
    );
    settle(&mut coned, 240);
    let swing = coned
        .get::<Joint>(anchor)
        .expect("the anchor holds its joint")
        .value(Coordinate::Swing);
    assert!(
        (swing - 0.3).abs() < 0.05,
        "a spherical joint holds a swing of {swing}",
    );

    let mut twisted = world();
    let anchor = node(&mut twisted, Vec3::new(0.0, 3.0, 0.0));
    let body = ball(&mut twisted, Vec3::new(2.0, 3.0, 0.0));
    twisted.insert(
        anchor,
        Joint::spherical(
            body,
            Vec3::ZERO,
            Vec3::new(-2.0, 0.0, 0.0),
            Vec3::Z,
            Quat::IDENTITY,
            Quat::IDENTITY,
        )
        .with_limit(-0.3, 0.3),
    );
    settle(&mut twisted, 240);
    let twist = twisted
        .get::<Joint>(anchor)
        .expect("the anchor holds its joint")
        .value(Coordinate::Twist);
    assert!(
        (twist + 0.3).abs() < 0.05,
        "a spherical joint holds a twist of {twist}",
    );
}

#[test]
fn a_limited_distance_joint_spans_its_bounds() {
    let mut world = world();
    let anchor = node(&mut world, Vec3::new(0.0, 3.0, 0.0));
    let body = ball(&mut world, Vec3::new(2.0, 3.0, 0.0));
    world.insert(
        anchor,
        Joint::distance(body, Vec3::ZERO, Vec3::ZERO, 2.0).with_limit(1.0, 3.0),
    );
    settle(&mut world, 240);
    let reach = position(&world, body).distance(position(&world, anchor));
    assert!(
        (reach - 3.0).abs() < 0.05,
        "a limited distance joint spans a reach of {reach}",
    );
}

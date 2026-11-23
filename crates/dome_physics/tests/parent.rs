use dome_ecs::{Entity, LocalTransform, World, set_parent};
use dome_math::{Quat, Transform, Vec3};
use dome_physics::{BodyKind, Collider, CollisionFilter, Gravity, RigidBody, Shape, raycast, step};
use std::f32::consts::FRAC_PI_2;

const DELTA: f32 = 1.0 / 60.0;

fn empty(gravity: Vec3) -> World {
    let mut world = World::new();
    world.insert_resource(Gravity(gravity));
    world
}

fn node(world: &mut World, parent: Option<Entity>, transform: Transform) -> Entity {
    let entity = world.spawn();
    world.insert(entity, LocalTransform(transform));
    if let Some(parent) = parent {
        set_parent(world, entity, parent);
    }
    entity
}

fn floor(world: &mut World) {
    let floor = node(
        world,
        None,
        Transform::from_translation(Vec3::new(0.0, -0.5, 0.0)),
    );
    world.insert(floor, RigidBody::new(BodyKind::Static));
    world.insert(
        floor,
        Collider::new(Shape::cuboid(Vec3::new(20.0, 0.5, 20.0))),
    );
}

fn sphere(world: &mut World, parent: Option<Entity>, local: Transform) -> Entity {
    let body = node(world, parent, local);
    world.insert(body, RigidBody::new(BodyKind::Dynamic));
    world.insert(body, Collider::new(Shape::sphere(0.5)));
    body
}

fn settle(world: &mut World) {
    for _ in 0..240 {
        step(world, DELTA);
    }
}

fn local(world: &World, entity: Entity) -> Transform {
    world
        .get::<LocalTransform>(entity)
        .expect("a simulated node holds its local transform")
        .0
}

#[test]
fn a_movable_body_follows_the_frame_of_its_parent() {
    let mut world = empty(Vec3::new(0.0, -9.81, 0.0));
    floor(&mut world);
    let parent = node(
        &mut world,
        None,
        Transform::from_translation(Vec3::new(2.0, 1.0, 0.0)),
    );
    let body = sphere(
        &mut world,
        Some(parent),
        Transform::from_translation(Vec3::new(0.0, 3.0, 0.0)),
    );
    settle(&mut world);
    let held = world
        .get::<Transform>(body)
        .expect("the body holds its world transform");
    assert!(
        (held.translation.y - 0.5).abs() < 0.02 && (held.translation.x - 2.0).abs() < 0.02,
        "the parented body rests at {:?}",
        held.translation,
    );
    let held = local(&world, body);
    assert!(
        (held.translation.y + 0.5).abs() < 0.02 && held.translation.x.abs() < 0.02,
        "the parented body keeps the local pose {:?}",
        held.translation,
    );
    let parent_transform = Transform::from_translation(Vec3::new(5.0, 1.0, 0.0));
    world.insert(parent, LocalTransform(parent_transform));
    step(&mut world, DELTA);
    let held = world
        .get::<Transform>(body)
        .expect("the body holds its world transform");
    assert!(
        (held.translation.x - 5.0).abs() < 0.02,
        "the body follows its parent to {:?}",
        held.translation,
    );
    assert!(
        local(&world, body).translation.x.abs() < 0.02,
        "the moved parent keeps the body local",
    );
    assert!(
        !world
            .get::<RigidBody>(body)
            .expect("the body holds its state")
            .is_sleeping(),
        "a moving parent wakes its body",
    );
    settle(&mut world);
    assert!(
        world
            .get::<RigidBody>(body)
            .expect("the body holds its state")
            .is_sleeping(),
        "a still parent lets its body sleep",
    );

    let mut turning = empty(Vec3::ZERO);

    let parent = node(
        &mut turning,
        None,
        Transform::from_scale_rotation_translation(
            Vec3::ONE,
            Quat::from_axis_angle(Vec3::Y, FRAC_PI_2),
            Vec3::new(0.0, 2.0, 0.0),
        ),
    );
    let body = sphere(
        &mut turning,
        Some(parent),
        Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)),
    );
    step(&mut turning, DELTA);
    let held = turning
        .get::<Transform>(body)
        .expect("the body holds its turning transform");
    assert!(
        (held.translation - Vec3::new(0.0, 2.0, -2.0)).length() < 0.02,
        "the body turns into the parent frame at {:?}",
        held.translation,
    );
    let held = local(&turning, body);
    assert!(
        (held.translation - Vec3::new(2.0, 0.0, 0.0)).length() < 0.02,
        "the body keeps its local offset {:?}",
        held.translation,
    );
    assert!(
        (held.rotation.dot(Quat::IDENTITY) - 1.0).abs() < 1e-4,
        "the body keeps its local rotation {:?}",
        held.rotation,
    );
    let hit = raycast(
        &turning,
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::new(0.0, 0.0, -1.0),
        10.0,
        CollisionFilter::DEFAULT,
    )
    .expect("a ray meets the parented body");
    assert_eq!(hit.entity, body);
    assert!((hit.distance - 1.5).abs() < 0.02, "a ray stops at {hit:?}",);
}

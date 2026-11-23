use dome_ecs::{Entity, LocalTransform, World, set_parent};
use dome_math::{Quat, Transform, Vec3};
use dome_physics::{BodyKind, Collider, CollisionFilter, Gravity, RigidBody, Shape, raycast, step};

const DELTA: f32 = 1.0 / 60.0;

fn world() -> World {
    let mut world = World::new();
    world.insert_resource(Gravity::default());
    world
}

fn node(world: &mut World, parent: Option<Entity>, translation: Vec3) -> Entity {
    let entity = world.spawn();
    let transform =
        Transform::from_scale_rotation_translation(Vec3::ONE, Quat::IDENTITY, translation);
    world.insert(entity, LocalTransform(transform));
    world.insert(entity, transform);
    if let Some(parent) = parent {
        set_parent(world, entity, parent);
    }
    entity
}

fn floor(world: &mut World) -> Entity {
    let floor = node(world, None, Vec3::new(0.0, -0.5, 0.0));
    world.insert(floor, RigidBody::new(BodyKind::Static));
    world.insert(
        floor,
        Collider::new(Shape::cuboid(Vec3::new(20.0, 0.5, 20.0))),
    );
    floor
}

fn settle(world: &mut World) {
    for _ in 0..300 {
        step(world, DELTA);
    }
}

fn translation(world: &World, entity: Entity) -> Vec3 {
    world
        .get::<Transform>(entity)
        .expect("a simulated node holds its world transform")
        .translation
}

#[test]
fn a_body_composes_the_colliders_below_it() {
    let mut world = world();
    floor(&mut world);
    let root = node(&mut world, None, Vec3::new(0.0, 4.0, 0.0));
    world.insert(root, RigidBody::new(BodyKind::Dynamic));
    let part = node(&mut world, Some(root), Vec3::new(0.0, -1.0, 0.0));
    world.insert(part, Collider::new(Shape::sphere(0.5)));
    settle(&mut world);
    assert!(
        (translation(&world, part).y - 0.5).abs() < 0.02,
        "the part rests on the floor at {:?}",
        translation(&world, part),
    );
    assert!(
        (translation(&world, root).y - 1.5).abs() < 0.02,
        "the body holds its origin a part above the floor at {:?}",
        translation(&world, root),
    );
    let hit = raycast(
        &world,
        Vec3::new(0.0, 3.0, 0.0),
        Vec3::new(0.0, -1.0, 0.0),
        10.0,
        CollisionFilter::DEFAULT,
    )
    .expect("a ray meets the composed collider");
    assert_eq!(hit.entity, part);
    assert!((hit.distance - 2.0).abs() < 0.03, "a ray stops at {hit:?}");
}

#[test]
fn a_body_balances_the_parts_on_both_sides_of_it() {
    let mut world = world();
    floor(&mut world);
    let root = node(&mut world, None, Vec3::new(0.0, 3.0, 0.0));
    world.insert(root, RigidBody::new(BodyKind::Dynamic));
    for side in [-1.0, 1.0] {
        let part = node(&mut world, Some(root), Vec3::new(side, 0.0, 0.0));
        world.insert(part, Collider::new(Shape::sphere(0.5)));
    }
    settle(&mut world);
    let held = translation(&world, root);
    assert!(
        held.x.abs() < 0.05 && (held.y - 0.5).abs() < 0.02,
        "a body balances its two parts at {held:?}",
    );
    let rotation = world
        .get::<Transform>(root)
        .expect("the body holds its world transform")
        .rotation;
    assert!(
        rotation.to_yaw_pitch_roll().z.abs() < 0.05,
        "a balanced body keeps its level at {rotation:?}",
    );
}

#[test]
fn a_compound_body_swings_its_mass_around_a_joint() {
    let mut world = world();
    let anchor = node(&mut world, None, Vec3::new(0.0, 3.0, 0.0));
    let root = node(&mut world, None, Vec3::new(2.0, 3.0, 0.0));
    world.insert(root, RigidBody::new(BodyKind::Dynamic));
    let heavy = node(&mut world, Some(root), Vec3::new(-1.0, 0.0, 0.0));
    world.insert(heavy, Collider::new(Shape::sphere(0.5)));
    let light = node(&mut world, Some(root), Vec3::new(0.0, 0.0, 0.0));
    world.insert(light, Collider::new(Shape::sphere(0.25)));
    world.insert(
        anchor,
        dome_physics::Joint::hinge(
            root,
            Vec3::ZERO,
            Vec3::new(-2.0, 0.0, 0.0),
            Vec3::Z,
            Vec3::Z,
            Quat::IDENTITY,
            Quat::IDENTITY,
        ),
    );
    let volume = |radius: f32| 4.0 / 3.0 * std::f32::consts::PI * radius * radius * radius;
    let center = |world: &World| {
        (translation(world, heavy) * volume(0.5) + translation(world, light) * volume(0.25))
            / (volume(0.5) + volume(0.25))
    };
    step(&mut world, DELTA);
    let start = center(&world);
    settle(&mut world);
    let end = center(&world);
    assert!(
        (start.distance(Vec3::new(0.0, 3.0, 0.0)) - end.distance(Vec3::new(0.0, 3.0, 0.0))).abs()
            < 0.05,
        "a hinge holds the center of mass at a reach of {}",
        end.distance(Vec3::new(0.0, 3.0, 0.0)),
    );
    assert!(
        start.distance(end) > 0.1,
        "a compound body swings its mass from {start:?} to {end:?}",
    );
}

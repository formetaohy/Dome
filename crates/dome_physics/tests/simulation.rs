use dome_ecs::{Entity, LocalTransform, World};
use dome_math::{Quat, Transform, Vec3};
use dome_physics::{BodyKind, Collider, CollisionFilter, Gravity, RigidBody, Shape, raycast, step};

const DELTA: f32 = 1.0 / 60.0;

fn world() -> World {
    let mut world = World::new();
    world.insert_resource(Gravity::default());
    world
}

fn spawn(
    world: &mut World,
    translation: Vec3,
    rotation: Quat,
    shape: Shape,
    kind: BodyKind,
) -> Entity {
    let entity = world.spawn();
    let transform = Transform::from_scale_rotation_translation(Vec3::ONE, rotation, translation);
    world.insert(entity, LocalTransform(transform));
    world.insert(entity, transform);
    world.insert(entity, RigidBody::new(kind));
    world.insert(entity, Collider::new(shape));
    entity
}

fn floor(world: &mut World) -> Entity {
    spawn(
        world,
        Vec3::new(0.0, -0.5, 0.0),
        Quat::IDENTITY,
        Shape::cuboid(Vec3::new(10.0, 0.5, 10.0)),
        BodyKind::Static,
    )
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
fn a_convex_shape_settles_on_a_cuboid() {
    let mut world = world();
    floor(&mut world);
    let sphere = spawn(
        &mut world,
        Vec3::new(-3.0, 4.0, 0.0),
        Quat::IDENTITY,
        Shape::sphere(0.5),
        BodyKind::Dynamic,
    );
    let cube = spawn(
        &mut world,
        Vec3::new(0.0, 3.0, 0.0),
        Quat::IDENTITY,
        Shape::cuboid(Vec3::splat(0.5)),
        BodyKind::Dynamic,
    );
    let capsule = spawn(
        &mut world,
        Vec3::new(3.0, 3.0, 0.0),
        Quat::IDENTITY,
        Shape::capsule(0.25, 0.5),
        BodyKind::Dynamic,
    );
    settle(&mut world, 300);
    let held = position(&world, sphere);
    assert!(
        (held.y - 0.5).abs() < 0.05,
        "a sphere of radius 0.5 rests at {}",
        held.y,
    );
    let held = position(&world, cube);
    assert!(
        (held.y - 0.5).abs() < 0.05,
        "a cube of half extent 0.5 rests at {}",
        held.y,
    );
    let rotation = world
        .get::<Transform>(cube)
        .expect("the cube holds its world transform")
        .rotation;
    assert!(
        rotation.to_yaw_pitch_roll().length() < 0.1,
        "a falling cube lies flat at {rotation:?}",
    );
    let held = position(&world, capsule);
    assert!(
        (held.y - 0.75).abs() < 0.06,
        "a capsule of bottom 0.75 rests at {}",
        held.y,
    );
    assert!(
        world
            .get::<RigidBody>(sphere)
            .expect("the sphere holds a body")
            .is_sleeping(),
        "a resting sphere falls asleep",
    );
}

#[test]
fn two_cuboids_stack() {
    let mut world = world();
    floor(&mut world);
    let lower = spawn(
        &mut world,
        Vec3::new(0.0, 0.5, 0.0),
        Quat::IDENTITY,
        Shape::cuboid(Vec3::splat(0.5)),
        BodyKind::Dynamic,
    );
    let upper = spawn(
        &mut world,
        Vec3::new(0.0, 1.6, 0.0),
        Quat::from_rotation_y(0.6),
        Shape::cuboid(Vec3::splat(0.5)),
        BodyKind::Dynamic,
    );
    settle(&mut world, 600);
    let lower = position(&world, lower).y;
    let held = world
        .get::<Transform>(upper)
        .expect("the upper cube holds its world transform");
    assert!(
        (lower - 0.5).abs() < 0.06 && (held.translation.y - 1.5).abs() < 0.08,
        "a stack of two cubes rests at {lower} and {}",
        held.translation.y,
    );
    assert!(
        held.translation.x.abs() + held.translation.z.abs() < 0.02,
        "a twisted cube stays over the cube below it at {:?}",
        held.translation,
    );
    assert!(
        (held.rotation.to_yaw_pitch_roll().x - 0.6).abs() < 0.01,
        "a twisted cube keeps the yaw it was placed with at {:?}",
        held.rotation.to_yaw_pitch_roll(),
    );
    assert!(
        world
            .get::<RigidBody>(upper)
            .expect("the upper cube holds a body")
            .is_sleeping(),
        "a twisted cube settles instead of creeping",
    );
}

#[test]
fn a_resting_body_wakes_when_its_support_leaves() {
    let mut world = world();
    floor(&mut world);
    let support = spawn(
        &mut world,
        Vec3::new(0.0, 0.5, 0.0),
        Quat::IDENTITY,
        Shape::cuboid(Vec3::splat(0.5)),
        BodyKind::Dynamic,
    );
    let resting = spawn(
        &mut world,
        Vec3::new(0.0, 1.5, 0.0),
        Quat::IDENTITY,
        Shape::cuboid(Vec3::splat(0.5)),
        BodyKind::Dynamic,
    );
    settle(&mut world, 600);
    assert!(
        world
            .get::<RigidBody>(resting)
            .expect("the resting cube holds a body")
            .is_sleeping(),
        "a cube resting on another falls asleep",
    );
    world.despawn(support);
    settle(&mut world, 300);
    let held = position(&world, resting).y;
    assert!(
        (held - 0.5).abs() < 0.05,
        "a cube whose support leaves falls to the floor below instead of hanging at {held}",
    );
}

#[test]
fn a_posed_platform_carries_the_pile_it_slides_under() {
    let mut world = world();
    floor(&mut world);
    let platform = spawn(
        &mut world,
        Vec3::new(0.0, 0.5, 0.0),
        Quat::IDENTITY,
        Shape::cuboid(Vec3::new(2.0, 0.5, 2.0)),
        BodyKind::Kinematic,
    );
    let rider = spawn(
        &mut world,
        Vec3::new(0.0, 1.25, 0.0),
        Quat::IDENTITY,
        Shape::cuboid(Vec3::splat(0.25)),
        BodyKind::Dynamic,
    );
    settle(&mut world, 600);
    assert!(
        world
            .get::<RigidBody>(rider)
            .expect("the rider holds a body")
            .is_sleeping(),
        "a rider on a still platform falls asleep",
    );
    let mut moved = 0.0;
    for round in 0..60 {
        moved = 0.02 * (round + 1) as f32;
        world.insert(
            platform,
            LocalTransform(Transform::from_translation(Vec3::new(moved, 0.5, 0.0))),
        );
        step(&mut world, DELTA);
    }
    let body = world
        .get::<RigidBody>(rider)
        .expect("the rider holds a body");
    assert!(
        !body.is_sleeping(),
        "a platform that slides under a sleeping rider wakes it",
    );
    let held = position(&world, rider);
    assert!(
        held.x > moved * 0.5,
        "a sliding platform carries its rider to {:?} of {moved}",
        held,
    );
}

#[test]
fn friction_stops_a_sliding_cube() {
    let mut world = world();
    floor(&mut world);
    let cube = spawn(
        &mut world,
        Vec3::new(0.0, 0.5, 0.0),
        Quat::IDENTITY,
        Shape::cuboid(Vec3::splat(0.5)),
        BodyKind::Dynamic,
    );
    world
        .get_mut::<Collider>(cube)
        .expect("the cube slides on a collider")
        .set_friction(1.0);
    world
        .get_mut::<RigidBody>(cube)
        .expect("the cube holds a body")
        .set_linear_velocity(Vec3::new(5.0, 0.0, 0.0));
    settle(&mut world, 120);
    let held = world.get::<RigidBody>(cube).expect("the cube holds a body");
    assert!(
        held.linear_velocity().length() < 0.1,
        "a sliding cube of friction 1.0 keeps {}",
        held.linear_velocity().length(),
    );
}

#[test]
fn a_ray_meets_the_nearest_collider() {
    let mut world = world();
    floor(&mut world);
    let sphere = spawn(
        &mut world,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        Shape::sphere(0.5),
        BodyKind::Dynamic,
    );
    let hit = raycast(
        &world,
        Vec3::new(0.0, 5.0, 0.0),
        Vec3::new(0.0, -1.0, 0.0),
        10.0,
        CollisionFilter::DEFAULT,
    )
    .expect("a ray down meets the sphere");
    assert_eq!(hit.entity, sphere);
    assert!(
        (hit.distance - 3.5).abs() < 1e-3,
        "a sphere at 1.0 meets a ray from 5.0 at {}",
        hit.distance,
    );
}

#[test]
fn a_ray_ignores_a_layer_outside_its_filter() {
    let mut world = world();
    let floor = floor(&mut world);
    let sphere = spawn(
        &mut world,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        Shape::sphere(0.5),
        BodyKind::Dynamic,
    );
    let mut collider = world
        .get::<Collider>(sphere)
        .expect("the sphere holds its collider")
        .clone();
    collider.set_filter(CollisionFilter::new(1, 1 << 1));
    world.insert(sphere, collider);
    let origin = Vec3::new(0.0, 5.0, 0.0);
    let direction = Vec3::new(0.0, -1.0, 0.0);
    let missed = raycast(&world, origin, direction, 10.0, CollisionFilter::DEFAULT)
        .expect("a ray down meets the floor below the masked sphere");
    assert_eq!(missed.entity, floor);
    assert!(
        (missed.distance - 5.0).abs() < 1e-3,
        "a ray down meets the floor at {}",
        missed.distance,
    );
    let hit = raycast(
        &world,
        origin,
        direction,
        10.0,
        CollisionFilter::new(1, 1 << 1),
    )
    .expect("a ray of the sphere layer meets the sphere");
    assert_eq!(hit.entity, sphere);
    assert!(
        (hit.distance - 3.5).abs() < 1e-3,
        "a filtered ray meets the sphere at {}",
        hit.distance,
    );
}

use dome_ecs::{Entity, LocalTransform, World};
use dome_math::{Quat, Transform, Vec3};
use dome_physics::{
    BodyKind, Collider, CollisionEvent, CollisionFilter, CollisionPhase, Collisions, Gravity,
    RigidBody, Shape, step,
};

const DELTA: f32 = 1.0 / 60.0;

fn world() -> World {
    let mut world = World::new();
    world.insert_resource(Gravity::default());
    world
}

fn spawn(world: &mut World, translation: Vec3, shape: Shape, kind: BodyKind) -> Entity {
    let entity = world.spawn();
    let transform =
        Transform::from_scale_rotation_translation(Vec3::ONE, Quat::IDENTITY, translation);
    world.insert(entity, LocalTransform(transform));
    world.insert(entity, transform);
    world.insert(entity, RigidBody::new(kind));
    world.insert(entity, Collider::new(shape));
    entity
}

fn events(world: &World) -> Vec<CollisionEvent> {
    world
        .resource::<Collisions>()
        .expect("a stepped world holds its collisions")
        .events()
        .to_vec()
}

fn position(world: &World, entity: Entity) -> Vec3 {
    world
        .get::<Transform>(entity)
        .expect("a simulated body holds its world transform")
        .translation
}

#[test]
fn a_sensor_reports_overlap_without_holding_a_body() {
    let mut world = world();
    let trigger = spawn(
        &mut world,
        Vec3::new(0.0, 1.0, 0.0),
        Shape::cuboid(Vec3::splat(1.0)),
        BodyKind::Static,
    );
    world
        .get_mut::<Collider>(trigger)
        .expect("the trigger holds a collider")
        .set_sensor(true);
    let ball = spawn(
        &mut world,
        Vec3::new(0.0, 1.0, 0.0),
        Shape::sphere(0.5),
        BodyKind::Dynamic,
    );
    let mut begun = false;
    let mut ended = false;
    for _ in 0..240 {
        step(&mut world, DELTA);
        for event in events(&world) {
            assert!(event.sensor, "a sensor pair reports its sensor");
            match event.phase {
                CollisionPhase::Begin => begun = true,
                CollisionPhase::End => {
                    assert!(begun, "an overlap ends after it begins");
                    ended = true;
                }
            }
        }
    }
    assert!(begun, "a sphere inside a sensor reports its overlap");
    assert!(ended, "a falling sphere leaves the sensor below it");
    assert!(
        position(&world, ball).y < -10.0,
        "a sensor holds no falling body at {}",
        position(&world, ball).y,
    );
}

#[test]
fn a_resting_contact_begins_once_and_ends_when_its_body_leaves() {
    let mut world = world();
    let floor = spawn(
        &mut world,
        Vec3::new(0.0, -0.5, 0.0),
        Shape::cuboid(Vec3::new(10.0, 0.5, 10.0)),
        BodyKind::Static,
    );
    let ball = spawn(
        &mut world,
        Vec3::new(0.0, 4.0, 0.0),
        Shape::sphere(0.5),
        BodyKind::Dynamic,
    );
    let mut begun = 0;
    let mut ended = 0;
    for _ in 0..300 {
        step(&mut world, DELTA);
        for event in events(&world) {
            assert!(!event.sensor, "a solid pair reports no sensor");
            assert_eq!((event.first, event.second), (floor, ball));
            match event.phase {
                CollisionPhase::Begin => begun += 1,
                CollisionPhase::End => ended += 1,
            }
        }
    }
    assert_eq!(begun, 1, "a resting sphere begins one contact");
    assert_eq!(ended, 0, "a resting sphere ends no contact");
    world.despawn(ball);
    step(&mut world, DELTA);
    let ended = events(&world)
        .into_iter()
        .filter(|event| event.phase == CollisionPhase::End)
        .count();
    assert_eq!(ended, 1, "a removed body ends the contact it held");
}

#[test]
fn a_collider_ignores_a_layer_outside_its_mask() {
    let mut world = world();
    spawn(
        &mut world,
        Vec3::new(0.0, -0.5, 0.0),
        Shape::cuboid(Vec3::new(10.0, 0.5, 10.0)),
        BodyKind::Static,
    );
    let ball = spawn(
        &mut world,
        Vec3::new(0.0, 4.0, 0.0),
        Shape::sphere(0.5),
        BodyKind::Dynamic,
    );
    set_filter(&mut world, ball, CollisionFilter::new(1, 1 << 1));
    for _ in 0..240 {
        step(&mut world, DELTA);
    }
    assert!(
        position(&world, ball).y < -10.0,
        "a sphere of a masked layer falls through the floor to {}",
        position(&world, ball).y,
    );
}

#[test]
fn a_sensor_ignores_a_layer_outside_its_mask() {
    let mut world = world();
    let sensor = spawn(
        &mut world,
        Vec3::new(0.0, 1.0, 0.0),
        Shape::cuboid(Vec3::splat(1.0)),
        BodyKind::Static,
    );
    let mut collider = world
        .get::<Collider>(sensor)
        .expect("the sensor holds its collider")
        .clone();
    collider.set_sensor(true);
    collider.set_filter(CollisionFilter::new(2, 1 << 2));
    world.insert(sensor, collider);
    let ball = spawn(
        &mut world,
        Vec3::new(0.0, 1.0, 0.0),
        Shape::sphere(0.5),
        BodyKind::Dynamic,
    );
    for _ in 0..240 {
        step(&mut world, DELTA);
        assert!(
            events(&world).is_empty(),
            "a sensor ignores the layer outside its mask",
        );
    }
    assert!(
        position(&world, ball).y < -10.0,
        "a masked sensor holds no falling body at {}",
        position(&world, ball).y,
    );
}

#[test]
fn a_sensor_reports_a_kinematic_body_it_meets() {
    let mut world = world();
    let trigger = spawn(
        &mut world,
        Vec3::new(0.0, 1.0, 0.0),
        Shape::cuboid(Vec3::splat(1.0)),
        BodyKind::Static,
    );
    world
        .get_mut::<Collider>(trigger)
        .expect("the trigger holds a collider")
        .set_sensor(true);
    let gate = spawn(
        &mut world,
        Vec3::new(4.0, 1.0, 0.0),
        Shape::cuboid(Vec3::splat(0.5)),
        BodyKind::Kinematic,
    );
    world
        .get_mut::<RigidBody>(gate)
        .expect("the gate holds a body")
        .set_linear_velocity(Vec3::new(-4.0, 0.0, 0.0));
    let mut begun = false;
    for _ in 0..120 {
        step(&mut world, DELTA);
        begun |= events(&world)
            .iter()
            .any(|event| event.sensor && event.phase == CollisionPhase::Begin);
    }
    assert!(
        begun,
        "a sensor reports the kinematic gate that passes through it",
    );
}

fn set_filter(world: &mut World, entity: Entity, filter: CollisionFilter) {
    let mut collider = world
        .get::<Collider>(entity)
        .expect("the entity holds its collider")
        .clone();
    collider.set_filter(filter);
    world.insert(entity, collider);
}

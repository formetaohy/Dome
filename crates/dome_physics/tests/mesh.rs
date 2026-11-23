use dome_ecs::{Entity, LocalTransform, World};
use dome_math::{Quat, Transform, Vec3};
use dome_physics::{BodyKind, Collider, Gravity, RigidBody, Shape, TriangleMesh, raycast, step};
use std::sync::Arc;

const DELTA: f32 = 1.0 / 60.0;
const FLOOR: [[f32; 3]; 6] = [
    [-10.0, 0.0, -10.0],
    [-10.0, 0.0, 10.0],
    [10.0, 0.0, 10.0],
    [-10.0, 0.0, -10.0],
    [10.0, 0.0, 10.0],
    [10.0, 0.0, -10.0],
];

fn world() -> World {
    let mut world = World::new();
    world.insert_resource(Gravity::default());
    world
}

fn spawn(
    world: &mut World,
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
    shape: Shape,
    kind: BodyKind,
) -> Entity {
    let entity = world.spawn();
    let transform = Transform::from_scale_rotation_translation(scale, rotation, translation);
    world.insert(entity, LocalTransform(transform));
    world.insert(entity, transform);
    world.insert(entity, RigidBody::new(kind));
    world.insert(entity, Collider::new(shape));
    entity
}

fn mesh(positions: &[[f32; 3]]) -> Shape {
    Shape::mesh(Arc::new(TriangleMesh::from_positions(positions)))
}

fn floor(world: &mut World, positions: &[[f32; 3]], scale: Vec3) -> Entity {
    spawn(
        world,
        Vec3::ZERO,
        Quat::IDENTITY,
        scale,
        mesh(positions),
        BodyKind::Static,
    )
}

fn settle(world: &mut World, steps: usize) {
    for _ in 0..steps {
        step(world, DELTA);
    }
}

fn transform(world: &World, entity: Entity) -> Transform {
    *world
        .get::<Transform>(entity)
        .expect("a simulated body holds its world transform")
}

#[test]
fn a_sphere_and_a_capsule_rest_on_a_triangle_mesh_and_a_ray_meets_it() {
    let mut world = world();
    let floor = floor(&mut world, &FLOOR, Vec3::ONE);
    let sphere = spawn(
        &mut world,
        Vec3::new(0.25, 3.0, 0.75),
        Quat::IDENTITY,
        Vec3::ONE,
        Shape::sphere(0.5),
        BodyKind::Dynamic,
    );
    let capsule = spawn(
        &mut world,
        Vec3::new(-2.0, 3.0, 0.0),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        Vec3::ONE,
        Shape::capsule(0.25, 0.5),
        BodyKind::Dynamic,
    );
    settle(&mut world, 240);
    let held = transform(&world, sphere).translation;
    assert!(
        (held.y - 0.5).abs() < 0.05,
        "a sphere rests on a triangle mesh at {held:?}",
    );
    assert!(
        world
            .get::<RigidBody>(sphere)
            .expect("the sphere holds its body")
            .is_sleeping(),
        "a sphere resting on a triangle mesh falls asleep",
    );
    let held = transform(&world, capsule).translation;
    assert!(
        (held.y - 0.25).abs() < 0.05,
        "a capsule rests on a triangle mesh at {held:?}",
    );
    let hit = raycast(
        &world,
        Vec3::new(2.0, 5.0, 2.0),
        Vec3::new(0.0, -1.0, 0.0),
        10.0,
        dome_physics::CollisionFilter::DEFAULT,
    )
    .expect("a ray meets the triangle mesh");
    assert_eq!(hit.entity, floor);
    assert!((hit.distance - 5.0).abs() < 1e-3, "a ray stops at {hit:?}");
    assert!(
        (hit.normal - Vec3::Y).length() < 1e-3,
        "a triangle mesh faces its ray with {hit:?}",
    );
}

#[test]
fn a_cuboid_rests_flat_on_a_triangle_mesh() {
    let mut world = world();
    floor(&mut world, &FLOOR, Vec3::ONE);
    let cuboid = spawn(
        &mut world,
        Vec3::new(0.0, 3.0, 0.0),
        Quat::IDENTITY,
        Vec3::ONE,
        Shape::cuboid(Vec3::splat(0.5)),
        BodyKind::Dynamic,
    );
    settle(&mut world, 300);
    let held = transform(&world, cuboid);
    assert!(
        (held.translation.y - 0.5).abs() < 0.05,
        "a cuboid rests on a triangle mesh at {:?}",
        held.translation,
    );
    let rotation = held.rotation.to_yaw_pitch_roll();
    assert!(
        rotation.x.abs() + rotation.y.abs() + rotation.z.abs() < 0.05,
        "a cuboid rests flat on a triangle mesh at {rotation:?}",
    );
}

#[test]
fn a_scaled_mesh_collides_at_its_world_scale() {
    let mut world = world();
    let raised = FLOOR.map(|[x, y, z]| [x, y + 0.5, z]);
    floor(&mut world, &raised, Vec3::splat(2.0));
    let sphere = spawn(
        &mut world,
        Vec3::new(0.0, 5.0, 0.0),
        Quat::IDENTITY,
        Vec3::ONE,
        Shape::sphere(0.5),
        BodyKind::Dynamic,
    );
    settle(&mut world, 240);
    let held = transform(&world, sphere).translation;
    assert!(
        (held.y - 1.5).abs() < 0.05,
        "a sphere rests on a scaled triangle mesh at {held:?}",
    );
    let hit = raycast(
        &world,
        Vec3::new(2.0, 5.0, 2.0),
        Vec3::new(0.0, -1.0, 0.0),
        10.0,
        dome_physics::CollisionFilter::DEFAULT,
    )
    .expect("a ray meets the scaled triangle mesh");
    assert!((hit.distance - 4.0).abs() < 0.1, "a ray stops at {hit:?}");
}

#[test]
#[should_panic(expected = "carries no volume")]
fn a_dynamic_body_holds_no_mesh_collider() {
    let mut world = world();
    spawn(
        &mut world,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::ONE,
        mesh(&FLOOR),
        BodyKind::Dynamic,
    );
    step(&mut world, DELTA);
}

#[test]
fn a_mesh_sensor_reports_an_overlap_whichever_side_holds_it() {
    let mut world = world();
    let sphere = spawn(
        &mut world,
        Vec3::new(0.0, 0.3, 0.0),
        Quat::IDENTITY,
        Vec3::ONE,
        Shape::sphere(0.5),
        BodyKind::Dynamic,
    );
    let mesh = floor(&mut world, &FLOOR, Vec3::ONE);
    world
        .get_mut::<Collider>(mesh)
        .expect("the mesh holds its collider")
        .set_sensor(true);
    step(&mut world, DELTA);
    let collisions = world
        .resource::<dome_physics::Collisions>()
        .expect("a stepped world holds its collisions");
    assert!(
        collisions.events().iter().any(|event| {
            event.sensor
                && event.phase == dome_physics::CollisionPhase::Begin
                && event.first == sphere
                && event.second == mesh
        }),
        "a mesh sensor reports its overlap",
    );
}

#[test]
fn a_rotated_mesh_collides_and_casts_its_ray() {
    let mut world = world();
    let mesh = spawn(
        &mut world,
        Vec3::ZERO,
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        Vec3::ONE,
        mesh(&FLOOR),
        BodyKind::Static,
    );
    let sphere = spawn(
        &mut world,
        Vec3::new(2.0, 0.0, 0.0),
        Quat::IDENTITY,
        Vec3::ONE,
        Shape::sphere(0.5),
        BodyKind::Dynamic,
    );
    world
        .get_mut::<RigidBody>(sphere)
        .expect("the sphere holds its body")
        .set_gravity_scale(0.0);
    world
        .get_mut::<RigidBody>(sphere)
        .expect("the sphere holds its body")
        .set_linear_velocity(Vec3::new(-2.0, 0.0, 0.0));
    settle(&mut world, 120);
    let held = transform(&world, sphere).translation;
    assert!(
        (held.x - 0.5).abs() < 0.05,
        "a sphere stops at the rotated triangle mesh at {held:?}",
    );
    let hit = raycast(
        &world,
        Vec3::new(5.0, 0.0, 4.0),
        Vec3::new(-1.0, 0.0, 0.0),
        10.0,
        dome_physics::CollisionFilter::DEFAULT,
    )
    .expect("a ray meets the rotated triangle mesh");
    assert_eq!(hit.entity, mesh);
    assert!((hit.distance - 5.0).abs() < 0.05, "a ray stops at {hit:?}");
    assert!(
        (hit.normal - Vec3::X).length() < 1e-3,
        "a rotated triangle mesh faces its ray with {hit:?}",
    );
}

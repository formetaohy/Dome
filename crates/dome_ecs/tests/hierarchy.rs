use dome_ecs::{
    LocalTransform, PreviousTransform, World, children, propagate, propagate_step, set_parent,
    snapshot,
};
use dome_math::{Quat, Transform, Vec3};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|held| held.set(held.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let _ = ALLOCATIONS.try_with(|held| held.set(held.get().saturating_sub(1)));
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations() -> usize {
    ALLOCATIONS.with(Cell::get)
}

#[test]
fn a_parent_carries_its_child_into_world_space_and_takes_it_down() {
    let mut world = World::new();
    let root = world.spawn();
    world.insert(
        root,
        LocalTransform(Transform::from_scale_rotation_translation(
            Vec3::splat(2.0),
            Quat::IDENTITY,
            Vec3::new(1.0, 2.0, 3.0),
        )),
    );
    let child = world.spawn();
    world.insert(
        child,
        LocalTransform(Transform::from_translation(Vec3::new(1.0, 0.0, 0.0))),
    );
    set_parent(&mut world, child, root);
    assert_eq!(children(&world, root), [child]);
    propagate(&mut world);
    let held = *world
        .get::<Transform>(child)
        .expect("the child holds its world transform");
    assert_eq!(held.translation, Vec3::new(3.0, 2.0, 3.0));
    assert_eq!(held.scale, Vec3::splat(2.0));
    assert!(world.despawn(root), "a live root goes down");
    assert!(!world.is_alive(child), "a child goes down with its parent");
    propagate(&mut world);
    assert_eq!(world.len(), 0);
}

#[test]
fn a_settled_hierarchy_propagates_without_touching_the_heap() {
    let mut world = World::new();
    let root = world.spawn();
    world.insert(root, LocalTransform(Transform::IDENTITY));
    let mut deepest = root;
    for index in 0..64 {
        let child = world.spawn();
        world.insert(
            child,
            LocalTransform(Transform::from_translation(Vec3::new(
                index as f32,
                0.0,
                0.0,
            ))),
        );
        set_parent(&mut world, child, deepest);
        deepest = child;
    }
    propagate(&mut world);
    for round in 0..2 {
        world.insert(
            root,
            LocalTransform(Transform::from_translation(Vec3::new(
                round as f32,
                1.0,
                0.0,
            ))),
        );
        let before = allocations();
        propagate(&mut world);
        assert_eq!(
            allocations(),
            before,
            "a settled hierarchy of 65 nodes allocates while it propagates",
        );
    }
    let held = *world
        .get::<Transform>(deepest)
        .expect("the deepest node holds its world transform");
    assert_eq!(held.translation, Vec3::new(2017.0, 1.0, 0.0));
}

#[test]
fn a_stepped_pose_keeps_its_predecessor_and_an_immediate_pose_snaps_it() {
    let mut world = World::new();
    let entity = world.spawn();
    world.insert(
        entity,
        LocalTransform(Transform::from_translation(Vec3::new(1.0, 0.0, 0.0))),
    );
    propagate(&mut world);
    assert_eq!(
        pose(&world, entity),
        Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
    );

    world.insert(
        entity,
        LocalTransform(Transform::from_translation(Vec3::new(2.0, 0.0, 0.0))),
    );
    snapshot(&mut world);
    propagate_step(&mut world);
    assert_eq!(
        *world
            .get::<Transform>(entity)
            .expect("a stepped entity holds its world transform"),
        Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)),
    );
    assert_eq!(
        pose(&world, entity),
        Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
        "a stepped pose keeps the pose its step started from",
    );

    world.insert(
        entity,
        LocalTransform(Transform::from_translation(Vec3::new(3.0, 0.0, 0.0))),
    );
    propagate(&mut world);
    assert_eq!(
        pose(&world, entity),
        Transform::from_translation(Vec3::new(3.0, 0.0, 0.0)),
        "an immediate pose carries no pose to interpolate from",
    );
}

fn pose(world: &World, entity: dome_ecs::Entity) -> Transform {
    world
        .get::<PreviousTransform>(entity)
        .expect("a published entity holds the pose it came from")
        .0
}

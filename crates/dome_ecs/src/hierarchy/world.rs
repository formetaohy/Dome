use crate::{Entity, LocalTransform, Parent, World};
use dome_math::Transform;

pub fn world_transform(world: &World, entity: Entity) -> Transform {
    resolve(world, entity, 0)
}

fn resolve(world: &World, entity: Entity, depth: usize) -> Transform {
    assert!(
        depth <= world.len(),
        "the node of entity {} closes its parent chain into a cycle",
        entity.index(),
    );
    let local = world.get::<LocalTransform>(entity).unwrap_or_else(|| {
        panic!(
            "the node of entity {} holds no local transform",
            entity.index(),
        )
    });
    match world.get::<Parent>(entity) {
        Some(parent) => resolve(world, parent.entity(), depth + 1).compose(local.0),
        None => local.0,
    }
}

pub fn children(world: &World, entity: Entity) -> &[Entity] {
    world.tree.children(entity)
}

pub fn set_parent(world: &mut World, child: Entity, parent: Entity) {
    assert!(
        world.is_alive(child) && world.is_alive(parent),
        "a parent joins two live entities",
    );
    assert!(child != parent, "entity {} parents itself", child.index(),);
    let mut ancestor = Some(parent);
    let mut steps = 0;
    while let Some(held) = ancestor {
        assert!(
            held != child,
            "the hierarchy closes the parent chain of entity {} into a cycle",
            child.index(),
        );
        steps += 1;
        assert!(
            steps <= world.len(),
            "the node of entity {} closes its parent chain into a cycle",
            child.index(),
        );
        ancestor = world.get::<Parent>(held).map(Parent::entity);
    }
    let previous = world.get::<Parent>(child).map(Parent::entity);
    if previous == Some(parent) {
        return;
    }
    if let Some(previous) = previous {
        world.tree.detach(child, previous);
    }
    world.tree.attach(child, parent);
    world.store(child, Parent::new(parent));
    world.bump_hierarchy();
}

pub fn clear_parent(world: &mut World, child: Entity) -> bool {
    let Some(previous) = world.get::<Parent>(child).map(Parent::entity) else {
        return false;
    };
    world.tree.detach(child, previous);
    world.drop_component::<Parent>(child);
    world.bump_hierarchy();
    true
}

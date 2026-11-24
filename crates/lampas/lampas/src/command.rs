use dome_ecs::{Entity, World};

pub enum Command {
    Add {
        entity: Entity,
        component: u16,
        fields: Vec<dome_ecs::Value>,
    },
    Remove {
        entity: Entity,
        component: u16,
    },
    Despawn(Entity),
}

pub fn apply(commands: &mut Vec<Command>, world: &mut World) {
    for command in commands.drain(..) {
        match command {
            Command::Add {
                entity,
                component,
                fields,
            } => {
                assert!(
                    world.is_alive(entity),
                    "a lampas command adds a component to the dead entity {}",
                    entity.index(),
                );
                world.dynamic_mut().insert(entity, component, &fields);
            }
            Command::Remove { entity, component } => {
                assert!(
                    world.is_alive(entity),
                    "a lampas command removes a component from the dead entity {}",
                    entity.index(),
                );
                assert!(
                    world.dynamic_mut().remove(entity, component),
                    "a lampas command removes a component the entity {} holds none of",
                    entity.index(),
                );
            }
            Command::Despawn(entity) => {
                assert!(
                    world.despawn(entity),
                    "a lampas command despawns the dead entity {}",
                    entity.index(),
                );
            }
        }
    }
}

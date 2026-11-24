use crate::store::Store;
use dome_ecs::{Entity, World};
use lampas_vm::Value;

pub enum Command {
    Add {
        entity: Entity,
        component: u16,
        fields: Vec<Value>,
    },
    Remove {
        entity: Entity,
        component: u16,
    },
    Despawn(Entity),
}

pub fn apply(commands: &mut Vec<Command>, store: &mut Store, world: &mut World) {
    for command in commands.drain(..) {
        match command {
            Command::Add {
                entity,
                component,
                fields,
            } => {
                assert!(
                    world.is_alive(entity),
                    "a lampas command adds {} to the dead entity {}",
                    component,
                    entity.index(),
                );
                store.insert(entity, component, &fields);
            }
            Command::Remove { entity, component } => {
                assert!(
                    world.is_alive(entity),
                    "a lampas command removes {} from the dead entity {}",
                    component,
                    entity.index(),
                );
                store.remove(entity, component);
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

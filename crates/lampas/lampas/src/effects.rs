use dome_ecs::{Entity, World, children};
use lampas_vm::Trap;
use std::collections::HashMap;

enum Command {
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

enum Undo {
    Spawned(Entity),
    Field {
        row: u32,
        component: u16,
        field: u16,
        previous: dome_ecs::Value,
    },
    Edited(Box<dyn FnOnce(&mut World)>),
}

pub(crate) struct Effects {
    undo: Vec<Undo>,
    commands: Vec<Command>,
    dead: HashMap<Entity, ()>,
    present: HashMap<(Entity, u16), bool>,
}

impl Effects {
    pub(crate) fn new() -> Self {
        Self {
            undo: Vec::new(),
            commands: Vec::new(),
            dead: HashMap::new(),
            present: HashMap::new(),
        }
    }

    pub(crate) fn spawned(&mut self, entity: Entity) {
        self.undo.push(Undo::Spawned(entity));
    }

    pub(crate) fn field(
        &mut self,
        row: u32,
        component: u16,
        field: u16,
        previous: dome_ecs::Value,
    ) {
        self.undo.push(Undo::Field {
            row,
            component,
            field,
            previous,
        });
    }

    pub(crate) fn edited(&mut self, undo: impl FnOnce(&mut World) + 'static) {
        self.undo.push(Undo::Edited(Box::new(undo)));
    }

    pub(crate) fn add(&mut self, entity: Entity, component: u16, fields: Vec<dome_ecs::Value>) {
        self.commands.push(Command::Add {
            entity,
            component,
            fields,
        });
    }

    pub(crate) fn remove(&mut self, entity: Entity, component: u16) {
        self.commands.push(Command::Remove { entity, component });
    }

    pub(crate) fn despawn(&mut self, entity: Entity) {
        self.commands.push(Command::Despawn(entity));
    }

    pub(crate) fn commit(&mut self, world: &mut World) -> Result<(), Trap> {
        let commands = std::mem::take(&mut self.commands);
        self.verify(world, &commands)?;
        for command in commands {
            match command {
                Command::Add {
                    entity,
                    component,
                    fields,
                } => world.dynamic_mut().insert(entity, component, &fields),
                Command::Remove { entity, component } => {
                    assert!(
                        world.dynamic_mut().remove(entity, component),
                        "a verified command removes a component its entity holds",
                    );
                }
                Command::Despawn(entity) => {
                    assert!(
                        world.despawn(entity),
                        "a verified command despawns a live entity",
                    );
                }
            }
        }
        self.undo.clear();
        Ok(())
    }

    pub(crate) fn rollback(&mut self, world: &mut World) {
        self.commands.clear();
        for undo in self.undo.drain(..).rev() {
            match undo {
                Undo::Spawned(entity) => {
                    world.despawn(entity);
                }
                Undo::Field {
                    row,
                    component,
                    field,
                    previous,
                } => world.dynamic_mut().write(row, component, field, previous),
                Undo::Edited(undo) => undo(world),
            }
        }
    }

    fn verify(&mut self, world: &World, commands: &[Command]) -> Result<(), Trap> {
        self.dead.clear();
        self.present.clear();
        for command in commands {
            match command {
                Command::Add {
                    entity, component, ..
                } => {
                    self.live(world, *entity, "adds a component to")?;
                    self.present.insert((*entity, *component), true);
                }
                Command::Remove { entity, component } => {
                    self.live(world, *entity, "removes a component from")?;
                    if !self.holds(world, *entity, *component) {
                        return Err(Trap::new(format!(
                            "the lampas command removes a component the entity {} holds none of",
                            entity.index(),
                        )));
                    }
                    self.present.insert((*entity, *component), false);
                }
                Command::Despawn(entity) => {
                    self.live(world, *entity, "despawns")?;
                    self.kill(world, *entity);
                }
            }
        }
        Ok(())
    }

    fn live(&self, world: &World, entity: Entity, holds: &str) -> Result<(), Trap> {
        if self.dead.contains_key(&entity) || !world.is_alive(entity) {
            return Err(Trap::new(format!(
                "the lampas command {holds} the dead entity {}",
                entity.index(),
            )));
        }
        Ok(())
    }

    fn holds(&self, world: &World, entity: Entity, component: u16) -> bool {
        if self.dead.contains_key(&entity) || !world.is_alive(entity) {
            return false;
        }
        self.present
            .get(&(entity, component))
            .copied()
            .unwrap_or_else(|| world.dynamic().row(component, entity).is_some())
    }

    fn kill(&mut self, world: &World, entity: Entity) {
        let mut held = vec![entity];
        let mut at = 0;
        while at < held.len() {
            let children = children(world, held[at]);
            held.extend_from_slice(children);
            at += 1;
        }
        for dead in held {
            self.dead.insert(dead, ());
        }
    }
}

use crate::command::Command;
use crate::host::Hosts;
use crate::store::Store;
use dome_ecs::{Entity, World};
use lampas_vm::{Context, Value};

pub(crate) struct Runner<'a> {
    pub(crate) store: &'a mut Store,
    pub(crate) world: &'a mut World,
    pub(crate) hosts: &'a mut Hosts,
    pub(crate) commands: &'a mut Vec<Command>,
}

impl Context for Runner<'_> {
    fn read(&mut self, row: u32, component: u16, field: u16) -> Value {
        self.store.read(row, component, field)
    }

    fn write(&mut self, row: u32, component: u16, field: u16, value: Value) {
        self.store.write(row, component, field, value);
    }

    fn has(&mut self, entity: Entity, component: u16) -> bool {
        self.store.row(component, entity).is_some()
    }

    fn spawn(&mut self) -> Entity {
        self.world.spawn()
    }

    fn add(&mut self, entity: Entity, component: u16, fields: &[Value]) {
        self.commands.push(Command::Add {
            entity,
            component,
            fields: fields.to_vec(),
        });
    }

    fn remove(&mut self, entity: Entity, component: u16) {
        self.commands.push(Command::Remove { entity, component });
    }

    fn despawn(&mut self, entity: Entity) {
        self.commands.push(Command::Despawn(entity));
    }

    fn host(&mut self, function: u16, arguments: &[Value]) -> Value {
        self.hosts.call(function, arguments, self.world)
    }
}

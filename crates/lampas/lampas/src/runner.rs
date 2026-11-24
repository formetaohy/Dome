use crate::command::Command;
use crate::host::Hosts;
use crate::scripts::ScriptSpawned;
use dome_ecs::{Entity, World};
use lampas_vm::{Context, Value};

pub(crate) struct Runner<'a> {
    pub(crate) world: &'a mut World,
    pub(crate) hosts: &'a mut Hosts,
    pub(crate) commands: &'a mut Vec<Command>,
}

impl Context for Runner<'_> {
    fn read(&mut self, row: u32, component: u16, field: u16) -> Value {
        Value::from(self.world.dynamic().read(row, component, field))
    }

    fn write(&mut self, row: u32, component: u16, field: u16, value: Value) {
        let value = dome_ecs::Value::from(&value);
        self.world.dynamic_mut().write(row, component, field, value);
    }

    fn has(&mut self, entity: Entity, component: u16) -> bool {
        self.world.dynamic().row(component, entity).is_some()
    }

    fn spawn(&mut self) -> Entity {
        let entity = self.world.spawn();
        self.world.insert(entity, ScriptSpawned);
        entity
    }

    fn add(&mut self, entity: Entity, component: u16, fields: &[Value]) {
        self.commands.push(Command::Add {
            entity,
            component,
            fields: fields.iter().map(dome_ecs::Value::from).collect(),
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

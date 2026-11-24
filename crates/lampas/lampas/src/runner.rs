use crate::effects::Effects;
use crate::host::{Call, Hosts};
use crate::scripts::ScriptSpawned;
use dome_ecs::{Entity, World};
use lampas_vm::{Context, Trap, Value};

pub(crate) struct Runner<'a> {
    pub(crate) world: &'a mut World,
    pub(crate) hosts: &'a mut Hosts,
    pub(crate) effects: &'a mut Effects,
}

impl Context for Runner<'_> {
    fn read(&mut self, row: u32, component: u16, field: u16) -> Value {
        Value::from(self.world.dynamic().read(row, component, field))
    }

    fn write(&mut self, row: u32, component: u16, field: u16, value: Value) {
        let previous = self.world.dynamic().read(row, component, field).clone();
        self.effects.field(row, component, field, previous);
        let value = dome_ecs::Value::from(&value);
        self.world.dynamic_mut().write(row, component, field, value);
    }

    fn has(&mut self, entity: Entity, component: u16) -> bool {
        self.world.dynamic().row(component, entity).is_some()
    }

    fn spawn(&mut self) -> Entity {
        let entity = self.world.spawn();
        self.world.insert(entity, ScriptSpawned);
        self.effects.spawned(entity);
        entity
    }

    fn add(&mut self, entity: Entity, component: u16, fields: &[Value]) {
        let fields = fields.iter().map(dome_ecs::Value::from).collect();
        self.effects.add(entity, component, fields);
    }

    fn remove(&mut self, entity: Entity, component: u16) {
        self.effects.remove(entity, component);
    }

    fn despawn(&mut self, entity: Entity) {
        self.effects.despawn(entity);
    }

    fn host(&mut self, function: u16, arguments: &[Value]) -> Result<Value, Trap> {
        let mut call = Call::new(self.world, self.effects);
        self.hosts.call(function, arguments, &mut call)
    }
}

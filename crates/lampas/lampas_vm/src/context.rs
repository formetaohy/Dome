use crate::trap::Trap;
use crate::value::Value;
use dome_ecs::Entity;

pub trait Context {
    fn read(&mut self, row: u32, component: u16, field: u16) -> Value;

    fn write(&mut self, row: u32, component: u16, field: u16, value: Value);

    fn has(&mut self, entity: Entity, component: u16) -> bool;

    fn spawn(&mut self) -> Entity;

    fn add(&mut self, entity: Entity, component: u16, fields: &[Value]);

    fn remove(&mut self, entity: Entity, component: u16);

    fn despawn(&mut self, entity: Entity);

    fn host(&mut self, function: u16, arguments: &[Value]) -> Result<Value, Trap>;
}

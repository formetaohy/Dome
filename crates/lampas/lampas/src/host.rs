use crate::effects::Effects;
use dome_ecs::{Component, Entity, World};
use lampas_bytecode::Kind;
use lampas_semantic::{HostSignature, Intrinsic, Type};
use lampas_vm::{Trap, Value};

type Callback = Box<dyn FnMut(&[Value], &mut Call<'_>) -> Result<Value, Trap>>;

struct Entry {
    signature: HostSignature,
    call: Callback,
}

#[derive(Default)]
pub struct Hosts {
    entries: Vec<Entry>,
}

pub struct Call<'a> {
    world: &'a mut World,
    effects: &'a mut Effects,
}

impl<'a> Call<'a> {
    pub(crate) fn new(world: &'a mut World, effects: &'a mut Effects) -> Self {
        Self { world, effects }
    }

    pub fn world(&self) -> &World {
        self.world
    }

    pub fn edit<T: Component + Clone>(
        &mut self,
        entity: Entity,
        edit: impl FnOnce(&mut T),
    ) -> Result<(), Trap> {
        let previous = self
            .world
            .get::<T>(entity)
            .cloned()
            .ok_or_else(|| missing(entity))?;
        let held = self
            .world
            .get_mut::<T>(entity)
            .ok_or_else(|| missing(entity))?;
        self.effects.edited(move |world: &mut World| {
            if let Some(held) = world.get_mut::<T>(entity) {
                *held = previous;
            }
        });
        edit(held);
        Ok(())
    }
}

fn missing(entity: Entity) -> Trap {
    Trap::new(format!(
        "the lampas script edits a component the entity {} does not hold",
        entity.index(),
    ))
}

impl Hosts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn function(
        &mut self,
        name: &str,
        params: &[Type],
        result: Type,
        call: impl FnMut(&[Value], &mut Call<'_>) -> Result<Value, Trap> + 'static,
    ) -> &mut Self {
        self.holds_name(name);
        assert!(
            Intrinsic::named(name).is_none(),
            "the host function {name} holds the name of an intrinsic",
        );
        for param in params {
            assert!(
                param.is_value(),
                "the host function {name} takes {}",
                describe(param),
            );
        }
        assert!(
            result.is_value() || result == Type::VOID,
            "the host function {name} returns {}",
            describe(&result),
        );
        self.entries.push(Entry {
            signature: HostSignature {
                name: String::from(name),
                params: params.to_vec(),
                result,
            },
            call: Box::new(call),
        });
        self
    }

    pub fn signatures(&self) -> Vec<HostSignature> {
        self.entries
            .iter()
            .map(|entry| entry.signature.clone())
            .collect()
    }

    pub(crate) fn call(
        &mut self,
        index: u16,
        arguments: &[Value],
        call: &mut Call<'_>,
    ) -> Result<Value, Trap> {
        let entry = &mut self.entries[index as usize];
        let value = (entry.call)(arguments, call)?;
        if !holds(&entry.signature.result, &value) {
            return Err(Trap::new(format!(
                "the host function {} returns a {} where {} belongs",
                entry.signature.name,
                value.kind(),
                describe(&entry.signature.result),
            )));
        }
        Ok(value)
    }

    fn holds_name(&self, name: &str) {
        let mut held = name.chars();
        assert!(
            held.next().is_some_and(|first| first.is_ascii_lowercase()),
            "the host function {name} starts with a small letter",
        );
        assert!(
            held.all(|held| held.is_ascii_alphanumeric() || held == '_'),
            "the host function {name} holds a letter, digit or _",
        );
        assert!(
            !self
                .entries
                .iter()
                .any(|entry| entry.signature.name == name),
            "the host holds the function {name} twice",
        );
    }
}

fn holds(ty: &Type, value: &Value) -> bool {
    match (ty, value) {
        (Type::Void, Value::Unit) => true,
        (Type::Value(kind), value) => value_kind(value) == Some(*kind),
        (Type::List(kind), Value::List(items)) => items
            .borrow()
            .iter()
            .all(|item| value_kind(item) == Some(*kind)),
        _ => false,
    }
}

fn value_kind(value: &Value) -> Option<Kind> {
    Some(match value {
        Value::Unit | Value::List(_) | Value::Component(_) => return None,
        Value::Bool(_) => Kind::Bool,
        Value::Int(_) => Kind::Int,
        Value::Float(_) => Kind::Float,
        Value::Vector(_) => Kind::Vector,
        Value::Entity(_) => Kind::Entity,
        Value::Text(_) => Kind::Text,
    })
}

fn describe(ty: &Type) -> String {
    match ty {
        Type::Void => String::from("no value"),
        Type::Value(kind) => String::from(kind.name()),
        Type::List(kind) => format!("a list of {}", kind.name()),
        Type::Component(_) => String::from("a component"),
    }
}

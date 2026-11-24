use dome_ecs::World;
use lampas_bytecode::FieldKind;
use lampas_semantic::{HostSignature, Intrinsic, Type};
use lampas_vm::Value;

type Callback = Box<dyn FnMut(&[Value], &mut World) -> Value>;

struct Entry {
    signature: HostSignature,
    call: Callback,
}

#[derive(Default)]
pub struct Hosts {
    entries: Vec<Entry>,
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
        call: impl FnMut(&[Value], &mut World) -> Value + 'static,
    ) -> &mut Self {
        assert!(!name.is_empty(), "a host function holds a name");
        assert!(
            Intrinsic::named(name).is_none(),
            "the host function {name} holds the name of an intrinsic",
        );
        assert!(
            self.entries
                .iter()
                .all(|entry| entry.signature.name != name),
            "the host holds the function {name} twice",
        );
        for ty in params.iter().chain(std::iter::once(&result)) {
            assert!(
                !matches!(ty, Type::Component(_)),
                "the host function {name} passes no component",
            );
        }
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

    pub(crate) fn call(&mut self, index: u16, arguments: &[Value], world: &mut World) -> Value {
        let entry = &mut self.entries[index as usize];
        let value = (entry.call)(arguments, world);
        assert!(
            holds(&entry.signature.result, &value),
            "the host function {} returns a {} where {} belongs",
            entry.signature.name,
            value.kind(),
            describe(&entry.signature.result),
        );
        value
    }
}

fn holds(ty: &Type, value: &Value) -> bool {
    match (ty, value) {
        (Type::Void, Value::Unit) => true,
        (Type::Value(kind), value) => kind_of(value) == Some(*kind),
        (Type::List(_), Value::List(_)) => true,
        _ => false,
    }
}

fn kind_of(value: &Value) -> Option<FieldKind> {
    Some(match value {
        Value::Unit => return None,
        Value::Bool(_) => FieldKind::Bool,
        Value::Int(_) => FieldKind::Int,
        Value::Float(_) => FieldKind::Float,
        Value::Vector(_) => FieldKind::Vector,
        Value::Entity(_) => FieldKind::Entity,
        Value::Text(_) => FieldKind::Text,
        Value::List(_) | Value::Component(_) => return None,
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

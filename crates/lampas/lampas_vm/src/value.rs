use dome_ecs::Entity;
use dome_math::Vec3;
use lampas_bytecode::Constant;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Vector(Vec3),
    Entity(Entity),
    Text(Rc<str>),
    List(Rc<RefCell<Vec<Value>>>),
    Component(u32),
}

impl Value {
    pub fn constant(value: &Constant) -> Self {
        match value {
            Constant::Bool(held) => Self::Bool(*held),
            Constant::Int(held) => Self::Int(*held),
            Constant::Float(held) => Self::Float(*held),
            Constant::Text(held) => Self::Text(held.clone()),
            Constant::Vector(held) => Self::Vector(*held),
            Constant::List(items) => Self::List(Rc::new(RefCell::new(
                items.iter().map(Self::constant).collect(),
            ))),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Unit => "no value",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Vector(_) => "vec3",
            Self::Entity(_) => "entity",
            Self::Text(_) => "string",
            Self::List(_) => "list",
            Self::Component(_) => "component",
        }
    }

    pub fn boolean(&self) -> bool {
        match self {
            Self::Bool(held) => *held,
            other => other.misuse("a bool"),
        }
    }

    pub fn integer(&self) -> i64 {
        match self {
            Self::Int(held) => *held,
            other => other.misuse("an int"),
        }
    }

    pub fn float(&self) -> f64 {
        match self {
            Self::Float(held) => *held,
            other => other.misuse("a float"),
        }
    }

    pub fn vector(&self) -> Vec3 {
        match self {
            Self::Vector(held) => *held,
            other => other.misuse("a vec3"),
        }
    }

    pub fn entity(&self) -> Entity {
        match self {
            Self::Entity(held) => *held,
            other => other.misuse("an entity"),
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Self::Text(held) => held,
            other => other.misuse("a string"),
        }
    }

    pub fn list(&self) -> &RefCell<Vec<Value>> {
        match self {
            Self::List(held) => held,
            other => other.misuse("a list"),
        }
    }

    pub fn component(&self) -> u32 {
        match self {
            Self::Component(held) => *held,
            other => other.misuse("a component"),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Unit => String::from("()"),
            Self::Bool(held) => held.to_string(),
            Self::Int(held) => held.to_string(),
            Self::Float(held) => held.to_string(),
            Self::Vector(held) => format!("({}, {}, {})", held.x, held.y, held.z),
            Self::Entity(held) => {
                format!(
                    "entity {} of generation {}",
                    held.index(),
                    held.generation()
                )
            }
            Self::Text(held) => held.to_string(),
            Self::List(held) => {
                let items = held.borrow();
                let mut text = String::from("[");
                for (at, item) in items.iter().enumerate() {
                    if at > 0 {
                        text.push_str(", ");
                    }
                    text.push_str(&item.describe());
                }
                text.push(']');
                text
            }
            Self::Component(row) => format!("the component row {row}"),
        }
    }

    fn misuse(&self, expected: &str) -> ! {
        panic!("a lampas {} stands where {expected} belongs", self.kind(),)
    }
}

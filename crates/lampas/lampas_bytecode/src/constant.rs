use dome_math::Vec3;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldKind {
    Bool,
    Int,
    Float,
    Text,
    Vector,
    Entity,
}

impl FieldKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Float => "float",
            Self::Text => "string",
            Self::Vector => "vec3",
            Self::Entity => "entity",
        }
    }
}

impl fmt::Display for FieldKind {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(self.name())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(Rc<str>),
    Vector(Vec3),
    List(Vec<Constant>),
}

impl Constant {}

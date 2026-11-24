use crate::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbol {
    None,
    Local(u8),
    Global(u16),
    Function(u16),
    Host(u16),
    Intrinsic(Intrinsic),
    Field { component: u16, field: u16 },
    Component(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intrinsic {
    Spawn,
    Despawn,
    Add,
    Remove,
    Has,
    Vector,
    Length,
    Push,
    Int,
    Float,
    Text,
    SquareRoot,
    Sine,
    Cosine,
    Floor,
    Absolute,
    Minimum,
    Maximum,
    Normalize,
    Dot,
}

impl Intrinsic {
    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "spawn" => Self::Spawn,
            "despawn" => Self::Despawn,
            "add" => Self::Add,
            "remove" => Self::Remove,
            "has" => Self::Has,
            "vec3" => Self::Vector,
            "length" => Self::Length,
            "push" => Self::Push,
            "int" => Self::Int,
            "float" => Self::Float,
            "text" => Self::Text,
            "sqrt" => Self::SquareRoot,
            "sin" => Self::Sine,
            "cos" => Self::Cosine,
            "floor" => Self::Floor,
            "abs" => Self::Absolute,
            "min" => Self::Minimum,
            "max" => Self::Maximum,
            "normalize" => Self::Normalize,
            "dot" => Self::Dot,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Spawn => "spawn",
            Self::Despawn => "despawn",
            Self::Add => "add",
            Self::Remove => "remove",
            Self::Has => "has",
            Self::Vector => "vec3",
            Self::Length => "length",
            Self::Push => "push",
            Self::Int => "int",
            Self::Float => "float",
            Self::Text => "text",
            Self::SquareRoot => "sqrt",
            Self::Sine => "sin",
            Self::Cosine => "cos",
            Self::Floor => "floor",
            Self::Absolute => "abs",
            Self::Minimum => "min",
            Self::Maximum => "max",
            Self::Normalize => "normalize",
            Self::Dot => "dot",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ExprInfo {
    pub ty: Type,
    pub symbol: Symbol,
}

#[derive(Clone, Copy, Debug)]
pub enum StmtInfo {
    Let(u8),
    For { binder: u8, list: u8, index: u8 },
}

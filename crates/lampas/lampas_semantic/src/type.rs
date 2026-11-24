use lampas_bytecode::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Void,
    Value(Kind),
    List(Kind),
    Component(u16),
}

impl Type {
    pub const VOID: Self = Self::Void;
    pub const BOOL: Self = Self::Value(Kind::Bool);
    pub const INT: Self = Self::Value(Kind::Int);
    pub const FLOAT: Self = Self::Value(Kind::Float);
    pub const TEXT: Self = Self::Value(Kind::Text);
    pub const VECTOR: Self = Self::Value(Kind::Vector);
    pub const ENTITY: Self = Self::Value(Kind::Entity);

    pub const fn list(kind: Kind) -> Self {
        Self::List(kind)
    }

    pub fn kind(self) -> Option<Kind> {
        match self {
            Self::Value(kind) => Some(kind),
            _ => None,
        }
    }

    pub fn is_value(self) -> bool {
        matches!(self, Self::Value(_) | Self::List(_))
    }
}

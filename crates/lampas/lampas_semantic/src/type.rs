use lampas_bytecode::FieldKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Void,
    Value(FieldKind),
    List(FieldKind),
    Component(u16),
}

impl Type {
    pub const VOID: Self = Self::Void;
    pub const BOOL: Self = Self::Value(FieldKind::Bool);
    pub const INT: Self = Self::Value(FieldKind::Int);
    pub const FLOAT: Self = Self::Value(FieldKind::Float);
    pub const TEXT: Self = Self::Value(FieldKind::Text);
    pub const VECTOR: Self = Self::Value(FieldKind::Vector);
    pub const ENTITY: Self = Self::Value(FieldKind::Entity);

    pub const fn list(kind: FieldKind) -> Self {
        Self::List(kind)
    }

    pub fn kind(self) -> Option<FieldKind> {
        match self {
            Self::Value(kind) => Some(kind),
            _ => None,
        }
    }

    pub fn is_value(self) -> bool {
        matches!(self, Self::Value(_) | Self::List(_))
    }
}

use crate::constant::{Constant, FieldKind};
use crate::instruction::Instruction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Update,
    Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentParam {
    pub component: u16,
    pub access: Access,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub kind: FieldKind,
    pub default: Option<Constant>,
}

#[derive(Clone, Debug)]
pub struct Component {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug)]
pub struct Global {
    pub name: String,
    pub value: Constant,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub registers: u16,
    pub code: Vec<Instruction>,
}

#[derive(Clone, Debug)]
pub struct System {
    pub name: String,
    pub phase: Phase,
    pub function: u32,
    pub components: Vec<ComponentParam>,
}

#[derive(Clone, Debug, Default)]
pub struct Module {
    pub components: Vec<Component>,
    pub globals: Vec<Global>,
    pub constants: Vec<Constant>,
    pub functions: Vec<Function>,
    pub systems: Vec<System>,
    pub boot: Option<u32>,
}

impl Module {
    pub fn new() -> Self {
        Self::default()
    }
}

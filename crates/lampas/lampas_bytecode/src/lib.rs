mod constant;
mod instruction;
mod module;

pub use constant::{Constant, FieldKind};
pub use instruction::{Instruction, Op, fielded, pair};
pub use module::{
    Access, Component, ComponentParam, Field, Function, Global, Module, Phase, System,
};

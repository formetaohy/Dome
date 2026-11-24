mod constant;
mod instruction;
mod module;

pub use constant::Constant;
pub use dome_ecs::Kind;
pub use instruction::{Instruction, Op, fielded, pair};
pub use module::{
    Access, Component, ComponentParam, Field, Function, Global, Module, Phase, System,
};

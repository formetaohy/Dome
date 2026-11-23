mod arithmetic;
pub mod instruction;
mod literal;
pub mod module;
pub mod msl;
pub mod program;
pub mod spirv;
pub mod ty;
mod validate;

pub use arithmetic::IntegerArithmetic;
pub use instruction::{
    Address, AtomicOp, Barrier, BinaryOp, Block, BuiltIn, Constant, DerivativeOp, Instruction,
    MathFun, MatrixLayout, UnaryOp,
};
pub use module::{
    Access, Argument, Binding, Function, Global, Interface, Local, Module, Requirements, Space,
    Stage, StageEntry, Target, element_name,
};
pub use program::{
    Backend, BindingKind, BindingSpec, ComputeProgram, GraphicsProgram, GraphicsTranslation,
    MAX_BINDING_BYTES, METAL_SIZE_BUFFER_SLOT, SAMPLER_TABLE_CAPACITY, ShaderBinding,
    ShaderTranslation, TEXTURE_TABLE_CAPACITY, describe, reflect,
};
pub use ty::{MatrixUse, Member, Scalar, Type, TypeId, ValueId};

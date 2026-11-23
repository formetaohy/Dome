extern crate self as dome_gpu;

pub mod ast;
pub mod compiler;
pub mod layout;
pub mod shader;

pub use compiler::{
    AtomicU32, Compiler, Fvec2, Fvec4, In, Out, Position, Read, ReadWrite, Sampler, SamplerTable,
    Texture2D, TextureTable, Uvec3, Uvec4, Workgroup,
};
pub use dome_gpu_macro::{graphics, kernel, module};
pub use layout::{
    Block, Element, FP4_BLOCK, FieldLayout, FieldType, FloatFormat, INT4_BLOCK, QUANT_BLOCK,
    Record, RecordLayout, SUPER_BLOCK, SUPER_SCALE_BYTES, SUPER_SMALL_BLOCK, SUPER_SUB_BLOCK,
    WORD_BYTES,
};
pub use shader::{
    BindingKind, BindingSpec, ComputeProgram, GraphicsProgram, GraphicsTranslation, Interface,
    SAMPLER_TABLE_CAPACITY, ShaderBinding, ShaderTranslation, Space, Stage, TEXTURE_TABLE_CAPACITY,
    Target,
};

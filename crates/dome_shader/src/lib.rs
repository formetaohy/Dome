extern crate self as dome_shader;

pub mod ast;
pub mod compiler;
pub mod layout;
pub mod shader;

pub use compiler::{
    AtomicU32, ComparisonSampler, Compiler, Depth2D, Fvec2, Fvec4, In, Out, Position, Read,
    ReadWrite, SampledDepth, Sampler, SamplerTable, Texture2D, TextureTable, Uvec3, Uvec4,
    Workgroup,
};
pub use dome_shader_macro::{graphics, kernel, module};
pub use layout::{FieldLayout, FieldType, Record, RecordLayout};
pub use shader::{
    BindingKind, BindingSpec, ComputeProgram, GraphicsProgram, GraphicsTranslation, Interface,
    SAMPLER_TABLE_CAPACITY, ShaderBinding, ShaderTranslation, Space, Stage, TEXTURE_TABLE_CAPACITY,
    Target,
};

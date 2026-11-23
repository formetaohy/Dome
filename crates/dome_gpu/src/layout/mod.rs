pub mod element;
pub mod record;

pub use element::{
    Block, Element, FP4_BLOCK, FloatFormat, INT4_BLOCK, QUANT_BLOCK, SUPER_BLOCK,
    SUPER_SCALE_BYTES, SUPER_SMALL_BLOCK, SUPER_SUB_BLOCK,
};
pub use record::{FieldLayout, FieldType, Record, RecordLayout};

pub const WORD_BYTES: u64 = 4;

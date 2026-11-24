#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    LoadConstant,
    LoadGlobal,
    StoreGlobal,
    Move,
    LoadUnit,

    Not,
    NegateInt,
    NegateFloat,
    NegateVector,

    AddInt,
    SubtractInt,
    MultiplyInt,
    DivideInt,
    RemainderInt,
    AddFloat,
    SubtractFloat,
    MultiplyFloat,
    DivideFloat,
    RemainderFloat,
    AddVector,
    SubtractVector,
    ScaleVector,
    DivideVector,

    EqualInt,
    NotEqualInt,
    EqualFloat,
    NotEqualFloat,
    EqualBool,
    NotEqualBool,
    EqualText,
    NotEqualText,
    EqualEntity,
    NotEqualEntity,
    EqualVector,
    NotEqualVector,
    LessInt,
    LessEqualInt,
    GreaterInt,
    GreaterEqualInt,
    LessFloat,
    LessEqualFloat,
    GreaterFloat,
    GreaterEqualFloat,

    Jump,
    JumpIfFalse,
    JumpIfTrue,
    Call,
    CallHost,
    Return,
    ReturnUnit,
    Stop,

    MakeList,
    MakeVector,
    ListLength,
    TextLength,
    ListPush,
    ListGet,
    ListSet,
    Concat,

    ReadField,
    WriteField,
    Has,
    Spawn,
    Despawn,
    AddComponent,
    RemoveComponent,

    IntToFloat,
    FloatToInt,
    ToText,

    SquareRoot,
    Sine,
    Cosine,
    Floor,
    Absolute,
    Minimum,
    Maximum,
    LengthVector,
    NormalizeVector,
    DotVector,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub op: Op,
    pub a: u8,
    pub b: u16,
    pub imm: u32,
}

impl Instruction {
    pub fn new(op: Op, a: u8, b: u16, imm: u32) -> Self {
        Self { op, a, b, imm }
    }

    pub fn low(self) -> u8 {
        self.b as u8
    }

    pub fn high(self) -> u8 {
        (self.b >> 8) as u8
    }

    pub fn component(self) -> u16 {
        (self.imm >> 16) as u16
    }

    pub fn field(self) -> u16 {
        self.imm as u16
    }
}

pub fn pair(low: u8, high: u8) -> u16 {
    u16::from(low) | (u16::from(high) << 8)
}

pub fn fielded(component: u16, field: u16) -> u32 {
    (u32::from(component) << 16) | u32::from(field)
}

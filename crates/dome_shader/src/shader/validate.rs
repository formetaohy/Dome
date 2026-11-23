use crate::shader::instruction::{Address, AtomicOp, BinaryOp, Instruction, MathFun, UnaryOp};
use crate::shader::module::{Function, Interface, Module, Space, Stage, element_name};
use crate::shader::ty::{Scalar, Type, TypeId, ValueId};

mod uniformity;

use std::collections::{HashMap, HashSet};
use uniformity::Analysis;

pub struct Verifier<'m> {
    module: &'m Module,
}

impl<'m> Verifier<'m> {
    pub fn new(module: &'m Module) -> Self {
        Self { module }
    }

    pub fn run(&self) {
        self.structure();
        for function in 0..self.module.functions().len() as u32 {
            Checker::new(self.module, function).run();
        }
        Analysis::new(self.module).run();
    }

    fn structure(&self) {
        let stages = self.module.stages();
        assert!(
            !stages.is_empty(),
            "a shader module declares no entry point"
        );
        for entry in stages {
            let stage = self.module.stage_function(entry.stage);
            assert!(
                stage.result.is_none(),
                "the shader {} entry {} returns a value",
                entry.stage.name(),
                stage.name
            );
            for argument in &stage.arguments {
                let builtin = argument.builtin.unwrap_or_else(|| {
                    panic!(
                        "the shader {} entry argument {} is not a builtin",
                        entry.stage.name(),
                        argument.name
                    )
                });
                assert!(
                    entry.stage.builtin(builtin),
                    "the shader {} stage reads the {} builtin",
                    entry.stage.name(),
                    builtin.name()
                );
            }
            assert!(
                entry.stage == Stage::Fragment || !discards(&stage.body),
                "the shader {} entry {} discards a fragment",
                entry.stage.name(),
                stage.name,
            );
        }
        for (index, global) in self.module.globals().iter().enumerate() {
            if !global.space.interface() {
                continue;
            }
            let used = stages
                .iter()
                .filter(|entry| {
                    self.module
                        .accessed_globals(entry.function)
                        .contains(&(index as u32))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                used.len(),
                1,
                "the interface variable {} is read by {} stages",
                global.name,
                used.len()
            );
            match (used[0].stage, global.space) {
                (Stage::Vertex, Space::Output)
                | (Stage::Fragment, Space::Output)
                | (Stage::Fragment, Space::Input) => {}
                (stage, space) => panic!(
                    "the shader {} stage reads the {} interface variable {}",
                    stage.name(),
                    space.name(),
                    global.name
                ),
            }
            assert!(
                global.space != Space::Input || global.access.uniform(),
                "the fragment stage writes the input varying {}",
                global.name
            );
        }
        for entry in stages {
            let mut locations = Vec::new();
            for index in self.module.accessed_globals(entry.function) {
                let global = self.module.global(index);
                if !global.space.interface() {
                    continue;
                }
                if let Some(Interface::Location(location)) = global.interface {
                    locations.push((global.space, location, global.name.clone()));
                }
            }
            locations.sort_by_key(|(space, location, _)| (*space as u8, *location));
            for pair in locations.windows(2) {
                assert!(
                    pair[0].0 != pair[1].0 || pair[0].1 != pair[1].1,
                    "the interface variables {} and {} of the {} stage share a location",
                    pair[0].2,
                    pair[1].2,
                    entry.stage.name()
                );
            }
        }
        assert!(
            self.module.compute().is_none()
                || self
                    .module
                    .globals()
                    .iter()
                    .all(|global| !global.space.interface()),
            "a compute program carries a graphics interface"
        );
        let mut groups = self
            .module
            .globals()
            .iter()
            .filter_map(|global| {
                global
                    .binding
                    .map(|binding| (binding.group, binding.binding, global))
            })
            .collect::<Vec<_>>();
        groups.sort_by_key(|(group, slot, _)| (*group, *slot));
        let mut expected = (0u32, 0u32);
        for (group, slot, global) in &groups {
            if *group != expected.0 {
                assert_eq!(
                    *group,
                    expected.0 + 1,
                    "binding groups are dense but {} lives in group {group}",
                    global.name
                );
                expected = (*group, 0);
            }
            assert_eq!(
                *slot, expected.1,
                "binding slots are dense within their group but {} occupies {slot}",
                global.name
            );
            expected.1 += 1;
            assert!(
                matches!(global.space, Space::Storage | Space::UniformConstant),
                "the shader global {} is bound yet lives in {} memory",
                global.name,
                global.space.name()
            );
            match global.space {
                Space::Storage => assert!(
                    matches!(self.module.ty(global.ty), Type::Block { .. }),
                    "the storage global {} holds {} instead of a storage block",
                    global.name,
                    element_name(self.module.ty(global.ty))
                ),
                Space::UniformConstant => assert!(
                    matches!(
                        self.module.ty(global.ty),
                        Type::Image | Type::DepthImage | Type::Sampler | Type::ComparisonSampler
                    ) || matches!(
                        self.module.ty(global.ty),
                        Type::Array { element, .. } if matches!(
                            self.module.ty(*element),
                            Type::Image | Type::Sampler
                        )
                    ),
                    "the uniform constant {} holds {}",
                    global.name,
                    element_name(self.module.ty(global.ty))
                ),
                _ => unreachable!("a bound global lives in storage or uniform constant memory"),
            }
        }
        for global in self.module.globals() {
            assert!(
                global.binding.is_some()
                    || global.space == Space::WorkGroup
                    || global.space.interface(),
                "the shader global {} is neither bound nor a workgroup or interface variable",
                global.name
            );
            assert!(
                !global.coherent || global.access.writable(),
                "the read-only shader global {} claims to be coherent",
                global.name
            );
            let subject = format!("the shader global {}", global.name);
            match self.module.ty(global.ty) {
                Type::Image | Type::DepthImage | Type::Sampler | Type::ComparisonSampler => {}
                Type::Array { element, .. }
                    if matches!(self.module.ty(*element), Type::Image | Type::Sampler) => {}
                Type::Block { element, .. } => self.storable(*element, &subject),
                _ => self.storable(global.ty, &subject),
            }
        }
        for ty in self.module.types() {
            let Type::Struct {
                name,
                members,
                span,
            } = ty
            else {
                continue;
            };
            let mut end = 0;
            for member in members {
                assert!(
                    self.module.ty(member.ty).scalar() != Some(Scalar::Bool),
                    "the shader struct {name} holds a boolean member {}",
                    member.name
                );
                assert!(
                    member.offset >= end,
                    "the shader struct {name} overlaps its member {}",
                    member.name
                );
                assert!(
                    member.offset + self.module.size(member.ty) <= *span,
                    "the shader struct {name} member {} outruns its span",
                    member.name
                );
                end = member.offset + self.module.size(member.ty);
            }
        }
    }

    fn storable(&self, ty: TypeId, subject: &str) {
        match self.module.ty(ty) {
            Type::Scalar(scalar) => assert!(
                scalar.storable(),
                "{subject} holds the boolean {}",
                scalar.name()
            ),
            Type::Vector { scalar, .. } => assert!(
                scalar.storable(),
                "{subject} holds the boolean {}",
                scalar.name()
            ),
            Type::Array { element, .. } => self.storable(*element, subject),
            Type::Struct { members, .. } => {
                for member in members {
                    self.storable(member.ty, subject);
                }
            }
            Type::Block { .. } => {
                panic!(
                    "{subject} holds {} beyond its own block",
                    element_name(self.module.ty(ty))
                )
            }
            Type::Atomic(scalar) => assert!(
                scalar.integer(),
                "{subject} atomically holds the non-integer {}",
                scalar.name()
            ),
            Type::Image | Type::DepthImage | Type::Sampler | Type::ComparisonSampler => {
                panic!("{subject} holds {}", element_name(self.module.ty(ty)))
            }
            Type::Pointer { .. } => panic!("{subject} holds a shader pointer"),
            Type::CooperativeMatrix { .. } => {
                panic!("{subject} holds a cooperative matrix")
            }
        }
    }
}

fn discards(body: &[Instruction]) -> bool {
    body.iter().any(|instruction| match instruction {
        Instruction::Discard => true,
        Instruction::If { accept, reject, .. } => discards(accept) || discards(reject),
        Instruction::Switch { cases, default, .. } => {
            cases.iter().any(|(_, body)| discards(body)) || discards(default)
        }
        Instruction::Loop { body, continuing } => discards(body) || discards(continuing),
        Instruction::Block(body) => discards(body),
        _ => false,
    })
}

struct Checker<'m> {
    module: &'m Module,
    index: u32,
    function: &'m Function,
    defined: HashSet<ValueId>,
    declarations: HashMap<ValueId, &'m Instruction>,
    loops: usize,
}

impl<'m> Checker<'m> {
    fn new(module: &'m Module, index: u32) -> Self {
        let function = &module.functions()[index as usize];
        let mut declarations = HashMap::new();
        collect(&function.body, &mut declarations);
        Self {
            module,
            index,
            function,
            defined: HashSet::new(),
            declarations,
            loops: 0,
        }
    }

    fn run(mut self) {
        self.block(&self.function.body);
    }

    fn block(&mut self, block: &[Instruction]) {
        for instruction in block {
            self.instruction(instruction);
            if let Some(result) = instruction.result() {
                if let Some(expected) = self.expected(instruction) {
                    assert_eq!(
                        self.module.value_ty(result),
                        expected,
                        "the shader instruction {instruction:?} defines a value of the wrong type"
                    );
                }
                assert!(
                    self.module.constant_of(result).is_none(),
                    "the shader function {} carries the instruction {instruction:?} whose result {} is a constant",
                    self.function.name,
                    result.index()
                );
                assert!(
                    self.defined.insert(result),
                    "the shader value {} is defined twice",
                    result.0
                );
            }
        }
    }

    fn instruction(&mut self, instruction: &Instruction) {
        for operand in instruction.operands() {
            assert!(
                self.defined.contains(&operand) || self.module.constant_of(operand).is_some(),
                "the shader value {} is used before it is defined in {}",
                operand.0,
                self.function.name
            );
        }
        match instruction {
            Instruction::Argument { index, result } => {
                assert!(
                    (*index as usize) < self.function.arguments.len(),
                    "the shader function {} has no argument {index}",
                    self.function.name
                );
                assert_eq!(
                    self.ty(*result),
                    self.function.arguments[*index as usize].ty,
                    "the shader argument {index} of {} defines a value of another type",
                    self.function.name
                );
            }
            Instruction::Address { address, result } => {
                self.address(*address);
                let (space, base) = match address {
                    Address::Global(index) => {
                        let global = self.module.global(*index);
                        (global.space, global.ty)
                    }
                    Address::Local(index) => {
                        (Space::Function, self.function.locals[*index as usize].ty)
                    }
                };
                assert_eq!(
                    self.module.ty(self.ty(*result)),
                    &Type::Pointer { space, base },
                    "the shader address of {} defines a value of another type",
                    self.function.name
                );
            }
            Instruction::Access { base, index, .. } => {
                let (space, element) = self.pointee(*base, "an indexed access");
                self.index(*index);
                self.expect_pointer(instruction, space, self.module.element(element));
            }
            Instruction::AccessIndex { base, index, .. } => {
                self.access_index(*base, *index, instruction);
            }
            Instruction::Load { pointer, .. } => {
                let (_, base) = self.pointee(*pointer, "a load");
                assert!(
                    !matches!(
                        self.module.ty(base),
                        Type::Array { .. } | Type::Block { .. }
                    ),
                    "{} is read as a whole instead of element by element",
                    element_name(self.module.ty(base))
                );
                assert!(
                    !matches!(self.module.ty(base), Type::Atomic(_)),
                    "a shader reads an atomic cell through a plain load"
                );
            }
            Instruction::AtomicLoad { pointer, result } => {
                let (_, base) = self.pointee(*pointer, "an atomic load");
                let Type::Atomic(scalar) = self.module.ty(base) else {
                    panic!(
                        "a shader atomic load reads {} instead of an atomic cell",
                        element_name(self.module.ty(base))
                    );
                };
                assert_eq!(
                    self.ty(*result),
                    self.module.scalar_id(*scalar),
                    "a shader atomic load yields a different type"
                );
            }
            Instruction::AtomicStore { pointer, value } => {
                self.writable(*pointer, "an atomic store");
                let (_, base) = self.pointee(*pointer, "an atomic store");
                let Type::Atomic(scalar) = self.module.ty(base) else {
                    panic!(
                        "a shader atomic store writes {} instead of an atomic cell",
                        element_name(self.module.ty(base))
                    );
                };
                assert_eq!(
                    self.ty(*value),
                    self.module.scalar_id(*scalar),
                    "a shader atomic store holds a different type"
                );
            }
            Instruction::Derivative { value, .. } => {
                assert!(
                    !matches!(
                        self.module.stage_of(self.index),
                        Some(crate::shader::Stage::Vertex | crate::shader::Stage::Compute)
                    ),
                    "a shader derivative belongs to the fragment stage"
                );
                assert!(
                    matches!(
                        self.module.ty(self.ty(*value)),
                        Type::Scalar(Scalar::F32)
                            | Type::Vector {
                                scalar: Scalar::F32,
                                ..
                            }
                    ),
                    "a shader derivative reads {}",
                    element_name(self.module.ty(self.ty(*value)))
                );
            }
            Instruction::Store { pointer, value } => {
                self.writable(*pointer, "a shader store");
                let pointer_ty = self.ty(*pointer);
                let (_, base) = self.pointee(*pointer, "a store");
                assert!(
                    !matches!(self.module.ty(base), Type::Atomic(_)),
                    "a shader writes an atomic cell through a plain store"
                );
                assert_eq!(
                    self.ty(*value),
                    self.module.loaded_ty(pointer_ty),
                    "a shader store writes {} where {} stands",
                    element_name(self.module.ty(self.ty(*value))),
                    element_name(self.module.ty(base))
                );
            }
            Instruction::Sample {
                image,
                sampler,
                coordinates,
                level,
                result,
            } => {
                if level.is_none() {
                    assert!(
                        !matches!(
                            self.module.stage_of(self.index),
                            Some(crate::shader::Stage::Vertex | crate::shader::Stage::Compute)
                        ),
                        "implicit level of detail sampling belongs to the fragment stage"
                    );
                }
                if let Some(level) = level {
                    assert!(
                        matches!(self.module.ty(self.ty(*level)), Type::Scalar(Scalar::F32)),
                        "a shader sample reads {} as its level where f32 stands",
                        element_name(self.module.ty(self.ty(*level)))
                    );
                }
                assert!(
                    matches!(self.module.ty(self.ty(*image)), Type::Image),
                    "a shader sample reads {} as its image",
                    element_name(self.module.ty(self.ty(*image)))
                );
                assert!(
                    matches!(self.module.ty(self.ty(*sampler)), Type::Sampler),
                    "a shader sample reads {} as its sampler",
                    element_name(self.module.ty(self.ty(*sampler)))
                );
                assert!(
                    matches!(
                        self.module.ty(self.ty(*coordinates)),
                        Type::Vector {
                            scalar: Scalar::F32,
                            length: 2
                        }
                    ),
                    "a shader sample takes {} as its coordinates",
                    element_name(self.module.ty(self.ty(*coordinates)))
                );
                assert!(
                    matches!(
                        self.module.ty(self.ty(*result)),
                        Type::Vector {
                            scalar: Scalar::F32,
                            length: 4
                        }
                    ),
                    "a shader sample yields {}",
                    element_name(self.module.ty(self.ty(*result)))
                );
            }
            Instruction::SampleCompare {
                depth,
                sampler,
                coordinates,
                reference,
                result,
            } => {
                assert!(
                    !matches!(
                        self.module.stage_of(self.index),
                        Some(crate::shader::Stage::Vertex | crate::shader::Stage::Compute)
                    ),
                    "comparison sampling belongs to the fragment stage"
                );
                assert!(
                    matches!(self.module.ty(self.ty(*depth)), Type::DepthImage),
                    "a shader comparison reads {} as its depth image",
                    element_name(self.module.ty(self.ty(*depth)))
                );
                assert!(
                    matches!(self.module.ty(self.ty(*sampler)), Type::ComparisonSampler),
                    "a shader comparison reads {} as its sampler",
                    element_name(self.module.ty(self.ty(*sampler)))
                );
                assert!(
                    matches!(
                        self.module.ty(self.ty(*coordinates)),
                        Type::Vector {
                            scalar: Scalar::F32,
                            length: 2
                        }
                    ),
                    "a shader comparison takes {} as its coordinates",
                    element_name(self.module.ty(self.ty(*coordinates)))
                );
                assert!(
                    matches!(
                        self.module.ty(self.ty(*reference)),
                        Type::Scalar(Scalar::F32)
                    ),
                    "a shader comparison compares against {}",
                    element_name(self.module.ty(self.ty(*reference)))
                );
                assert!(
                    matches!(self.module.ty(self.ty(*result)), Type::Scalar(Scalar::F32)),
                    "a shader comparison yields {}",
                    element_name(self.module.ty(self.ty(*result)))
                );
            }
            Instruction::Unary { op, value, .. } => {
                let scalar = self.scalar(*value, "a unary operation");
                match op {
                    UnaryOp::Negate => assert!(
                        scalar.floating() || scalar.signed(),
                        "a shader negation applies to the unsigned {}",
                        scalar.name()
                    ),
                    UnaryOp::LogicalNot => assert_eq!(
                        scalar,
                        Scalar::Bool,
                        "a shader logical negation applies to the {}",
                        scalar.name()
                    ),
                    UnaryOp::BitwiseNot => assert!(
                        scalar.integer(),
                        "a shader bitwise negation applies to the {}",
                        scalar.name()
                    ),
                }
            }
            Instruction::Binary {
                op,
                left,
                right,
                result,
            } => {
                let outer = self.lane(*left);
                match op {
                    BinaryOp::LogicalAnd | BinaryOp::LogicalOr => assert_eq!(
                        outer,
                        Scalar::Bool,
                        "a shader logical operation applies to the {}",
                        outer.name()
                    ),
                    BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::Xor
                    | BinaryOp::Modulo
                    | BinaryOp::ShiftLeft
                    | BinaryOp::ShiftRight => assert!(
                        outer.integer(),
                        "a shader bitwise operation applies to the {}",
                        outer.name()
                    ),
                    BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
                        assert!(
                            outer.floating() || outer.integer(),
                            "a shader arithmetic operation applies to numbers, and not to the {}",
                            outer.name()
                        )
                    }
                    _ => {}
                }
                let left_ty = self.ty(*left);
                let right_ty = self.ty(*right);
                let result_ty = self.ty(*result);
                if *op == BinaryOp::Multiply
                    && (self.lanes(left_ty) == 1) != (self.lanes(right_ty) == 1)
                {
                    let vector = if self.lanes(left_ty) == 1 {
                        right_ty
                    } else {
                        left_ty
                    };
                    let scalar = if self.lanes(left_ty) == 1 {
                        left_ty
                    } else {
                        right_ty
                    };
                    assert_eq!(
                        self.lane_of(left_ty),
                        self.lane_of(right_ty),
                        "a shader product scales {} by {}",
                        element_name(self.module.ty(vector)),
                        element_name(self.module.ty(scalar))
                    );
                    assert_eq!(
                        result_ty,
                        vector,
                        "a shader product in {} of a {} and a {} yields {}",
                        self.function.name,
                        element_name(self.module.ty(vector)),
                        element_name(self.module.ty(scalar)),
                        element_name(self.module.ty(result_ty))
                    );
                    return;
                }
                if matches!(op, BinaryOp::ShiftLeft | BinaryOp::ShiftRight) {
                    self.integer(*right, "a shift");
                    assert_eq!(result_ty, left_ty, "a shader shift changes its type");
                    return;
                }
                assert_eq!(
                    left_ty,
                    right_ty,
                    "a shader binary operation mixes {} and {}",
                    element_name(self.module.ty(left_ty)),
                    element_name(self.module.ty(right_ty))
                );
                if op.comparison() {
                    assert_eq!(
                        self.lane(*result),
                        Scalar::Bool,
                        "a shader comparison yields {}",
                        element_name(self.module.ty(result_ty))
                    );
                    assert_eq!(
                        self.lanes(left_ty),
                        self.lanes(result_ty),
                        "a shader comparison changes its lane count"
                    );
                } else {
                    assert_eq!(
                        result_ty, left_ty,
                        "a shader binary operation changes its type"
                    );
                }
            }
            Instruction::Select {
                condition,
                accept,
                reject,
                ..
            } => {
                assert_eq!(
                    self.scalar(*condition, "a shader select"),
                    Scalar::Bool,
                    "a shader select is conditioned on a number"
                );
                assert_eq!(
                    self.ty(*accept),
                    self.ty(*reject),
                    "a shader select chooses between different types"
                );
            }
            Instruction::Convert { value, result } => {
                let source = self.ty(*value);
                let target = self.ty(*result);
                assert_eq!(
                    self.lanes(source),
                    self.lanes(target),
                    "a shader conversion changes its lane count"
                );
                assert!(
                    self.number(source) && self.number(target),
                    "a shader conversion mixes {} and {}",
                    element_name(self.module.ty(source)),
                    element_name(self.module.ty(target))
                );
            }
            Instruction::Bitcast { value, result } => assert_eq!(
                self.module.size(self.ty(*value)),
                self.module.size(self.ty(*result)),
                "a shader bitcast reinterprets a different width"
            ),
            Instruction::Math {
                fun,
                arguments,
                result,
            } => {
                assert_eq!(
                    arguments.len(),
                    fun.arity(),
                    "a shader {} takes {} arguments",
                    fun.name(),
                    fun.arity()
                );
                if *fun == MathFun::UnpackHalf2x16 {
                    assert_eq!(
                        self.ty(arguments[0]),
                        self.module.scalar_id(Scalar::U32),
                        "a shader half unpacking reads {}",
                        element_name(self.module.ty(self.ty(arguments[0])))
                    );
                    assert!(
                        matches!(
                            self.module.ty(self.ty(*result)),
                            Type::Vector {
                                scalar: Scalar::F32,
                                length: 2
                            }
                        ),
                        "a shader half unpacking yields {}",
                        element_name(self.module.ty(self.ty(*result)))
                    );
                } else {
                    for argument in arguments {
                        assert_eq!(
                            self.ty(*argument),
                            self.ty(arguments[0]),
                            "a shader {} mixes its argument types",
                            fun.name()
                        );
                    }
                    assert_eq!(
                        self.ty(*result),
                        self.ty(arguments[0]),
                        "a shader {} changes its type",
                        fun.name()
                    );
                    let scalar = self.scalar(*result, "a shader math function");
                    assert!(
                        scalar.floating() || matches!(fun, MathFun::Min | MathFun::Max),
                        "a shader {} applies to the {}",
                        fun.name(),
                        scalar.name()
                    );
                }
            }
            Instruction::Compose {
                constituents,
                result,
            } => {
                let Type::Vector { scalar, length } = self.module.ty(self.ty(*result)) else {
                    panic!(
                        "a shader composition builds {}",
                        element_name(self.module.ty(self.ty(*result)))
                    );
                };
                assert_eq!(
                    constituents.len() as u32,
                    *length,
                    "a shader composition of {length} lanes takes {length} values"
                );
                for constituent in constituents {
                    assert_eq!(
                        self.ty(*constituent),
                        self.module.scalar_id(*scalar),
                        "a shader composition mixes its component types"
                    );
                }
            }
            Instruction::Call {
                function,
                arguments,
                result,
            } => {
                let callee = &self.module.functions()[*function as usize];
                assert_eq!(
                    arguments.len(),
                    callee.arguments.len(),
                    "the shader call to {} takes {} arguments",
                    callee.name,
                    callee.arguments.len()
                );
                for (argument, parameter) in arguments.iter().zip(&callee.arguments) {
                    assert_eq!(
                        self.ty(*argument),
                        parameter.ty,
                        "the shader call to {} passes {} where {} stands",
                        callee.name,
                        element_name(self.module.ty(self.ty(*argument))),
                        element_name(self.module.ty(parameter.ty))
                    );
                }
                assert_eq!(
                    result.is_some(),
                    callee.returns_value(),
                    "the shader call to {} disagrees about its result",
                    callee.name
                );
                if let (Some(result), Some(expected)) = (result, callee.result) {
                    assert_eq!(
                        self.ty(*result),
                        expected,
                        "the shader call to {} yields {} where {} stands",
                        callee.name,
                        element_name(self.module.ty(self.ty(*result))),
                        element_name(self.module.ty(expected))
                    );
                }
            }
            Instruction::Atomic {
                op,
                pointer,
                value,
                result,
            } => {
                self.writable(*pointer, "a shader atomic");
                let (_, base) = self.pointee(*pointer, "an atomic operation");
                let Type::Atomic(scalar) = self.module.ty(base) else {
                    panic!(
                        "a shader {} operates on {} instead of an atomic",
                        op.name(),
                        element_name(self.module.ty(base))
                    );
                };
                assert!(
                    *op == AtomicOp::Exchange || scalar.integer(),
                    "a shader {} operates on the {}",
                    op.name(),
                    scalar.name()
                );
                assert_eq!(
                    self.ty(*value),
                    self.module.scalar_id(*scalar),
                    "a shader {} counts {}",
                    op.name(),
                    element_name(self.module.ty(self.ty(*value)))
                );
                assert_eq!(
                    self.ty(*result),
                    self.module.scalar_id(*scalar),
                    "a shader {} yields a different type",
                    op.name()
                );
            }
            Instruction::CompareExchange {
                pointer,
                expected,
                desired,
                result,
            } => {
                self.writable(*pointer, "a shader compare exchange");
                let (_, base) = self.pointee(*pointer, "a compare exchange");
                let Type::Atomic(scalar) = self.module.ty(base) else {
                    panic!(
                        "a shader compare exchange operates on {} instead of an atomic",
                        element_name(self.module.ty(base))
                    );
                };
                assert!(
                    scalar.integer(),
                    "a shader compare exchange operates on the {}",
                    scalar.name()
                );
                for operand in [expected, desired, result] {
                    assert_eq!(
                        self.ty(*operand),
                        self.module.scalar_id(*scalar),
                        "a shader compare exchange holds a different type",
                    );
                }
            }
            Instruction::WorkGroupUniformLoad { pointer, .. } => assert_eq!(
                self.pointee(*pointer, "a uniform load").0,
                Space::WorkGroup,
                "a shader uniform load reads memory no invocation shares"
            ),
            Instruction::Barrier(_) => {}
            Instruction::MatrixFill { value, result } => {
                let scalar = self.matrix_scalar(*result, "a matrix fill");
                assert_eq!(
                    self.ty(*value),
                    self.module.scalar_id(scalar),
                    "a shader matrix fill writes {}",
                    element_name(self.module.ty(self.ty(*value)))
                );
            }
            Instruction::MatrixLoad {
                pointer,
                stride,
                result,
                ..
            } => {
                let scalar = self.matrix_scalar(*result, "a matrix load");
                let (_, base) = self.pointee(*pointer, "a matrix load");
                let element = self.matrix_element(base, "a matrix load");
                assert_eq!(
                    element,
                    scalar,
                    "a shader matrix load reads {} where {} stands",
                    element.name(),
                    scalar.name()
                );
                self.integer(*stride, "a matrix stride");
            }
            Instruction::MatrixStore {
                pointer,
                value,
                stride,
                ..
            } => {
                let scalar = self.matrix_scalar(*value, "a matrix store");
                self.writable(*pointer, "a shader matrix store");
                let (_, base) = self.pointee(*pointer, "a matrix store");
                let element = self.matrix_element(base, "a matrix store");
                assert_eq!(
                    element,
                    scalar,
                    "a shader matrix store writes {} where {} stands",
                    scalar.name(),
                    element.name()
                );
                self.integer(*stride, "a matrix stride");
            }
            Instruction::MatrixMulAdd {
                left,
                right,
                accumulate,
                result,
            } => {
                let (scalar, rows, columns) = self.matrix(*result, "a matrix multiply");
                let (left_scalar, left_rows, left_columns) =
                    self.matrix(*left, "a matrix multiply");
                let (right_scalar, right_rows, right_columns) =
                    self.matrix(*right, "a matrix multiply");
                assert_eq!(
                    self.ty(*accumulate),
                    self.ty(*result),
                    "a shader matrix multiply accumulates into a different matrix"
                );
                assert_eq!(
                    left_rows, rows,
                    "a shader matrix multiply disagrees about its rows"
                );
                assert_eq!(
                    left_columns, right_rows,
                    "a shader matrix multiply walks a different depth"
                );
                assert_eq!(
                    right_columns, columns,
                    "a shader matrix multiply disagrees about its columns"
                );
                assert_eq!(
                    left_scalar,
                    right_scalar,
                    "a shader matrix multiply mixes {} and {} inputs",
                    left_scalar.name(),
                    right_scalar.name()
                );
                assert!(
                    scalar == left_scalar || scalar == Scalar::F32 && left_scalar == Scalar::F16,
                    "a shader matrix multiply accumulates {} inputs into {}",
                    left_scalar.name(),
                    scalar.name()
                );
            }
            Instruction::MatrixLength { ty, result } => {
                self.matrix_ty(*ty, "a matrix length");
                assert_eq!(
                    self.ty(*result),
                    self.module.scalar_id(Scalar::U32),
                    "a shader matrix length is not a word"
                );
            }
            Instruction::MatrixExtract {
                value,
                index,
                result,
            } => {
                let scalar = self.matrix_scalar(*value, "a matrix extract");
                self.integer(*index, "a matrix index");
                assert_eq!(
                    self.ty(*result),
                    self.module.scalar_id(scalar),
                    "a shader matrix extract yields {}",
                    element_name(self.module.ty(self.ty(*result)))
                );
            }
            Instruction::MatrixInsert {
                value,
                index,
                component,
                result,
            } => {
                let scalar = self.matrix_scalar(*value, "a matrix insert");
                self.integer(*index, "a matrix index");
                assert_eq!(
                    self.ty(*component),
                    self.module.scalar_id(scalar),
                    "a shader matrix insert takes {}",
                    element_name(self.module.ty(self.ty(*component)))
                );
                assert_eq!(
                    self.ty(*result),
                    self.ty(*value),
                    "a shader matrix insert changes its matrix"
                );
            }
            Instruction::If {
                condition,
                accept,
                reject,
            } => {
                assert_eq!(
                    self.scalar(*condition, "a shader condition"),
                    Scalar::Bool,
                    "a shader branch tests a number"
                );
                self.block(accept);
                self.block(reject);
            }
            Instruction::Switch {
                selector,
                cases,
                default,
            } => {
                self.integer(*selector, "a shader switch");
                let mut seen = HashSet::new();
                for (value, body) in cases {
                    assert!(
                        seen.insert(*value),
                        "a shader switch repeats the case {value}"
                    );
                    self.block(body);
                }
                self.block(default);
            }
            Instruction::Loop { body, continuing } => {
                self.loops += 1;
                self.block(body);
                self.block(continuing);
                self.loops -= 1;
            }
            Instruction::Break | Instruction::Continue => assert!(
                self.loops > 0,
                "a shader loop control stands outside a loop"
            ),
            Instruction::Return { value } => assert_eq!(
                value.map(|value| self.ty(value)),
                self.function.result,
                "the shader function {} returns a different type",
                self.function.name
            ),
            Instruction::Discard => assert!(
                !matches!(
                    self.module.stage_of(self.index),
                    Some(crate::shader::Stage::Vertex | crate::shader::Stage::Compute)
                ),
                "a shader discard belongs to the fragment stage"
            ),
            Instruction::Block(body) => self.block(body),
        }
    }

    fn expected(&self, instruction: &Instruction) -> Option<TypeId> {
        match instruction {
            Instruction::Unary { value, .. } => Some(self.ty(*value)),
            Instruction::Derivative { value, .. } => Some(self.ty(*value)),
            Instruction::Binary {
                op, left, right, ..
            } => {
                if op.comparison() {
                    Some(self.module.scalar_id(Scalar::Bool))
                } else if *op == BinaryOp::Multiply
                    && self.lanes(self.ty(*right)) > self.lanes(self.ty(*left))
                {
                    Some(self.ty(*right))
                } else {
                    Some(self.ty(*left))
                }
            }
            Instruction::Select { accept, .. } => Some(self.ty(*accept)),
            Instruction::Math { fun, arguments, .. } => {
                if *fun == MathFun::UnpackHalf2x16 {
                    None
                } else {
                    Some(self.ty(arguments[0]))
                }
            }
            Instruction::Load { pointer, .. } => Some(self.module.loaded_ty(self.ty(*pointer))),
            Instruction::AtomicLoad { pointer, .. } => {
                Some(self.module.loaded_ty(self.ty(*pointer)))
            }
            Instruction::Sample { result, .. } => Some(self.ty(*result)),
            Instruction::Atomic { pointer, .. } => Some(self.module.loaded_ty(self.ty(*pointer))),
            Instruction::CompareExchange { pointer, .. } => {
                Some(self.module.loaded_ty(self.ty(*pointer)))
            }
            Instruction::WorkGroupUniformLoad { pointer, .. } => {
                Some(self.module.loaded_ty(self.ty(*pointer)))
            }
            Instruction::Convert { result, .. } | Instruction::Bitcast { result, .. } => {
                Some(self.module.value_ty(*result))
            }
            _ => None,
        }
    }

    fn access_index(&self, base: ValueId, index: u32, instruction: &Instruction) {
        match self.module.ty(self.ty(base)) {
            Type::Pointer { space, base } => {
                let element = match self.module.ty(*base) {
                    Type::Array { element, .. } => *element,
                    Type::Struct { members, .. } => {
                        members
                            .get(index as usize)
                            .unwrap_or_else(|| panic!("a shader struct has no member {index}"))
                            .ty
                    }
                    Type::Vector { scalar, .. } => self.module.scalar_id(*scalar),
                    other => panic!("a shader indexes {}", element_name(other)),
                };
                self.expect_pointer(instruction, *space, element);
            }
            Type::Vector { scalar, length } => {
                assert!(index < *length, "a shader vector has no lane {index}");
                assert_eq!(
                    self.module
                        .value_ty(instruction.result().expect("an access has a result")),
                    self.module.scalar_id(*scalar),
                    "a shader lane access yields a different type"
                );
            }
            Type::Struct { members, .. } => {
                let member = members
                    .get(index as usize)
                    .unwrap_or_else(|| panic!("a shader struct has no member {index}"));
                assert_eq!(
                    self.module
                        .value_ty(instruction.result().expect("an access has a result")),
                    member.ty,
                    "a shader member access yields a different type"
                );
            }
            other => panic!("a shader indexes {}", element_name(other)),
        }
    }

    fn address(&self, address: Address) {
        match address {
            Address::Global(index) => {
                assert!(
                    (index as usize) < self.module.globals().len(),
                    "a shader instruction addresses an undeclared global"
                );
                let global = self.module.global(index);
                assert!(
                    !global.space.interface() || self.module.is_entry(self.index),
                    "the shader function {} reads the {} interface variable {}",
                    self.function.name,
                    global.space.name(),
                    global.name
                );
            }
            Address::Local(index) => assert!(
                (index as usize) < self.function.locals.len(),
                "a shader instruction addresses an undeclared local"
            ),
        }
    }

    fn expect_pointer(&self, instruction: &Instruction, space: Space, base: TypeId) {
        let result = self
            .module
            .value_ty(instruction.result().expect("an access has a result"));
        assert_eq!(
            self.module.ty(result),
            &Type::Pointer { space, base },
            "a shader access does not reach into its base"
        );
    }

    fn writable(&self, pointer: ValueId, subject: &str) {
        let Some(index) = self.root(pointer) else {
            return;
        };
        let global = self.module.global(index);
        assert!(
            global.access.writable(),
            "{subject} writes into the read-only shader global {}",
            global.name
        );
    }

    fn root(&self, value: ValueId) -> Option<u32> {
        match self.declarations.get(&value).copied()? {
            Instruction::Address {
                address: Address::Global(index),
                ..
            } => Some(*index),
            Instruction::Access { base, .. } | Instruction::AccessIndex { base, .. } => {
                self.root(*base)
            }
            _ => None,
        }
    }

    fn matrix(&self, value: ValueId, subject: &str) -> (Scalar, u32, u32) {
        self.matrix_ty(self.ty(value), subject)
    }

    fn matrix_element(&self, ty: TypeId, subject: &str) -> Scalar {
        match self.module.ty(ty) {
            Type::Scalar(scalar) => *scalar,
            Type::Vector { scalar, .. } => *scalar,
            other => panic!("{subject} reads {}", element_name(other)),
        }
    }

    fn matrix_ty(&self, ty: TypeId, subject: &str) -> (Scalar, u32, u32) {
        match self.module.ty(ty) {
            Type::CooperativeMatrix {
                scalar,
                rows,
                columns,
                ..
            } => (*scalar, *rows, *columns),
            other => panic!(
                "{subject} operates on {} instead of a cooperative matrix",
                element_name(other)
            ),
        }
    }

    fn matrix_scalar(&self, value: ValueId, subject: &str) -> Scalar {
        self.matrix(value, subject).0
    }

    fn pointee(&self, pointer: ValueId, subject: &str) -> (Space, TypeId) {
        match self.module.ty(self.ty(pointer)) {
            Type::Pointer { space, base } => (*space, *base),
            other => panic!("{subject} dereferences {}", element_name(other)),
        }
    }

    fn ty(&self, value: ValueId) -> TypeId {
        self.module.value_ty(value)
    }

    fn scalar(&self, value: ValueId, subject: &str) -> Scalar {
        let ty = self.ty(value);
        self.module.ty(ty).scalar().unwrap_or_else(|| {
            panic!(
                "{subject} operates on {} instead of a scalar",
                element_name(self.module.ty(ty))
            )
        })
    }

    fn index(&self, value: ValueId) {
        let ty = self.ty(value);
        assert!(
            matches!(self.module.ty(ty), Type::Scalar(Scalar::U32 | Scalar::I32)),
            "a shader index is {}",
            element_name(self.module.ty(ty))
        );
    }

    fn integer(&self, value: ValueId, subject: &str) {
        let scalar = self.lane(value);
        assert!(scalar.integer(), "{subject} counts the {}", scalar.name());
    }

    fn lane(&self, value: ValueId) -> Scalar {
        self.lane_of(self.ty(value))
    }

    fn lane_of(&self, ty: TypeId) -> Scalar {
        match self.module.ty(ty) {
            Type::Scalar(scalar) => *scalar,
            Type::Vector { scalar, .. } => *scalar,
            other => panic!("a shader operand is {}", element_name(other)),
        }
    }

    fn number(&self, ty: TypeId) -> bool {
        matches!(
            self.module.ty(ty),
            Type::Scalar(Scalar::U32 | Scalar::I32 | Scalar::F32 | Scalar::F16)
                | Type::Vector { .. }
        )
    }

    fn lanes(&self, ty: TypeId) -> u32 {
        match self.module.ty(ty) {
            Type::Scalar(_) => 1,
            Type::Vector { length, .. } => *length,
            other => panic!("{} is not a shader number", element_name(other)),
        }
    }
}

fn collect<'m>(block: &'m [Instruction], declarations: &mut HashMap<ValueId, &'m Instruction>) {
    for instruction in block {
        if let Some(result) = instruction.result() {
            declarations.insert(result, instruction);
        }
        match instruction {
            Instruction::If { accept, reject, .. } => {
                collect(accept, declarations);
                collect(reject, declarations);
            }
            Instruction::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect(body, declarations);
                }
                collect(default, declarations);
            }
            Instruction::Loop { body, continuing } => {
                collect(body, declarations);
                collect(continuing, declarations);
            }
            Instruction::Block(body) => collect(body, declarations),
            _ => {}
        }
    }
}

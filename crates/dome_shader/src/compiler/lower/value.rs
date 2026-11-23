use super::{FunctionLower, Symbol, Typed};
use crate::ast;
use crate::compiler::intrinsic::Intrinsic;
use crate::compiler::{Compiler, ShaderInstruction};
use crate::shader::{
    AtomicOp, BinaryOp, Constant, DerivativeOp, MatrixLayout, MatrixUse, Scalar, Type, TypeId,
    UnaryOp,
};

impl Compiler {
    pub(crate) fn rust_type(&mut self, ty: &ast::Type) -> TypeId {
        match ty {
            ast::Type::Named(name) => match name.as_str() {
                "f16" => self.module_mut().scalar(Scalar::F16),
                "coopmat_a" => self.cooperative(MatrixUse::A),
                "coopmat_b" => self.cooperative(MatrixUse::B),
                "coopmat_accumulator" => self.cooperative(MatrixUse::Accumulator),
                other => self.ty(other),
            },
            ast::Type::Array { element, length } => {
                let element = self.rust_type(element);
                let count = self.evaluate(length);
                self.array(element, count)
            }
        }
    }

    fn cooperative(&mut self, usage: MatrixUse) -> TypeId {
        let rows = self.constant_u32("COOPMAT_ROWS");
        let columns = self.constant_u32("COOPMAT_COLUMNS");
        let depth = self.constant_u32("COOPMAT_DEPTH");
        let (scalar, rows, columns) = match usage {
            MatrixUse::A => (Scalar::F16, rows, depth),
            MatrixUse::B => (Scalar::F16, depth, columns),
            MatrixUse::Accumulator => (Scalar::F32, rows, columns),
        };
        self.module_mut()
            .cooperative_matrix(scalar, rows, columns, usage)
    }

    pub(crate) fn constant_u32(&self, name: &str) -> u32 {
        self.constant_value(name)
            .unwrap_or_else(|| panic!("the shader constant {name} is not defined"))
    }

    pub(crate) fn evaluate(&self, expr: &ast::Expression) -> u32 {
        use crate::ast::{BinaryOperator as Op, Expression as E, IntegerType};
        match expr {
            E::Integer { value, ty } if !matches!(ty, IntegerType::Signed) => *value,
            E::Name(name) => self.constant_u32(name),
            E::Binary { op, left, right } => {
                let left = self.evaluate(left);
                let right = self.evaluate(right);
                match op {
                    Op::Add => left.checked_add(right).expect("shader constant overflows"),
                    Op::Subtract => left.checked_sub(right).expect("shader constant underflows"),
                    Op::Multiply => left.checked_mul(right).expect("shader constant overflows"),
                    Op::Divide => left
                        .checked_div(right)
                        .expect("shader constant divides by zero"),
                    Op::Modulo => left
                        .checked_rem(right)
                        .expect("shader constant divides by zero"),
                    _ => panic!("unsupported shader constant expression"),
                }
            }
            other => panic!("a shader constant must be known at compilation: {other:?}"),
        }
    }

    pub(crate) fn member(&self, ty: TypeId, field: &str) -> (u32, TypeId) {
        match self.module().ty(ty) {
            Type::Struct { name, members, .. } => members
                .iter()
                .enumerate()
                .find(|(_, member)| member.name == field)
                .map(|(index, member)| (index as u32, member.ty))
                .unwrap_or_else(|| panic!("Rust shader struct {name} has no field {field}")),
            other => panic!(
                "a field of {} is not accessible",
                crate::shader::element_name(other)
            ),
        }
    }
}

impl FunctionLower<'_> {
    fn literal(&mut self, value: u32, kind: ast::IntegerType, hint: Option<TypeId>) -> Typed {
        let signed = matches!(kind, ast::IntegerType::Signed)
            || matches!(kind, ast::IntegerType::Inferred)
                && hint.is_some_and(|ty| ty == self.compiler.scalar("i32"));
        let ty = if signed {
            self.compiler.scalar("i32")
        } else {
            self.compiler.scalar("u32")
        };
        let constant = if signed {
            Constant::I32(i32::try_from(value).expect("a signed shader integer fits in i32"))
        } else {
            Constant::U32(value)
        };
        let value = self.compiler.module_mut().constant(constant);
        Typed { value, ty }
    }

    fn field(&mut self, base: Typed, name: &str) -> Typed {
        match self.compiler.module().ty(base.ty).clone() {
            Type::Struct { .. } => {
                let (index, ty) = self.compiler.member(base.ty, name);
                self.emit(ty, |result| ShaderInstruction::AccessIndex {
                    base: base.value,
                    index,
                    result,
                })
            }
            Type::Vector { scalar, length } => {
                let index = match name {
                    "x" => 0,
                    "y" => 1,
                    "z" => 2,
                    "w" => 3,
                    _ => panic!("Rust shader vector has no component {name}"),
                };
                assert!(index < length, "a shader vector has no {name} lane");
                let ty = self.compiler.module_mut().scalar(scalar);
                self.emit(ty, |result| ShaderInstruction::AccessIndex {
                    base: base.value,
                    index,
                    result,
                })
            }
            other => panic!(
                "a field of {} is not accessible",
                crate::shader::element_name(&other)
            ),
        }
    }

    fn element_type(&self, ty: TypeId) -> TypeId {
        match self.compiler.module().ty(ty).clone() {
            Type::Array { element, .. } | Type::Block { element, .. } => element,
            Type::Vector { scalar, .. } => self
                .compiler
                .module()
                .scalar_type(scalar)
                .expect("a shader vector lane exists"),
            other => panic!(
                "a Rust shader indexes {} instead of an array or vector",
                crate::shader::element_name(&other)
            ),
        }
    }

    fn indexed(&mut self, base: Typed, index: &ast::Expression) -> Typed {
        let element = self.element_type(base.ty);
        if matches!(self.compiler.module().ty(base.ty), Type::Vector { .. }) {
            let index = self
                .evaluate_u32(index)
                .expect("a shader vector requires a compile-time lane index");
            let length = match self.compiler.module().ty(base.ty) {
                Type::Vector { length, .. } => *length,
                _ => unreachable!(),
            };
            assert!(index < length, "a vector index lies inside its lanes");
            self.emit(element, |result| ShaderInstruction::AccessIndex {
                base: base.value,
                index,
                result,
            })
        } else {
            let index = self.value(index);
            self.emit(element, |result| ShaderInstruction::Access {
                base: base.value,
                index: index.value,
                result,
            })
        }
    }

    pub(super) fn place(&mut self, expr: &ast::Expression) -> Option<Typed> {
        use crate::ast::Expression as E;
        match expr {
            E::Name(name) => match self.lookup(name) {
                Some(Symbol::Local(local)) => Some(local),
                Some(Symbol::Value(_) | Symbol::Array(_) | Symbol::Constant(_)) => None,
                None if self.compiler.declares_global(name) => {
                    let global = self.compiler.workgroup_global(name);
                    let ty = self.compiler.module().global(global).ty;
                    let space = self.compiler.module().global(global).space;
                    let pointer = self.compiler.pointer(space, ty);
                    Some(self.emit(pointer, |result| ShaderInstruction::Address {
                        address: crate::shader::Address::Global(global),
                        result,
                    }))
                }
                None => panic!("Rust shader name {name} is not defined"),
            },
            E::Field { base, name } => {
                let base = self.place(base)?;
                let pointee = pointee(self, base.ty);
                let space = pointer_space(self, base.ty);
                let (index, ty) = match self.compiler.module().ty(pointee).clone() {
                    Type::Vector { scalar, length } => {
                        let index = match name.as_str() {
                            "x" => 0,
                            "y" => 1,
                            "z" => 2,
                            "w" => 3,
                            _ => panic!("Rust shader vector has no component {name}"),
                        };
                        assert!(index < length, "a shader vector has no {name} lane");
                        (index, self.compiler.module_mut().scalar(scalar))
                    }
                    _ => self.compiler.member(pointee, name),
                };
                let pointer = self.compiler.pointer(space, ty);
                Some(self.emit(pointer, |result| ShaderInstruction::AccessIndex {
                    base: base.value,
                    index,
                    result,
                }))
            }
            E::Index { base, index } => {
                if let E::Name(name) = base.as_ref()
                    && let Some(Symbol::Array(registers)) = self.lookup(name)
                {
                    let offset = self
                        .evaluate_u32(index)
                        .expect("a scalar register requires a compile-time index");
                    return Some(
                        *registers
                            .get(offset as usize)
                            .expect("a scalar register index lies inside its array"),
                    );
                }
                let base = self.place(base)?;
                let space = pointer_space(self, base.ty);
                let pointee = pointee(self, base.ty);
                let element = self.element_type(pointee);
                let pointer = self.compiler.pointer(space, element);
                if matches!(self.compiler.module().ty(pointee), Type::Vector { .. }) {
                    let offset = self
                        .evaluate_u32(index)
                        .expect("a shader vector requires a compile-time lane index");
                    return Some(self.emit(pointer, |result| ShaderInstruction::AccessIndex {
                        base: base.value,
                        index: offset,
                        result,
                    }));
                }
                let index = self.value(index);
                Some(self.emit(pointer, |result| ShaderInstruction::Access {
                    base: base.value,
                    index: index.value,
                    result,
                }))
            }
            _ => None,
        }
    }

    pub(super) fn value(&mut self, expr: &ast::Expression) -> Typed {
        self.value_with_hint(expr, None)
    }

    pub(super) fn value_with_hint(
        &mut self,
        expr: &ast::Expression,
        hint: Option<TypeId>,
    ) -> Typed {
        let value = self.value_of(expr, hint);
        if let Some(ty) = hint {
            assert_eq!(value.ty, ty, "shader operand has a different type");
        }
        value
    }

    fn value_of(&mut self, expr: &ast::Expression, hint: Option<TypeId>) -> Typed {
        use crate::ast::Expression as E;
        if matches!(expr, E::Name(_) | E::Field { .. } | E::Index { .. })
            && let Some(place) = self.place(expr)
        {
            let ty = self.compiler.module().loaded_ty(place.ty);
            return self.emit(ty, |result| ShaderInstruction::Load {
                pointer: place.value,
                result,
            });
        }
        match expr {
            E::Integer { value, ty } => self.literal(*value, *ty, hint),
            E::Float(value) => {
                let ty = self.compiler.scalar("f32");
                let value = self.compiler.module_mut().constant(Constant::F32(*value));
                Typed { value, ty }
            }
            E::Bool(value) => {
                let ty = self.compiler.scalar("bool");
                let value = self.compiler.module_mut().constant(Constant::Bool(*value));
                Typed { value, ty }
            }
            E::Name(name) => match self.lookup(name) {
                Some(Symbol::Value(value)) => value,
                Some(Symbol::Constant(value)) => {
                    let ty = self.compiler.scalar("u32");
                    let value = self.compiler.module_mut().constant(Constant::U32(value));
                    Typed { value, ty }
                }
                Some(Symbol::Array(_)) => {
                    panic!("a scalar register array is accessed by constant index")
                }
                Some(Symbol::Local(_)) => unreachable!("a local is a shader place"),
                None => panic!("Rust shader name {name} is not defined"),
            },
            E::Field { base, name } => {
                let base = self.value(base);
                self.field(base, name)
            }
            E::Index { base, index } => {
                let base = self.value(base);
                self.indexed(base, index)
            }
            E::Unary { op, value } => {
                let arg = self.value(value);
                let scalar = self.scalar_of(arg.ty);
                let op = match op {
                    ast::UnaryOperator::Negate => UnaryOp::Negate,
                    ast::UnaryOperator::Not if scalar == Scalar::Bool => UnaryOp::LogicalNot,
                    ast::UnaryOperator::Not => UnaryOp::BitwiseNot,
                };
                self.emit(arg.ty, |result| ShaderInstruction::Unary {
                    op,
                    value: arg.value,
                    result,
                })
            }
            E::Binary { op, left, right } => self.binary(*op, left, right),
            E::Call { name, arguments } => self.call(name, arguments),
            E::Repeat { value, length } => {
                let count = self.compiler.evaluate(length);
                let value = self.value(value);
                assert!(
                    matches!(
                        self.compiler.module().constant_of(value.value),
                        Some(Constant::F32(0.0)) | Some(Constant::U32(0))
                    ),
                    "a shader array starts from zero"
                );
                let ty = self.compiler.array(value.ty, count);
                let value = self.compiler.module_mut().constant(Constant::Zero(ty));
                Typed { value, ty }
            }
            E::Cast { value, ty } => {
                let source = self.value(value);
                let target = self.rust_type(ty);
                self.emit(target, |result| ShaderInstruction::Convert {
                    value: source.value,
                    result,
                })
            }
            E::Reference(_) => panic!("a shader reference is only passed to an atomic store"),
        }
    }

    pub(super) fn scalar_of(&self, ty: TypeId) -> Scalar {
        self.compiler
            .module()
            .ty(ty)
            .scalar()
            .unwrap_or_else(|| panic!("a shader operand is not a scalar"))
    }

    fn lanes(&self, ty: TypeId) -> u32 {
        match self.compiler.module().ty(ty) {
            Type::Vector { length, .. } => *length,
            _ => 1,
        }
    }

    fn vector_element(&self, ty: TypeId) -> Option<TypeId> {
        match self.compiler.module().ty(ty) {
            Type::Vector { scalar, .. } => self.compiler.module().scalar_type(*scalar),
            _ => None,
        }
    }

    pub(super) fn evaluate_u32(&self, expr: &ast::Expression) -> Option<u32> {
        use crate::ast::{BinaryOperator as Op, Expression as E, IntegerType};
        match expr {
            E::Integer { value, ty } if !matches!(ty, IntegerType::Signed) => Some(*value),
            E::Name(name) => match self.lookup(name)? {
                Symbol::Constant(value) => Some(value),
                _ => None,
            },
            E::Binary { op, left, right } => {
                let left = self.evaluate_u32(left)?;
                let right = self.evaluate_u32(right)?;
                match op {
                    Op::Add => Some(left.checked_add(right).expect("shader constant overflows")),
                    Op::Subtract => {
                        Some(left.checked_sub(right).expect("shader constant underflows"))
                    }
                    Op::Multiply => {
                        Some(left.checked_mul(right).expect("shader constant overflows"))
                    }
                    Op::Divide => Some(
                        left.checked_div(right)
                            .expect("shader constant divides by zero"),
                    ),
                    Op::Modulo => Some(
                        left.checked_rem(right)
                            .expect("shader constant divides by zero"),
                    ),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn binary(
        &mut self,
        operator: ast::BinaryOperator,
        left: &ast::Expression,
        right: &ast::Expression,
    ) -> Typed {
        use crate::ast::BinaryOperator as Op;
        let op = match operator {
            Op::Add => BinaryOp::Add,
            Op::Subtract => BinaryOp::Subtract,
            Op::Multiply => BinaryOp::Multiply,
            Op::Divide => BinaryOp::Divide,
            Op::Modulo => BinaryOp::Modulo,
            Op::Equal => BinaryOp::Equal,
            Op::NotEqual => BinaryOp::NotEqual,
            Op::Less => BinaryOp::Less,
            Op::LessEqual => BinaryOp::LessEqual,
            Op::Greater => BinaryOp::Greater,
            Op::GreaterEqual => BinaryOp::GreaterEqual,
            Op::BitAnd => BinaryOp::And,
            Op::BitOr => BinaryOp::Or,
            Op::BitXor => BinaryOp::Xor,
            Op::LogicalAnd => BinaryOp::LogicalAnd,
            Op::LogicalOr => BinaryOp::LogicalOr,
            Op::ShiftLeft => BinaryOp::ShiftLeft,
            Op::ShiftRight => BinaryOp::ShiftRight,
        };
        if op.logical() {
            let boolean = self.compiler.scalar("bool");
            let left = self.value_with_hint(left, Some(boolean));
            let name = format!("branch{}", self.function.locals.len());
            let local = self.declare_local(&name, boolean);
            self.push(ShaderInstruction::Store {
                pointer: local.value,
                value: left.value,
            });
            let branch = self.block(|lower| {
                let right = lower.value_with_hint(right, Some(boolean));
                lower.push(ShaderInstruction::Store {
                    pointer: local.value,
                    value: right.value,
                });
            });
            let empty = Vec::new();
            let (accept, reject) = if op == BinaryOp::LogicalAnd {
                (branch, empty)
            } else {
                (empty, branch)
            };
            self.push(ShaderInstruction::If {
                condition: left.value,
                accept,
                reject,
            });
            return self.emit(boolean, |result| ShaderInstruction::Load {
                pointer: local.value,
                result,
            });
        }
        let left = self.value(left);
        let right = if op == BinaryOp::Multiply {
            self.factor(right, left.ty)
        } else if matches!(op, BinaryOp::ShiftLeft | BinaryOp::ShiftRight) {
            self.value_with_hint(right, Some(self.compiler.scalar("u32")))
        } else {
            self.value_with_hint(right, Some(left.ty))
        };
        let ty = if op.comparison() {
            self.compiler.scalar("bool")
        } else if op == BinaryOp::Multiply && self.lanes(right.ty) > self.lanes(left.ty) {
            right.ty
        } else {
            left.ty
        };
        self.emit(ty, |result| ShaderInstruction::Binary {
            op,
            left: left.value,
            right: right.value,
            result,
        })
    }

    fn factor(&mut self, expr: &ast::Expression, left: TypeId) -> Typed {
        if self.lanes(left) > 1 {
            return self.value(expr);
        }
        let factor = self.value_of(expr, Some(left));
        assert!(
            factor.ty == left || self.vector_element(factor.ty) == Some(left),
            "a shader product scales a {} by a {}",
            crate::shader::element_name(self.compiler.module().ty(left)),
            crate::shader::element_name(self.compiler.module().ty(factor.ty)),
        );
        factor
    }

    pub(super) fn reference(&mut self, argument: &ast::Expression, subject: &str) -> Typed {
        let ast::Expression::Reference(reference) = argument else {
            panic!("{subject} takes a reference");
        };
        self.place(reference)
            .unwrap_or_else(|| panic!("{subject} refers to a writable shader place"))
    }

    pub(super) fn atomic_cell(&self, pointer: Typed, subject: &str) -> TypeId {
        let (_, base) = self.compiler.module().pointee(pointer.ty);
        let Type::Atomic(scalar) = self.compiler.module().ty(base) else {
            panic!(
                "{subject} refers to {} instead of an atomic cell",
                crate::shader::element_name(self.compiler.module().ty(base)),
            );
        };
        self.compiler
            .module()
            .scalar_type(*scalar)
            .expect("an atomic cell holds a declared scalar")
    }

    fn call(&mut self, name: &str, args: &[ast::Expression]) -> Typed {
        match Intrinsic::of(name) {
            Some(intrinsic) => {
                assert!(
                    intrinsic.yields(),
                    "the shader intrinsic {name} writes without yielding a value",
                );
                let args = intrinsic_arguments(intrinsic, args);
                self.intrinsic(intrinsic, args)
            }
            None => self.shader_function(name, args),
        }
    }

    fn intrinsic(&mut self, intrinsic: Intrinsic, args: &[ast::Expression]) -> Typed {
        match intrinsic {
            Intrinsic::Uvec4 => {
                let ty = self.compiler.scalar("u32");
                let constituents = args
                    .iter()
                    .map(|arg| self.value_with_hint(arg, Some(ty)).value)
                    .collect::<Vec<_>>();
                let ty = self.compiler.ty("uvec4");
                self.emit(ty, |result| ShaderInstruction::Compose {
                    constituents,
                    result,
                })
            }
            Intrinsic::Fvec2 | Intrinsic::Fvec4 => {
                let ty = self.compiler.scalar("f32");
                let constituents = args
                    .iter()
                    .map(|arg| self.value_with_hint(arg, Some(ty)).value)
                    .collect::<Vec<_>>();
                let ty = self.compiler.ty(intrinsic.name());
                self.emit(ty, |result| ShaderInstruction::Compose {
                    constituents,
                    result,
                })
            }
            Intrinsic::Select => {
                let reject = self.value(&args[0]);
                let accept = self.value_with_hint(&args[1], Some(reject.ty));
                let condition = self.value_with_hint(&args[2], Some(self.compiler.scalar("bool")));
                self.emit(reject.ty, |result| ShaderInstruction::Select {
                    condition: condition.value,
                    accept: accept.value,
                    reject: reject.value,
                    result,
                })
            }
            Intrinsic::BitcastU32 => {
                let source = self.value_with_hint(&args[0], Some(self.compiler.scalar("f32")));
                let ty = self.compiler.scalar("u32");
                self.emit(ty, |result| ShaderInstruction::Bitcast {
                    value: source.value,
                    result,
                })
            }
            Intrinsic::BitcastF32 => {
                let source = self.value_with_hint(&args[0], Some(self.compiler.scalar("u32")));
                let ty = self.compiler.scalar("f32");
                self.emit(ty, |result| ShaderInstruction::Bitcast {
                    value: source.value,
                    result,
                })
            }
            Intrinsic::F32 | Intrinsic::U32 | Intrinsic::I32 | Intrinsic::F16 => {
                let source = self.value(&args[0]);
                let ty = match intrinsic {
                    Intrinsic::F16 => self.compiler.module_mut().scalar(Scalar::F16),
                    _ => self.compiler.ty(intrinsic.name()),
                };
                self.emit(ty, |result| ShaderInstruction::Convert {
                    value: source.value,
                    result,
                })
            }
            Intrinsic::Sample => {
                assert_eq!(
                    self.stage,
                    Some(crate::shader::Stage::Fragment),
                    "implicit level of detail sampling belongs to the fragment stage"
                );
                let image = self.value(&args[0]);
                let sampler = self.value(&args[1]);
                let coordinates_type = self.compiler.ty("fvec2");
                let coordinates = self.value_with_hint(&args[2], Some(coordinates_type));
                let ty = self.compiler.ty("fvec4");
                self.emit(ty, |result| ShaderInstruction::Sample {
                    image: image.value,
                    sampler: sampler.value,
                    coordinates: coordinates.value,
                    level: None,
                    result,
                })
            }
            Intrinsic::SampleLod => {
                assert_eq!(
                    self.stage,
                    Some(crate::shader::Stage::Fragment),
                    "explicit level of detail sampling belongs to the fragment stage"
                );
                let image = self.value(&args[0]);
                let sampler = self.value(&args[1]);
                let coordinates_type = self.compiler.ty("fvec2");
                let coordinates = self.value_with_hint(&args[2], Some(coordinates_type));
                let level_type = self.compiler.scalar("f32");
                let level = self.value_with_hint(&args[3], Some(level_type));
                let ty = self.compiler.ty("fvec4");
                self.emit(ty, |result| ShaderInstruction::Sample {
                    image: image.value,
                    sampler: sampler.value,
                    coordinates: coordinates.value,
                    level: Some(level.value),
                    result,
                })
            }
            Intrinsic::SampleCompare => {
                assert_eq!(
                    self.stage,
                    Some(crate::shader::Stage::Fragment),
                    "comparison sampling belongs to the fragment stage"
                );
                let depth = self.value(&args[0]);
                let sampler = self.value(&args[1]);
                let coordinates_type = self.compiler.ty("fvec2");
                let coordinates = self.value_with_hint(&args[2], Some(coordinates_type));
                let reference_type = self.compiler.scalar("f32");
                let reference = self.value_with_hint(&args[3], Some(reference_type));
                let ty = self.compiler.scalar("f32");
                self.emit(ty, |result| ShaderInstruction::SampleCompare {
                    depth: depth.value,
                    sampler: sampler.value,
                    coordinates: coordinates.value,
                    reference: reference.value,
                    result,
                })
            }
            Intrinsic::Dpdx | Intrinsic::Dpdy => {
                assert_eq!(
                    self.stage,
                    Some(crate::shader::Stage::Fragment),
                    "a shader derivative belongs to the fragment stage"
                );
                let source = self.value(&args[0]);
                let op = match intrinsic {
                    Intrinsic::Dpdx => DerivativeOp::X,
                    Intrinsic::Dpdy => DerivativeOp::Y,
                    _ => unreachable!(),
                };
                self.emit(source.ty, |result| ShaderInstruction::Derivative {
                    op,
                    value: source.value,
                    result,
                })
            }
            Intrinsic::UnpackHalf2x16 => {
                let source = self.value_with_hint(&args[0], Some(self.compiler.scalar("u32")));
                let ty = self.compiler.ty("fvec2");
                self.emit(ty, |result| ShaderInstruction::Math {
                    fun: crate::shader::MathFun::UnpackHalf2x16,
                    arguments: vec![source.value],
                    result,
                })
            }
            Intrinsic::Max
            | Intrinsic::Min
            | Intrinsic::Abs
            | Intrinsic::Sqrt
            | Intrinsic::Exp
            | Intrinsic::Log
            | Intrinsic::Tanh
            | Intrinsic::Trunc
            | Intrinsic::Sin
            | Intrinsic::Cos
            | Intrinsic::Acos
            | Intrinsic::Atan2
            | Intrinsic::Pow
            | Intrinsic::Floor => {
                let fun = match intrinsic {
                    Intrinsic::Max => crate::shader::MathFun::Max,
                    Intrinsic::Min => crate::shader::MathFun::Min,
                    Intrinsic::Abs => crate::shader::MathFun::Abs,
                    Intrinsic::Sqrt => crate::shader::MathFun::Sqrt,
                    Intrinsic::Exp => crate::shader::MathFun::Exp,
                    Intrinsic::Log => crate::shader::MathFun::Log,
                    Intrinsic::Tanh => crate::shader::MathFun::Tanh,
                    Intrinsic::Trunc => crate::shader::MathFun::Trunc,
                    Intrinsic::Sin => crate::shader::MathFun::Sin,
                    Intrinsic::Cos => crate::shader::MathFun::Cos,
                    Intrinsic::Acos => crate::shader::MathFun::Acos,
                    Intrinsic::Atan2 => crate::shader::MathFun::Atan2,
                    Intrinsic::Pow => crate::shader::MathFun::Pow,
                    Intrinsic::Floor => crate::shader::MathFun::Floor,
                    _ => unreachable!(),
                };
                let first = self.value(&args[0]);
                let mut arguments = vec![first.value];
                if let Some(second) = args.get(1) {
                    arguments.push(self.value_with_hint(second, Some(first.ty)).value);
                }
                self.emit(first.ty, |result| ShaderInstruction::Math {
                    fun,
                    arguments,
                    result,
                })
            }
            Intrinsic::AtomicLoad => {
                let pointer = self.reference(&args[0], "an atomic load");
                let ty = self.atomic_cell(pointer, "an atomic load");
                self.emit(ty, |result| ShaderInstruction::AtomicLoad {
                    pointer: pointer.value,
                    result,
                })
            }
            Intrinsic::AtomicAdd
            | Intrinsic::AtomicSub
            | Intrinsic::AtomicExchange
            | Intrinsic::AtomicCompareExchange => self.atomic(intrinsic, args),
            Intrinsic::WorkgroupUniformLoad => self.workgroup_uniform_load(args),
            Intrinsic::CoopmatAccumulator => {
                let value = self.value_with_hint(&args[0], Some(self.compiler.scalar("f32")));
                let ty = self.coopmat(ast::Type::Named("coopmat_accumulator".to_owned()));
                self.emit(ty, |result| ShaderInstruction::MatrixFill {
                    value: value.value,
                    result,
                })
            }
            Intrinsic::CoopmatLoadRow => {
                self.matrix_load(args, MatrixLayout::RowMajor, MatrixUse::A)
            }
            Intrinsic::CoopmatLoadBRow => {
                self.matrix_load(args, MatrixLayout::RowMajor, MatrixUse::B)
            }
            Intrinsic::CoopmatLoadColumn => {
                self.matrix_load(args, MatrixLayout::ColumnMajor, MatrixUse::B)
            }
            Intrinsic::CoopmatMuladd => {
                let accumulate = self.value(&args[2]);
                let left = self.value_with_hint(&args[0], None);
                let right = self.value_with_hint(&args[1], None);
                self.emit(accumulate.ty, |result| ShaderInstruction::MatrixMulAdd {
                    left: left.value,
                    right: right.value,
                    accumulate: accumulate.value,
                    result,
                })
            }
            Intrinsic::ScalarArray => {
                panic!("a Rust shader register array is declared by a let binding")
            }
            Intrinsic::AtomicStore
            | Intrinsic::Discard
            | Intrinsic::WorkgroupBarrier
            | Intrinsic::StorageBarrier
            | Intrinsic::CoopmatStore => {
                unreachable!("a shader synchronization, store or discard yields no value")
            }
        }
    }

    fn shader_function(&mut self, name: &str, args: &[ast::Expression]) -> Typed {
        let function = self.compiler.lower_callee(name);
        let (parameters, result) = {
            let callee = &self.compiler.module().functions()[function as usize];
            (
                callee
                    .arguments
                    .iter()
                    .map(|argument| argument.ty)
                    .collect::<Vec<_>>(),
                callee.result,
            )
        };
        assert_eq!(
            args.len(),
            parameters.len(),
            "{name} takes a fixed number of arguments"
        );
        let arguments = args
            .iter()
            .zip(parameters)
            .map(|(arg, ty)| self.value_with_hint(arg, Some(ty)).value)
            .collect();
        let ty = result.expect("a void function cannot be used as a value");
        self.emit(ty, |value| ShaderInstruction::Call {
            function,
            arguments,
            result: Some(value),
        })
    }

    fn coopmat(&mut self, ty: ast::Type) -> TypeId {
        self.compiler.rust_type(&ty)
    }

    fn matrix_load(
        &mut self,
        args: &[ast::Expression],
        layout: MatrixLayout,
        usage: MatrixUse,
    ) -> Typed {
        let pointer = self.reference(
            &args[0],
            if matches!(layout, MatrixLayout::RowMajor) {
                "a shader matrix load"
            } else {
                "a shader matrix transpose"
            },
        );
        let element = pointee(self, pointer.ty);
        let ty = match self.compiler.module().ty(element) {
            Type::Scalar(Scalar::F16) => self.compiler.cooperative(usage),
            other => panic!(
                "a shader matrix load reads {}",
                crate::shader::element_name(other)
            ),
        };
        let stride = self.value_with_hint(&args[1], Some(self.compiler.scalar("u32")));
        self.emit(ty, |result| ShaderInstruction::MatrixLoad {
            pointer: pointer.value,
            stride: stride.value,
            layout,
            result,
        })
    }

    pub(super) fn atomic(&mut self, intrinsic: Intrinsic, args: &[ast::Expression]) -> Typed {
        let pointer = self.reference(&args[0], "a shader atomic");
        let ty = self.atomic_cell(pointer, "a shader atomic");
        assert_eq!(
            self.compiler.module().ty(ty).scalar(),
            Some(Scalar::U32),
            "a shader atomic counts unsigned words",
        );
        if intrinsic == Intrinsic::AtomicCompareExchange {
            let expected = self.value_with_hint(&args[1], Some(ty));
            let desired = self.value_with_hint(&args[2], Some(ty));
            return self.emit(ty, |result| ShaderInstruction::CompareExchange {
                pointer: pointer.value,
                expected: expected.value,
                desired: desired.value,
                result,
            });
        }
        let fun = match intrinsic {
            Intrinsic::AtomicAdd => AtomicOp::Add,
            Intrinsic::AtomicSub => AtomicOp::Subtract,
            Intrinsic::AtomicExchange => AtomicOp::Exchange,
            other => panic!("{} is not a shader atomic", other.name()),
        };
        let value = self.value_with_hint(&args[1], Some(ty));
        self.emit(ty, |result| ShaderInstruction::Atomic {
            op: fun,
            pointer: pointer.value,
            value: value.value,
            result,
        })
    }
}

fn pointer_space(lower: &FunctionLower<'_>, ty: TypeId) -> crate::shader::Space {
    lower.compiler.module().pointee(ty).0
}

fn pointee(lower: &FunctionLower<'_>, ty: TypeId) -> TypeId {
    lower.compiler.module().pointee(ty).1
}

pub(super) fn intrinsic_arguments(
    intrinsic: Intrinsic,
    arguments: &[ast::Expression],
) -> &[ast::Expression] {
    assert_eq!(
        arguments.len(),
        intrinsic.arity() as usize,
        "the shader intrinsic {} takes {} arguments",
        intrinsic.name(),
        intrinsic.arity(),
    );
    arguments
}

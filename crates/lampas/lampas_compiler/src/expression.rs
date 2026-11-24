use crate::builder::{FunctionBuilder, operation};
use lampas_bytecode::{Constant, Op, fielded, pair};
use lampas_semantic::{Intrinsic, Symbol, Type};
use lampas_syntax::{Diagnostic, Expr, ExprKind, UnaryOp};

impl FunctionBuilder<'_> {
    pub(crate) fn expr(&mut self, expr: &Expr, dst: u8) -> Result<(), Diagnostic> {
        let entry = self.next;
        match &expr.kind {
            ExprKind::Int(value) => self.load(Constant::Int(*value), dst),
            ExprKind::Float(value) => self.load(Constant::Float(*value), dst),
            ExprKind::Bool(value) => self.load(Constant::Bool(*value), dst),
            ExprKind::Text(value) => {
                self.load(Constant::Text(std::rc::Rc::from(value.as_str())), dst)
            }
            ExprKind::Name(_) | ExprKind::SelfEntity => match self.checked.info(expr).symbol {
                Symbol::Local(slot) => {
                    if dst != slot {
                        self.emit(Op::Move, dst, u16::from(slot), 0);
                    }
                }
                Symbol::Global(global) => {
                    self.emit(Op::LoadGlobal, dst, 0, u32::from(global));
                }
                _ => panic!("the compiler reads a name that holds no value"),
            },
            ExprKind::Field { base, .. } => {
                let Symbol::Field { component, field } = self.checked.info(expr).symbol else {
                    panic!("the compiler reads a field that holds no position");
                };
                let at = self.reference(base)?;
                self.emit(
                    Op::ReadField,
                    dst,
                    u16::from(at),
                    lampas_bytecode::fielded(component, field),
                );
            }
            ExprKind::Index { base, index } => {
                let list = self.reference(base)?;
                let at = self.temp()?;
                self.expr(index, at)?;
                self.emit(Op::ListGet, dst, pair(list, at), 0);
            }
            ExprKind::Unary { op, operand } => {
                let source = self.temp()?;
                self.expr(operand, source)?;
                let instruction = match (op, self.checked.info(operand).ty) {
                    (UnaryOp::Not, _) => Op::Not,
                    (UnaryOp::Negate, Type::Value(kind)) => match kind {
                        lampas_bytecode::FieldKind::Int => Op::NegateInt,
                        lampas_bytecode::FieldKind::Float => Op::NegateFloat,
                        _ => Op::NegateVector,
                    },
                    (UnaryOp::Negate, _) => {
                        panic!("the compiler negates a value that holds no minus")
                    }
                };
                self.emit(instruction, dst, u16::from(source), 0);
            }
            ExprKind::Binary { op, lhs, rhs } => {
                if matches!(
                    op,
                    lampas_syntax::BinaryOp::And | lampas_syntax::BinaryOp::Or
                ) {
                    self.expr(lhs, dst)?;
                    let skip = if *op == lampas_syntax::BinaryOp::And {
                        self.emit(Op::JumpIfFalse, 0, u16::from(dst), 0)
                    } else {
                        self.emit(Op::JumpIfTrue, 0, u16::from(dst), 0)
                    };
                    self.expr(rhs, dst)?;
                    self.patch(skip);
                } else {
                    let left_type = self.checked.info(lhs).ty;
                    let right_type = self.checked.info(rhs).ty;
                    let left = self.temp()?;
                    self.expr(lhs, left)?;
                    let right = self.temp()?;
                    self.expr(rhs, right)?;
                    let (instruction, swap) = operation(*op, left_type, right_type);
                    let operands = if swap {
                        pair(right, left)
                    } else {
                        pair(left, right)
                    };
                    self.emit(instruction, dst, operands, 0);
                }
            }
            ExprKind::List(items) => {
                let base = self.reserve(items.len() as u16)?;
                for (position, item) in items.iter().enumerate() {
                    self.expr(item, base + position as u8)?;
                }
                self.emit(Op::MakeList, dst, u16::from(base), items.len() as u32);
            }
            ExprKind::Call { .. } => self.call(expr, dst)?,
            ExprKind::Construct { .. } => {
                panic!("the compiler builds a component outside of add")
            }
        }
        self.next = entry;
        Ok(())
    }

    pub(crate) fn reference(&mut self, expr: &Expr) -> Result<u8, Diagnostic> {
        if let Symbol::Local(slot) = self.checked.info(expr).symbol {
            return Ok(slot);
        }
        let register = self.temp()?;
        self.expr(expr, register)?;
        Ok(register)
    }

    fn call(&mut self, expr: &Expr, dst: u8) -> Result<(), Diagnostic> {
        let ExprKind::Call { arguments, .. } = &expr.kind else {
            panic!("the compiler calls an expression that calls nothing");
        };
        match self.checked.info(expr).symbol {
            Symbol::Function(index) => {
                let arity = self.checked.functions[index as usize].arity as u16;
                let base = self.reserve(arity)?;
                for (position, argument) in arguments.iter().enumerate() {
                    self.expr(argument, base + position as u8)?;
                }
                self.emit(Op::Call, dst, u16::from(base), index as u32);
            }
            Symbol::Host(index) => {
                let arity = u8::try_from(arguments.len()).map_err(|_| {
                    self.error("a call passes more arguments than the machine holds")
                })?;
                let base = self.reserve(arguments.len() as u16)?;
                for (position, argument) in arguments.iter().enumerate() {
                    self.expr(argument, base + position as u8)?;
                }
                self.emit(Op::CallHost, dst, pair(base, arity), u32::from(index));
            }
            Symbol::Intrinsic(intrinsic) => self.intrinsic(intrinsic, arguments, dst)?,
            _ => panic!("the compiler calls a symbol that calls nothing"),
        }
        Ok(())
    }

    fn intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        arguments: &[Expr],
        dst: u8,
    ) -> Result<(), Diagnostic> {
        match intrinsic {
            Intrinsic::Spawn => {
                self.emit(Op::Spawn, dst, 0, 0);
            }
            Intrinsic::Despawn => {
                let entity = self.temp()?;
                self.expr(&arguments[0], entity)?;
                self.emit(Op::Despawn, 0, u16::from(entity), 0);
            }
            Intrinsic::Add => {
                let entity = self.temp()?;
                self.expr(&arguments[0], entity)?;
                let Symbol::Component(component) = self.checked.info(&arguments[1]).symbol else {
                    panic!("the compiler adds a component that holds no type");
                };
                let ExprKind::Construct { fields, .. } = &arguments[1].kind else {
                    panic!("the compiler adds a component that holds no fields");
                };
                let definition = self
                    .checked
                    .fields(component)
                    .iter()
                    .map(|field| (field.name.clone(), field.default.clone()))
                    .collect::<Vec<_>>();
                let base = self.reserve(definition.len() as u16)?;
                for (position, (name, default)) in definition.iter().enumerate() {
                    let target = base + position as u8;
                    match fields.iter().find(|(field, _)| field.name == *name) {
                        Some((_, value)) => self.expr(value, target)?,
                        None => {
                            let value = default
                                .clone()
                                .expect("the checker fills a field that stands omitted");
                            self.load(value, target);
                        }
                    }
                }
                self.emit(
                    Op::AddComponent,
                    base,
                    u16::from(entity),
                    fielded(component, 0),
                );
            }
            Intrinsic::Remove => {
                let entity = self.temp()?;
                self.expr(&arguments[0], entity)?;
                let Symbol::Component(component) = self.checked.info(&arguments[1]).symbol else {
                    panic!("the compiler removes a component that holds no type");
                };
                self.emit(
                    Op::RemoveComponent,
                    0,
                    u16::from(entity),
                    fielded(component, 0),
                );
            }
            Intrinsic::Has => {
                let entity = self.temp()?;
                self.expr(&arguments[0], entity)?;
                let Symbol::Component(component) = self.checked.info(&arguments[1]).symbol else {
                    panic!("the compiler asks for a component that holds no type");
                };
                self.emit(Op::Has, dst, u16::from(entity), fielded(component, 0));
            }
            Intrinsic::Vector => {
                let base = self.reserve(3)?;
                for (position, argument) in arguments.iter().enumerate() {
                    self.expr(argument, base + position as u8)?;
                }
                self.emit(Op::MakeVector, dst, u16::from(base), 0);
            }
            Intrinsic::Length => {
                let checked = self.checked.info(&arguments[0]).ty;
                let source = self.temp()?;
                self.expr(&arguments[0], source)?;
                let instruction = match checked {
                    Type::List(_) => Op::ListLength,
                    Type::TEXT => Op::TextLength,
                    _ => Op::LengthVector,
                };
                self.emit(instruction, dst, u16::from(source), 0);
            }
            Intrinsic::Push => {
                let list = self.reference(&arguments[0])?;
                let value = self.temp()?;
                self.expr(&arguments[1], value)?;
                self.emit(Op::ListPush, list, u16::from(value), 0);
            }
            Intrinsic::Int => {
                let source = self.temp()?;
                self.expr(&arguments[0], source)?;
                self.emit(Op::FloatToInt, dst, u16::from(source), 0);
            }
            Intrinsic::Float => {
                let source = self.temp()?;
                self.expr(&arguments[0], source)?;
                self.emit(Op::IntToFloat, dst, u16::from(source), 0);
            }
            Intrinsic::Text => {
                let source = self.temp()?;
                self.expr(&arguments[0], source)?;
                self.emit(Op::ToText, dst, u16::from(source), 0);
            }
            Intrinsic::SquareRoot
            | Intrinsic::Sine
            | Intrinsic::Cosine
            | Intrinsic::Floor
            | Intrinsic::Absolute => {
                let source = self.temp()?;
                self.expr(&arguments[0], source)?;
                let instruction = match intrinsic {
                    Intrinsic::SquareRoot => Op::SquareRoot,
                    Intrinsic::Sine => Op::Sine,
                    Intrinsic::Cosine => Op::Cosine,
                    Intrinsic::Floor => Op::Floor,
                    _ => Op::Absolute,
                };
                self.emit(instruction, dst, u16::from(source), 0);
            }
            Intrinsic::Minimum | Intrinsic::Maximum => {
                let left = self.temp()?;
                self.expr(&arguments[0], left)?;
                let right = self.temp()?;
                self.expr(&arguments[1], right)?;
                let instruction = if intrinsic == Intrinsic::Minimum {
                    Op::Minimum
                } else {
                    Op::Maximum
                };
                self.emit(instruction, dst, pair(left, right), 0);
            }
            Intrinsic::Normalize => {
                let source = self.temp()?;
                self.expr(&arguments[0], source)?;
                self.emit(Op::NormalizeVector, dst, u16::from(source), 0);
            }
            Intrinsic::Dot => {
                let left = self.temp()?;
                self.expr(&arguments[0], left)?;
                let right = self.temp()?;
                self.expr(&arguments[1], right)?;
                self.emit(Op::DotVector, dst, pair(left, right), 0);
            }
        }
        Ok(())
    }
}

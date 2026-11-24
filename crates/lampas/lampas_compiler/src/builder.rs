use lampas_bytecode::{Constant, Function, Instruction, Op, fielded, pair};
use lampas_semantic::{Checked, StmtInfo, Symbol};
use lampas_syntax::{
    AssignOp, BinaryOp, Block, Diagnostic, Expr, ExprKind, Sources, Span, Statement, StatementKind,
};

pub(crate) struct FunctionBuilder<'a> {
    pub(crate) checked: &'a Checked,
    pub(crate) constants: &'a mut Vec<Constant>,
    pub(crate) code: Vec<Instruction>,
    pub(crate) next: u16,
    pub(crate) registers: u16,
    sources: &'a Sources,
    name: String,
    span: Span,
    system: bool,
    loops: Vec<Loop>,
    ends: Vec<usize>,
}

struct Loop {
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn new(
        checked: &'a Checked,
        constants: &'a mut Vec<Constant>,
        sources: &'a Sources,
        name: &str,
        locals: u16,
        system: bool,
        span: Span,
    ) -> Self {
        Self {
            checked,
            constants,
            sources,
            code: Vec::new(),
            next: locals,
            registers: locals,
            name: String::from(name),
            span,
            system,
            loops: Vec::new(),
            ends: Vec::new(),
        }
    }

    pub(crate) fn body(mut self, block: &Block) -> Result<Function, Diagnostic> {
        self.block(block)?;
        let end = self.code.len();
        let ends = std::mem::take(&mut self.ends);
        for jump in ends {
            self.patch_to(jump, end);
        }
        self.emit(Op::ReturnUnit, 0, 0, 0);
        Ok(Function {
            name: self.name,
            registers: self.registers,
            code: self.code,
        })
    }

    pub(crate) fn block(&mut self, block: &Block) -> Result<(), Diagnostic> {
        for statement in &block.statements {
            self.statement(statement)?;
        }
        Ok(())
    }

    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        let entry = self.next;
        match &statement.kind {
            StatementKind::Let { value, .. } => {
                let StmtInfo::Let(slot) = self.checked.statement(statement) else {
                    panic!("the compiler declares a let without a slot");
                };
                self.expr(value, slot)?;
            }
            StatementKind::Set { target, op, value } => self.set(target, *op, value)?,
            StatementKind::If {
                condition,
                then,
                otherwise,
            } => {
                let register = self.temp()?;
                self.expr(condition, register)?;
                let otherwise_jump = self.emit(Op::JumpIfFalse, 0, u16::from(register), 0);
                self.next = entry;
                self.block(then)?;
                let end_jump = otherwise.as_ref().map(|_| self.emit(Op::Jump, 0, 0, 0));
                self.patch(otherwise_jump);
                if let Some(otherwise) = otherwise {
                    self.block(otherwise)?;
                }
                if let Some(end_jump) = end_jump {
                    self.patch(end_jump);
                }
            }
            StatementKind::While { condition, body } => {
                let start = self.code.len();
                let register = self.temp()?;
                self.expr(condition, register)?;
                let out = self.emit(Op::JumpIfFalse, 0, u16::from(register), 0);
                self.next = entry;
                self.loops.push(Loop {
                    breaks: Vec::new(),
                    continues: Vec::new(),
                });
                self.block(body)?;
                let held = self.loops.pop().expect("a while holds its loop");
                self.emit(Op::Jump, 0, 0, start as u32);
                let end = self.code.len();
                self.patch_to(out, end);
                for jump in held.breaks {
                    self.patch_to(jump, end);
                }
                for jump in held.continues {
                    self.patch_to(jump, start);
                }
            }
            StatementKind::For { list, body, .. } => self.for_statement(statement, list, body)?,
            StatementKind::Return(value) => match value {
                Some(value) => {
                    let register = self.temp()?;
                    self.expr(value, register)?;
                    self.emit(Op::Return, 0, u16::from(register), 0);
                }
                None => {
                    self.emit(Op::ReturnUnit, 0, 0, 0);
                }
            },
            StatementKind::Break => self.break_control(),
            StatementKind::Continue => self.continue_control(),
            StatementKind::Call(expr) => {
                let register = self.temp()?;
                self.expr(expr, register)?;
            }
        }
        self.next = entry;
        Ok(())
    }

    fn for_statement(
        &mut self,
        statement: &Statement,
        list: &Expr,
        body: &Block,
    ) -> Result<(), Diagnostic> {
        let StmtInfo::For {
            binder,
            list: list_slot,
            index,
        } = self.checked.statement(statement)
        else {
            panic!("the compiler walks a for without its slots");
        };
        let entry = self.next;
        self.expr(list, list_slot)?;
        self.load(Constant::Int(0), index);
        let start = self.code.len();
        let length = self.temp()?;
        self.emit(Op::ListLength, length, u16::from(list_slot), 0);
        let condition = self.temp()?;
        self.emit(Op::LessInt, condition, pair(index, length), 0);
        let out = self.emit(Op::JumpIfFalse, 0, u16::from(condition), 0);
        self.next = entry;
        self.emit(Op::ListGet, binder, pair(list_slot, index), 0);
        self.loops.push(Loop {
            breaks: Vec::new(),
            continues: Vec::new(),
        });
        self.block(body)?;
        let held = self.loops.pop().expect("a for holds its loop");
        let step = self.code.len();
        let one = self.temp()?;
        self.load(Constant::Int(1), one);
        self.emit(Op::AddInt, index, pair(index, one), 0);
        self.emit(Op::Jump, 0, 0, start as u32);
        let end = self.code.len();
        self.patch_to(out, end);
        for jump in held.breaks {
            self.patch_to(jump, end);
        }
        for jump in held.continues {
            self.patch_to(jump, step);
        }
        self.next = entry;
        Ok(())
    }

    fn set(&mut self, target: &Expr, op: AssignOp, value: &Expr) -> Result<(), Diagnostic> {
        let symbol = self.checked.info(target).symbol;
        let ty = self.checked.info(target).ty;
        match (&target.kind, symbol) {
            (ExprKind::Name(_) | ExprKind::SelfEntity, Symbol::Local(slot)) => {
                let Some(binary) = op.binary() else {
                    self.expr(value, slot)?;
                    return Ok(());
                };
                let right = self.temp()?;
                self.expr(value, right)?;
                let (instruction, swap) = operation(binary, ty, self.checked.info(value).ty);
                self.emit(instruction, slot, operands(swap, slot, right), 0);
            }
            (ExprKind::Name(_), Symbol::Global(global)) => {
                let Some(binary) = op.binary() else {
                    let register = self.temp()?;
                    self.expr(value, register)?;
                    self.emit(Op::StoreGlobal, 0, u16::from(register), u32::from(global));
                    return Ok(());
                };
                let current = self.temp()?;
                self.emit(Op::LoadGlobal, current, 0, u32::from(global));
                let right = self.temp()?;
                self.expr(value, right)?;
                let (instruction, swap) = operation(binary, ty, self.checked.info(value).ty);
                self.emit(instruction, current, operands(swap, current, right), 0);
                self.emit(Op::StoreGlobal, 0, u16::from(current), u32::from(global));
            }
            (ExprKind::Field { base, .. }, Symbol::Field { component, field }) => {
                let at = self.reference(base)?;
                let Some(binary) = op.binary() else {
                    let register = self.temp()?;
                    self.expr(value, register)?;
                    self.emit(
                        Op::WriteField,
                        at,
                        u16::from(register),
                        fielded(component, field),
                    );
                    return Ok(());
                };
                let current = self.temp()?;
                self.emit(
                    Op::ReadField,
                    current,
                    u16::from(at),
                    fielded(component, field),
                );
                let right = self.temp()?;
                self.expr(value, right)?;
                let (instruction, swap) = operation(binary, ty, self.checked.info(value).ty);
                self.emit(instruction, current, operands(swap, current, right), 0);
                self.emit(
                    Op::WriteField,
                    at,
                    u16::from(current),
                    fielded(component, field),
                );
            }
            (ExprKind::Index { base, index }, _) => {
                let list = self.reference(base)?;
                let at = self.temp()?;
                self.expr(index, at)?;
                let Some(binary) = op.binary() else {
                    let register = self.temp()?;
                    self.expr(value, register)?;
                    self.emit(Op::ListSet, list, pair(at, register), 0);
                    return Ok(());
                };
                let current = self.temp()?;
                self.emit(Op::ListGet, current, pair(list, at), 0);
                let right = self.temp()?;
                self.expr(value, right)?;
                let (instruction, swap) = operation(binary, ty, self.checked.info(value).ty);
                self.emit(instruction, current, operands(swap, current, right), 0);
                self.emit(Op::ListSet, list, pair(at, current), 0);
            }
            _ => panic!("the compiler sets a place that holds no value"),
        }
        Ok(())
    }

    fn break_control(&mut self) {
        if self.loops.is_empty() {
            assert!(self.system, "the compiler breaks outside a system");
            self.emit(Op::Stop, 0, 0, 0);
            return;
        }
        let jump = self.emit(Op::Jump, 0, 0, 0);
        self.loops
            .last_mut()
            .expect("a loop holds its breaks")
            .breaks
            .push(jump);
    }

    fn continue_control(&mut self) {
        if self.loops.is_empty() {
            assert!(self.system, "the compiler continues outside a system");
            let jump = self.emit(Op::Jump, 0, 0, 0);
            self.ends.push(jump);
            return;
        }
        let jump = self.emit(Op::Jump, 0, 0, 0);
        self.loops
            .last_mut()
            .expect("a loop holds its continues")
            .continues
            .push(jump);
    }

    pub(crate) fn emit(&mut self, op: Op, a: u8, b: u16, imm: u32) -> usize {
        let at = self.code.len();
        self.code.push(Instruction::new(op, a, b, imm));
        at
    }

    pub(crate) fn patch(&mut self, at: usize) {
        let target = self.code.len();
        self.patch_to(at, target);
    }

    fn patch_to(&mut self, at: usize, target: usize) {
        self.code[at].imm = target as u32;
    }

    pub(crate) fn temp(&mut self) -> Result<u8, Diagnostic> {
        self.reserve(1)
    }

    pub(crate) fn reserve(&mut self, count: u16) -> Result<u8, Diagnostic> {
        if self.next + count > u16::from(u8::MAX) + 1 {
            return Err(self.error(format!(
                "the function {} holds more registers than the machine has",
                self.name
            )));
        }
        let base = self.next as u8;
        self.next += count;
        self.registers = self.registers.max(self.next);
        Ok(base)
    }

    pub(crate) fn load(&mut self, value: Constant, dst: u8) {
        self.constants.push(value);
        let index = (self.constants.len() - 1) as u32;
        self.emit(Op::LoadConstant, dst, 0, index);
    }

    pub(crate) fn error(&self, message: impl Into<String>) -> Diagnostic {
        self.sources.diagnostic(self.span, message)
    }
}

fn operands(swap: bool, left: u8, right: u8) -> u16 {
    if swap {
        lampas_bytecode::pair(right, left)
    } else {
        lampas_bytecode::pair(left, right)
    }
}

pub(crate) fn operation(
    op: BinaryOp,
    left: lampas_semantic::Type,
    right: lampas_semantic::Type,
) -> (Op, bool) {
    use lampas_bytecode::Kind;
    let kinds = (left.kind(), right.kind());
    match op {
        BinaryOp::Add => match kinds.0 {
            Some(Kind::Int) => (Op::AddInt, false),
            Some(Kind::Float) => (Op::AddFloat, false),
            Some(Kind::Text) => (Op::Concat, false),
            Some(Kind::Vector) => (Op::AddVector, false),
            _ => panic!("the compiler adds values that hold no sum"),
        },
        BinaryOp::Subtract => match kinds.0 {
            Some(Kind::Int) => (Op::SubtractInt, false),
            Some(Kind::Float) => (Op::SubtractFloat, false),
            Some(Kind::Vector) => (Op::SubtractVector, false),
            _ => panic!("the compiler subtracts values that hold no difference"),
        },
        BinaryOp::Multiply => match kinds {
            (Some(Kind::Vector), Some(Kind::Float)) => (Op::ScaleVector, false),
            (Some(Kind::Float), Some(Kind::Vector)) => (Op::ScaleVector, true),
            (Some(Kind::Int), _) => (Op::MultiplyInt, false),
            (Some(Kind::Float), _) => (Op::MultiplyFloat, false),
            _ => panic!("the compiler multiplies values that hold no product"),
        },
        BinaryOp::Divide => match kinds {
            (Some(Kind::Vector), _) => (Op::DivideVector, false),
            (Some(Kind::Int), _) => (Op::DivideInt, false),
            (Some(Kind::Float), _) => (Op::DivideFloat, false),
            _ => panic!("the compiler divides values that hold no quotient"),
        },
        BinaryOp::Remainder => match kinds.0 {
            Some(Kind::Int) => (Op::RemainderInt, false),
            _ => (Op::RemainderFloat, false),
        },
        BinaryOp::Equal | BinaryOp::NotEqual => {
            let equal = op == BinaryOp::Equal;
            let instruction = match kinds.0 {
                Some(Kind::Bool) => {
                    if equal {
                        Op::EqualBool
                    } else {
                        Op::NotEqualBool
                    }
                }
                Some(Kind::Int) => {
                    if equal {
                        Op::EqualInt
                    } else {
                        Op::NotEqualInt
                    }
                }
                Some(Kind::Float) => {
                    if equal {
                        Op::EqualFloat
                    } else {
                        Op::NotEqualFloat
                    }
                }
                Some(Kind::Text) => {
                    if equal {
                        Op::EqualText
                    } else {
                        Op::NotEqualText
                    }
                }
                Some(Kind::Vector) => {
                    if equal {
                        Op::EqualVector
                    } else {
                        Op::NotEqualVector
                    }
                }
                Some(Kind::Entity) => {
                    if equal {
                        Op::EqualEntity
                    } else {
                        Op::NotEqualEntity
                    }
                }
                _ => panic!("the compiler compares values that hold no equality"),
            };
            (instruction, false)
        }
        BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
            let order = match op {
                BinaryOp::Less => 0,
                BinaryOp::LessEqual => 1,
                BinaryOp::Greater => 2,
                _ => 3,
            };
            let instruction = match (kinds.0, order) {
                (Some(Kind::Int), 0) => Op::LessInt,
                (Some(Kind::Int), 1) => Op::LessEqualInt,
                (Some(Kind::Int), 2) => Op::GreaterInt,
                (Some(Kind::Int), _) => Op::GreaterEqualInt,
                (_, 0) => Op::LessFloat,
                (_, 1) => Op::LessEqualFloat,
                (_, 2) => Op::GreaterFloat,
                _ => Op::GreaterEqualFloat,
            };
            (instruction, false)
        }
        BinaryOp::And | BinaryOp::Or => {
            panic!("the compiler short circuits the logic it holds")
        }
    }
}

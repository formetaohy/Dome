use crate::context::Context;
use crate::value::Value;
use crate::vm::{Frame, Outcome, Vm};
use dome_ecs::Entity;
use dome_math::Vec3;
use lampas_bytecode::{Instruction, Module, Op};
use std::cell::RefCell;
use std::rc::Rc;

impl Vm {
    pub(crate) fn execute(&mut self, module: &Module, context: &mut dyn Context) -> Outcome {
        loop {
            let frame = *self.frames.last().expect("a lampas frame executes");
            let instruction = module.functions[frame.function as usize].code[frame.pc];
            self.frames.last_mut().expect("a lampas frame executes").pc += 1;
            let base = frame.base;
            let dst = base + instruction.a as usize;
            match instruction.op {
                Op::LoadConstant => {
                    self.stack[dst] = Value::constant(&module.constants[instruction.imm as usize]);
                }
                Op::LoadGlobal => {
                    self.stack[dst] = self.globals[instruction.imm as usize].clone();
                }
                Op::StoreGlobal => {
                    let value = self.stack[base + instruction.b as usize].clone();
                    self.globals[instruction.imm as usize] = value;
                }
                Op::Move => {
                    if instruction.a != instruction.low() {
                        self.stack[dst] = self.stack[base + instruction.low() as usize].clone();
                    }
                }
                Op::LoadUnit => self.stack[dst] = Value::Unit,
                Op::Not => {
                    let value = self.stack[base + instruction.b as usize].boolean();
                    self.stack[dst] = Value::Bool(!value);
                }
                Op::NegateInt => {
                    let value = self.stack[base + instruction.b as usize].integer();
                    let value = value
                        .checked_neg()
                        .unwrap_or_else(|| fail(module, frame, "the int holds no minus"));
                    self.stack[dst] = Value::Int(value);
                }
                Op::NegateFloat => {
                    let value = self.stack[base + instruction.b as usize].float();
                    self.stack[dst] = Value::Float(-value);
                }
                Op::NegateVector => {
                    let value = self.stack[base + instruction.b as usize].vector();
                    self.stack[dst] = Value::Vector(-value);
                }
                Op::AddInt => {
                    let (left, right) = self.integers(base, instruction);
                    let value = integer(module, frame, left.checked_add(right), right, false);
                    self.stack[dst] = Value::Int(value);
                }
                Op::SubtractInt => {
                    let (left, right) = self.integers(base, instruction);
                    let value = integer(module, frame, left.checked_sub(right), right, false);
                    self.stack[dst] = Value::Int(value);
                }
                Op::MultiplyInt => {
                    let (left, right) = self.integers(base, instruction);
                    let value = integer(module, frame, left.checked_mul(right), right, false);
                    self.stack[dst] = Value::Int(value);
                }
                Op::DivideInt => {
                    let (left, right) = self.integers(base, instruction);
                    let value = integer(module, frame, left.checked_div(right), right, true);
                    self.stack[dst] = Value::Int(value);
                }
                Op::RemainderInt => {
                    let (left, right) = self.integers(base, instruction);
                    let value = integer(module, frame, left.checked_rem(right), right, true);
                    self.stack[dst] = Value::Int(value);
                }
                Op::AddFloat => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Float(left + right);
                }
                Op::SubtractFloat => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Float(left - right);
                }
                Op::MultiplyFloat => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Float(left * right);
                }
                Op::DivideFloat => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Float(left / right);
                }
                Op::RemainderFloat => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Float(left % right);
                }
                Op::AddVector => {
                    let (left, right) = self.vectors(base, instruction);
                    self.stack[dst] = Value::Vector(left + right);
                }
                Op::SubtractVector => {
                    let (left, right) = self.vectors(base, instruction);
                    self.stack[dst] = Value::Vector(left - right);
                }
                Op::ScaleVector => {
                    let vector = self.stack[base + instruction.low() as usize].vector();
                    let scalar = self.stack[base + instruction.high() as usize].float() as f32;
                    self.stack[dst] = Value::Vector(vector * scalar);
                }
                Op::DivideVector => {
                    let vector = self.stack[base + instruction.low() as usize].vector();
                    let scalar = self.stack[base + instruction.high() as usize].float() as f32;
                    self.stack[dst] = Value::Vector(vector / scalar);
                }
                Op::Concat => {
                    let text = {
                        let (left, right) = self.texts(base, instruction);
                        format!("{left}{right}")
                    };
                    self.stack[dst] = Value::Text(Rc::from(text));
                }
                Op::EqualInt => {
                    let (left, right) = self.integers(base, instruction);
                    self.stack[dst] = Value::Bool(left == right);
                }
                Op::NotEqualInt => {
                    let (left, right) = self.integers(base, instruction);
                    self.stack[dst] = Value::Bool(left != right);
                }
                Op::EqualFloat => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Bool(left == right);
                }
                Op::NotEqualFloat => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Bool(left != right);
                }
                Op::EqualBool => {
                    let (left, right) = self.booleans(base, instruction);
                    self.stack[dst] = Value::Bool(left == right);
                }
                Op::NotEqualBool => {
                    let (left, right) = self.booleans(base, instruction);
                    self.stack[dst] = Value::Bool(left != right);
                }
                Op::EqualText => {
                    let equal = {
                        let (left, right) = self.texts(base, instruction);
                        left == right
                    };
                    self.stack[dst] = Value::Bool(equal);
                }
                Op::NotEqualText => {
                    let equal = {
                        let (left, right) = self.texts(base, instruction);
                        left != right
                    };
                    self.stack[dst] = Value::Bool(equal);
                }
                Op::EqualEntity => {
                    let (left, right) = self.entities(base, instruction);
                    self.stack[dst] = Value::Bool(left == right);
                }
                Op::NotEqualEntity => {
                    let (left, right) = self.entities(base, instruction);
                    self.stack[dst] = Value::Bool(left != right);
                }
                Op::EqualVector => {
                    let (left, right) = self.vectors(base, instruction);
                    self.stack[dst] = Value::Bool(left == right);
                }
                Op::NotEqualVector => {
                    let (left, right) = self.vectors(base, instruction);
                    self.stack[dst] = Value::Bool(left != right);
                }
                Op::LessInt => {
                    let (left, right) = self.integers(base, instruction);
                    self.stack[dst] = Value::Bool(left < right);
                }
                Op::LessEqualInt => {
                    let (left, right) = self.integers(base, instruction);
                    self.stack[dst] = Value::Bool(left <= right);
                }
                Op::GreaterInt => {
                    let (left, right) = self.integers(base, instruction);
                    self.stack[dst] = Value::Bool(left > right);
                }
                Op::GreaterEqualInt => {
                    let (left, right) = self.integers(base, instruction);
                    self.stack[dst] = Value::Bool(left >= right);
                }
                Op::LessFloat => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Bool(left < right);
                }
                Op::LessEqualFloat => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Bool(left <= right);
                }
                Op::GreaterFloat => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Bool(left > right);
                }
                Op::GreaterEqualFloat => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Bool(left >= right);
                }
                Op::Jump => {
                    self.frames.last_mut().expect("a lampas frame executes").pc =
                        instruction.imm as usize;
                }
                Op::JumpIfFalse => {
                    let jump = !self.stack[base + instruction.b as usize].boolean();
                    if jump {
                        self.frames.last_mut().expect("a lampas frame executes").pc =
                            instruction.imm as usize;
                    }
                }
                Op::JumpIfTrue => {
                    let jump = self.stack[base + instruction.b as usize].boolean();
                    if jump {
                        self.frames.last_mut().expect("a lampas frame executes").pc =
                            instruction.imm as usize;
                    }
                }
                Op::Call => {
                    let callee = instruction.imm as usize;
                    let at = base + instruction.b as usize;
                    assert!(
                        self.frames.len() < self.maximum_depth(),
                        "the lampas call stack overflows",
                    );
                    let registers = module.functions[callee].registers as usize;
                    self.stack.resize(at + registers, Value::Unit);
                    self.frames.push(Frame {
                        function: instruction.imm,
                        base: at,
                        pc: 0,
                        dst: instruction.a,
                    });
                }
                Op::CallHost => {
                    let arity = instruction.high() as usize;
                    let at = base + instruction.low() as usize;
                    let value = context.host(instruction.imm as u16, &self.stack[at..at + arity]);
                    self.stack[dst] = value;
                }
                Op::Return | Op::ReturnUnit => {
                    let value = if instruction.op == Op::Return {
                        self.stack[base + instruction.b as usize].clone()
                    } else {
                        Value::Unit
                    };
                    let frame = self.frames.pop().expect("a lampas frame returns");
                    match self.frames.last().copied() {
                        Some(caller) => {
                            let registers =
                                module.functions[caller.function as usize].registers as usize;
                            let target = caller.base + frame.dst as usize;
                            self.stack.resize(caller.base + registers, Value::Unit);
                            self.stack[target] = value;
                        }
                        None => {
                            self.stack.clear();
                            return Outcome::Returned(value);
                        }
                    }
                }
                Op::Stop => {
                    self.stack.clear();
                    self.frames.clear();
                    return Outcome::Stopped;
                }
                Op::MakeList => {
                    let count = instruction.imm as usize;
                    let at = base + instruction.b as usize;
                    let mut items = Vec::with_capacity(count);
                    for index in at..at + count {
                        items.push(self.stack[index].clone());
                    }
                    self.stack[dst] = Value::List(Rc::new(RefCell::new(items)));
                }
                Op::MakeVector => {
                    let at = base + instruction.b as usize;
                    let vector = Vec3::new(
                        self.stack[at].float() as f32,
                        self.stack[at + 1].float() as f32,
                        self.stack[at + 2].float() as f32,
                    );
                    self.stack[dst] = Value::Vector(vector);
                }
                Op::ListLength => {
                    let length = self.stack[base + instruction.b as usize]
                        .list()
                        .borrow()
                        .len();
                    self.stack[dst] = Value::Int(length as i64);
                }
                Op::TextLength => {
                    let length = self.stack[base + instruction.b as usize]
                        .text()
                        .chars()
                        .count();
                    self.stack[dst] = Value::Int(length as i64);
                }
                Op::ListPush => {
                    let value = self.stack[base + instruction.b as usize].clone();
                    self.stack[base + instruction.a as usize]
                        .list()
                        .borrow_mut()
                        .push(value);
                }
                Op::ListGet => {
                    let index = self.stack[base + instruction.high() as usize].integer();
                    let at = usize::try_from(index)
                        .unwrap_or_else(|_| fail(module, frame, "the list holds no negative item"));
                    let value = self.stack[base + instruction.low() as usize]
                        .list()
                        .borrow()
                        .get(at)
                        .cloned()
                        .unwrap_or_else(|| {
                            fail(module, frame, format!("the list holds no item {index}"))
                        });
                    self.stack[dst] = value;
                }
                Op::ListSet => {
                    let index = self.stack[base + instruction.low() as usize].integer();
                    let value = self.stack[base + instruction.high() as usize].clone();
                    let at = usize::try_from(index)
                        .unwrap_or_else(|_| fail(module, frame, "the list holds no negative item"));
                    let mut items = self.stack[base + instruction.a as usize]
                        .list()
                        .borrow_mut();
                    assert!(
                        at < items.len(),
                        "the lampas function {} fails at instruction {}: the list holds no item {index}",
                        module.functions[frame.function as usize].name,
                        frame.pc - 1,
                    );
                    items[at] = value;
                }
                Op::ReadField => {
                    let row = self.stack[base + instruction.b as usize].component();
                    let value = context.read(row, instruction.component(), instruction.field());
                    self.stack[dst] = value;
                }
                Op::WriteField => {
                    let row = self.stack[base + instruction.a as usize].component();
                    let value = self.stack[base + instruction.b as usize].clone();
                    context.write(row, instruction.component(), instruction.field(), value);
                }
                Op::Has => {
                    let entity = self.stack[base + instruction.b as usize].entity();
                    let has = context.has(entity, instruction.component());
                    self.stack[dst] = Value::Bool(has);
                }
                Op::Spawn => {
                    let entity = context.spawn();
                    self.stack[dst] = Value::Entity(entity);
                }
                Op::Despawn => {
                    let entity = self.stack[base + instruction.b as usize].entity();
                    context.despawn(entity);
                }
                Op::AddComponent => {
                    let component = instruction.component();
                    let count = module.components[component as usize].fields.len();
                    let at = base + instruction.a as usize;
                    let mut fields = Vec::with_capacity(count);
                    for index in at..at + count {
                        fields.push(self.stack[index].clone());
                    }
                    let entity = self.stack[base + instruction.b as usize].entity();
                    context.add(entity, component, &fields);
                }
                Op::RemoveComponent => {
                    let entity = self.stack[base + instruction.b as usize].entity();
                    context.remove(entity, instruction.component());
                }
                Op::IntToFloat => {
                    let value = self.stack[base + instruction.b as usize].integer();
                    self.stack[dst] = Value::Float(value as f64);
                }
                Op::FloatToInt => {
                    let value = self.stack[base + instruction.b as usize].float();
                    assert!(
                        value.is_finite() && value >= i64::MIN as f64 && value < -(i64::MIN as f64),
                        "the lampas function {} fails at instruction {}: {value} holds no int",
                        module.functions[frame.function as usize].name,
                        frame.pc - 1,
                    );
                    self.stack[dst] = Value::Int(value.trunc() as i64);
                }
                Op::ToText => {
                    let value = self.stack[base + instruction.b as usize].clone();
                    let text = match value {
                        Value::Text(text) => text,
                        other => Rc::from(other.describe()),
                    };
                    self.stack[dst] = Value::Text(text);
                }
                Op::SquareRoot => {
                    let value = self.stack[base + instruction.b as usize].float();
                    self.stack[dst] = Value::Float(value.sqrt());
                }
                Op::Sine => {
                    let value = self.stack[base + instruction.b as usize].float();
                    self.stack[dst] = Value::Float(value.sin());
                }
                Op::Cosine => {
                    let value = self.stack[base + instruction.b as usize].float();
                    self.stack[dst] = Value::Float(value.cos());
                }
                Op::Floor => {
                    let value = self.stack[base + instruction.b as usize].float();
                    self.stack[dst] = Value::Float(value.floor());
                }
                Op::Absolute => {
                    let value = self.stack[base + instruction.b as usize].float();
                    self.stack[dst] = Value::Float(value.abs());
                }
                Op::Minimum => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Float(left.min(right));
                }
                Op::Maximum => {
                    let (left, right) = self.floats(base, instruction);
                    self.stack[dst] = Value::Float(left.max(right));
                }
                Op::LengthVector => {
                    let value = self.stack[base + instruction.b as usize].vector();
                    self.stack[dst] = Value::Float(value.length() as f64);
                }
                Op::NormalizeVector => {
                    let value = self.stack[base + instruction.b as usize].vector();
                    self.stack[dst] = Value::Vector(value.normalize());
                }
                Op::DotVector => {
                    let (left, right) = self.vectors(base, instruction);
                    self.stack[dst] = Value::Float(left.dot(right) as f64);
                }
            }
        }
    }

    fn integers(&self, base: usize, instruction: Instruction) -> (i64, i64) {
        (
            self.stack[base + instruction.low() as usize].integer(),
            self.stack[base + instruction.high() as usize].integer(),
        )
    }

    fn floats(&self, base: usize, instruction: Instruction) -> (f64, f64) {
        (
            self.stack[base + instruction.low() as usize].float(),
            self.stack[base + instruction.high() as usize].float(),
        )
    }

    fn vectors(&self, base: usize, instruction: Instruction) -> (Vec3, Vec3) {
        (
            self.stack[base + instruction.low() as usize].vector(),
            self.stack[base + instruction.high() as usize].vector(),
        )
    }

    fn booleans(&self, base: usize, instruction: Instruction) -> (bool, bool) {
        (
            self.stack[base + instruction.low() as usize].boolean(),
            self.stack[base + instruction.high() as usize].boolean(),
        )
    }

    fn texts(&self, base: usize, instruction: Instruction) -> (&str, &str) {
        (
            self.stack[base + instruction.low() as usize].text(),
            self.stack[base + instruction.high() as usize].text(),
        )
    }

    fn entities(&self, base: usize, instruction: Instruction) -> (Entity, Entity) {
        (
            self.stack[base + instruction.low() as usize].entity(),
            self.stack[base + instruction.high() as usize].entity(),
        )
    }
}

fn integer(module: &Module, frame: Frame, value: Option<i64>, right: i64, divides: bool) -> i64 {
    value.unwrap_or_else(|| {
        if divides && right == 0 {
            fail(module, frame, "the int divides by zero")
        } else {
            fail(module, frame, "the int overflows")
        }
    })
}

fn fail(module: &Module, frame: Frame, message: impl std::fmt::Display) -> ! {
    panic!(
        "the lampas function {} fails at instruction {}: {message}",
        module.functions[frame.function as usize].name,
        frame.pc - 1,
    )
}

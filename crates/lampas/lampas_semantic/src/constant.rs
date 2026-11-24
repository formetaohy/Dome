use crate::Type;
use crate::check::Checker;
use dome_math::Vec3;
use lampas_bytecode::{Constant, Kind};
use lampas_syntax::{BinaryOp, Diagnostic, Expr, ExprKind, UnaryOp};
use std::sync::Arc;

impl Checker<'_> {
    pub(crate) fn constant(&self, expr: &Expr, expected: Type) -> Result<Constant, Diagnostic> {
        match (&expr.kind, expected) {
            (ExprKind::Int(value), Type::Value(Kind::Int)) => Ok(Constant::Int(*value)),
            (ExprKind::Float(value), Type::Value(Kind::Float)) => Ok(Constant::Float(*value)),
            (ExprKind::Bool(value), Type::Value(Kind::Bool)) => Ok(Constant::Bool(*value)),
            (ExprKind::Text(value), Type::Value(Kind::Text)) => {
                Ok(Constant::Text(Arc::from(value.as_str())))
            }
            (
                ExprKind::Unary {
                    op: UnaryOp::Negate,
                    operand,
                },
                Type::Value(kind @ (Kind::Int | Kind::Float)),
            ) => Ok(match self.constant(operand, Type::Value(kind))? {
                Constant::Int(value) => Constant::Int(-value),
                Constant::Float(value) => Constant::Float(-value),
                _ => unreachable!("a negated constant holds a number"),
            }),
            (ExprKind::Binary { op, lhs, rhs }, Type::Value(_)) => {
                let left = self.constant(lhs, expected)?;
                let right = self.constant(rhs, expected)?;
                self.fold(*op, left, right, expr.span)
            }
            (ExprKind::List(items), Type::List(kind)) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.constant(item, Type::Value(kind))?);
                }
                Ok(Constant::List(values))
            }
            (ExprKind::Call { callee, arguments }, Type::Value(Kind::Vector))
                if callee.name == "vec3" =>
            {
                if arguments.len() != 3 {
                    return Err(self.error(expr.span, "vec3 holds three numbers"));
                }
                let mut numbers = [0.0f32; 3];
                for (at, argument) in arguments.iter().enumerate() {
                    match self.constant(argument, Type::FLOAT)? {
                        Constant::Float(value) => numbers[at] = value as f32,
                        _ => unreachable!("a vec3 constant holds numbers"),
                    }
                }
                Ok(Constant::Vector(Vec3::new(
                    numbers[0], numbers[1], numbers[2],
                )))
            }
            _ => Err(self.error(expr.span, "a declaration holds a constant here")),
        }
    }

    fn fold(
        &self,
        op: BinaryOp,
        left: Constant,
        right: Constant,
        span: lampas_syntax::Span,
    ) -> Result<Constant, Diagnostic> {
        match (left, right) {
            (Constant::Int(left), Constant::Int(right)) => {
                let value = match op {
                    BinaryOp::Add => left.checked_add(right),
                    BinaryOp::Subtract => left.checked_sub(right),
                    BinaryOp::Multiply => left.checked_mul(right),
                    BinaryOp::Divide => left.checked_div(right),
                    BinaryOp::Remainder => left.checked_rem(right),
                    other => {
                        return Err(
                            self.error(span, format!("{} folds no constants", other.text()))
                        );
                    }
                };
                value
                    .map(Constant::Int)
                    .ok_or_else(|| self.error(span, "the constant overflows or divides by zero"))
            }
            (Constant::Float(left), Constant::Float(right)) => {
                let value = match op {
                    BinaryOp::Add => left + right,
                    BinaryOp::Subtract => left - right,
                    BinaryOp::Multiply => left * right,
                    BinaryOp::Divide => left / right,
                    BinaryOp::Remainder => left % right,
                    other => {
                        return Err(
                            self.error(span, format!("{} folds no constants", other.text()))
                        );
                    }
                };
                Ok(Constant::Float(value))
            }
            (Constant::Text(left), Constant::Text(right)) if op == BinaryOp::Add => {
                Ok(Constant::Text(Arc::from(format!("{left}{right}"))))
            }
            _ => Err(self.error(span, format!("{} folds no constants", op.text()))),
        }
    }
}

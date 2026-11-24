use crate::Type;
use crate::check::{Body, Checker, Local};
use crate::symbol::{StmtInfo, Symbol};
use lampas_bytecode::Access;
use lampas_syntax::{AssignOp, Block, Diagnostic, Expr, Statement, StatementKind};
use std::collections::HashMap;

impl Checker<'_> {
    pub(crate) fn block(&mut self, block: &Block) -> Result<(), Diagnostic> {
        self.scopes.push(HashMap::new());
        for statement in &block.statements {
            self.statement(statement)?;
        }
        self.scopes.pop();
        Ok(())
    }

    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match &statement.kind {
            StatementKind::Let { name, ty, value } => {
                let expected = match ty {
                    Some(ty) => Some(self.value_type(ty)?),
                    None => None,
                };
                let checked = self.expr(value, expected)?;
                if !checked.is_value() {
                    return Err(self.error(
                        value.span,
                        format!("{} holds no value", self.describe(checked)),
                    ));
                }
                if let Some(expected) = expected
                    && expected != checked
                {
                    return Err(self.mismatch(value.span, expected, checked));
                }
                let slot = self.declare_local(&name.name, name.span, Local::Value(checked))?;
                self.statements.insert(statement.id, StmtInfo::Let(slot));
            }
            StatementKind::Set { target, op, value } => {
                self.set(statement, target, *op, value)?;
            }
            StatementKind::If {
                condition,
                then,
                otherwise,
            } => {
                self.expect_bool(condition)?;
                self.block(then)?;
                if let Some(otherwise) = otherwise {
                    self.block(otherwise)?;
                }
            }
            StatementKind::While { condition, body } => {
                self.expect_bool(condition)?;
                self.loops += 1;
                self.block(body)?;
                self.loops -= 1;
            }
            StatementKind::For { name, list, body } => {
                let ty = self.expr(list, None)?;
                let Type::List(kind) = ty else {
                    return Err(self.error(
                        list.span,
                        format!("a for walks a list, not {}", self.describe(ty)),
                    ));
                };
                let list_slot = self.hidden(Local::Value(ty), list.span)?;
                let index_slot = self.hidden(Local::Value(Type::INT), list.span)?;
                self.loops += 1;
                self.scopes.push(HashMap::new());
                let binder =
                    self.declare_local(&name.name, name.span, Local::Value(Type::Value(kind)))?;
                self.block(body)?;
                self.scopes.pop();
                self.loops -= 1;
                self.statements.insert(
                    statement.id,
                    StmtInfo::For {
                        binder,
                        list: list_slot,
                        index: index_slot,
                    },
                );
            }
            StatementKind::Return(value) => self.return_value(statement.span, value.as_ref())?,
            StatementKind::Break => self.loop_control(statement.span, "break")?,
            StatementKind::Continue => self.loop_control(statement.span, "continue")?,
            StatementKind::Call(expr) => {
                self.expr(expr, None)?;
            }
        }
        Ok(())
    }

    fn set(
        &mut self,
        statement: &Statement,
        target: &Expr,
        op: AssignOp,
        value: &Expr,
    ) -> Result<(), Diagnostic> {
        let ty = self.expr(target, None)?;
        match &target.kind {
            lampas_syntax::ExprKind::SelfEntity => {
                return Err(self.error(target.span, "self holds no place"));
            }
            lampas_syntax::ExprKind::Name(_) => match self.symbol(target) {
                Symbol::Local(slot) => {
                    if matches!(self.locals[slot as usize], Local::Component { .. }) {
                        return Err(self.error(target.span, "a component parameter takes no value"));
                    }
                }
                Symbol::Global(_) => {}
                _ => {
                    return Err(self.error(target.span, "the name holds no place"));
                }
            },
            lampas_syntax::ExprKind::Field { base, .. } => {
                if self.access_of(base)? == Access::Read {
                    return Err(self.error(
                        target.span,
                        format!("{} is read only", self.field_name(target)),
                    ));
                }
            }
            lampas_syntax::ExprKind::Index { .. } => {}
            _ => {
                return Err(self.error(target.span, "the value holds no place to set"));
            }
        }
        let checked = self.expr(value, Some(ty))?;
        match op {
            AssignOp::Set => {
                if checked != ty {
                    return Err(self.mismatch(value.span, ty, checked));
                }
            }
            _ => {
                let binary = op
                    .binary()
                    .expect("a compound assignment holds its operator");
                let result = self.binary_type(binary, ty, checked, value.span)?;
                if result != ty {
                    return Err(self.error(
                        statement.span,
                        format!(
                            "{} is set into {}",
                            self.describe(result),
                            self.describe(ty),
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    fn return_value(
        &mut self,
        span: lampas_syntax::Span,
        value: Option<&Expr>,
    ) -> Result<(), Diagnostic> {
        if self.body != Body::Function {
            return Err(self.error(span, "a system returns to the engine"));
        }
        match value {
            Some(value) => {
                if self.result == Type::VOID {
                    return Err(self.error(span, "the function returns no value"));
                }
                let checked = self.expr(value, Some(self.result))?;
                if checked != self.result {
                    return Err(self.mismatch(value.span, self.result, checked));
                }
            }
            None => {
                if self.result != Type::VOID {
                    return Err(self.error(
                        span,
                        format!(
                            "the function returns {} and no value stands here",
                            self.describe(self.result),
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    fn loop_control(&mut self, span: lampas_syntax::Span, what: &str) -> Result<(), Diagnostic> {
        if self.loops > 0 {
            return Ok(());
        }
        match self.body {
            Body::System { entity: true } => Ok(()),
            _ => Err(self.error(span, format!("{what} holds no loop"))),
        }
    }

    pub(crate) fn expect_bool(&mut self, expr: &Expr) -> Result<(), Diagnostic> {
        let checked = self.expr(expr, Some(Type::BOOL))?;
        if checked != Type::BOOL {
            return Err(self.mismatch(expr.span, Type::BOOL, checked));
        }
        Ok(())
    }

    pub(crate) fn access_of(&mut self, base: &Expr) -> Result<Access, Diagnostic> {
        self.expr(base, None)?;
        match self.symbol(base) {
            Symbol::Local(slot) => match self.locals[slot as usize] {
                Local::Component { access, .. } => Ok(access),
                Local::Value(_) => Err(self.error(base.span, "the value holds no component")),
            },
            _ => Err(self.error(base.span, "the value holds no component")),
        }
    }

    fn field_name(&self, target: &Expr) -> String {
        match &target.kind {
            lampas_syntax::ExprKind::Field { base, name } => match self.symbol(base) {
                Symbol::Local(slot) => match self.locals[slot as usize] {
                    Local::Component { component, .. } => format!(
                        "the {}.{} field",
                        self.components[component as usize].name, name.name,
                    ),
                    Local::Value(_) => String::from(name.name.as_str()),
                },
                _ => String::from(name.name.as_str()),
            },
            _ => String::from("the place"),
        }
    }
}

pub(crate) fn diverges(block: &Block) -> bool {
    block.statements.iter().any(statement_diverges)
}

fn statement_diverges(statement: &Statement) -> bool {
    match &statement.kind {
        StatementKind::Return(_) => true,
        StatementKind::If {
            then,
            otherwise: Some(otherwise),
            ..
        } => diverges(then) && diverges(otherwise),
        _ => false,
    }
}

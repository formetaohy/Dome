use crate::Type;
use crate::symbol::{ExprInfo, StmtInfo};
use lampas_bytecode::{ComponentParam, Constant, Kind, Phase};
use lampas_syntax::{Expr, ExprId, Span, Statement, StmtId};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct HostSignature {
    pub name: String,
    pub params: Vec<Type>,
    pub result: Type,
}

#[derive(Clone, Debug)]
pub struct CheckedField {
    pub name: String,
    pub kind: Kind,
    pub default: Option<Constant>,
}

#[derive(Clone, Debug)]
pub struct CheckedComponent {
    pub name: String,
    pub fields: Vec<CheckedField>,
}

#[derive(Clone, Debug)]
pub struct CheckedGlobal {
    pub name: String,
    pub ty: Type,
    pub value: Constant,
}

#[derive(Clone, Debug)]
pub struct CheckedFunction {
    pub name: String,
    pub arity: u8,
    pub locals: u16,
    pub result: Type,
}

#[derive(Clone, Debug)]
pub struct CheckedSystem {
    pub name: String,
    pub span: Span,
    pub params: Vec<ComponentParam>,
    pub locals: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct CheckedRun {
    pub phase: Phase,
    pub system: u16,
}

#[derive(Clone, Debug)]
pub struct CheckedBoot {
    pub locals: u16,
}

#[derive(Default)]
pub struct Checked {
    pub components: Vec<CheckedComponent>,
    pub globals: Vec<CheckedGlobal>,
    pub functions: Vec<CheckedFunction>,
    pub systems: Vec<CheckedSystem>,
    pub runs: Vec<CheckedRun>,
    pub boot: Option<CheckedBoot>,
    pub hosts: usize,
    pub(crate) infos: HashMap<ExprId, ExprInfo>,
    pub(crate) statements: HashMap<StmtId, StmtInfo>,
}

impl Checked {
    pub fn info(&self, expr: &Expr) -> ExprInfo {
        *self.infos.get(&expr.id).unwrap_or_else(|| {
            panic!(
                "the checked program holds the expression at {}",
                expr.span.start
            )
        })
    }

    pub fn statement(&self, statement: &Statement) -> StmtInfo {
        *self.statements.get(&statement.id).unwrap_or_else(|| {
            panic!(
                "the checked program holds the statement at {}",
                statement.span.start,
            )
        })
    }

    pub fn fields(&self, component: u16) -> &[CheckedField] {
        &self.components[component as usize].fields
    }
}

use crate::Type;
use crate::check::{Body, Checker, Local};
use crate::symbol::{ExprInfo, Intrinsic, Symbol};
use lampas_bytecode::{Access, Kind};
use lampas_syntax::{BinaryOp, Diagnostic, Expr, ExprKind, Ident, Span, UnaryOp};

impl Checker<'_> {
    pub(crate) fn symbol(&self, expr: &Expr) -> Symbol {
        self.info(expr).symbol
    }

    fn info(&self, expr: &Expr) -> ExprInfo {
        *self.infos.get(&expr.id).unwrap_or_else(|| {
            panic!(
                "the checker holds the expression at source {}",
                expr.span.start,
            )
        })
    }

    pub(crate) fn expr(&mut self, expr: &Expr, expected: Option<Type>) -> Result<Type, Diagnostic> {
        let (ty, symbol) = match &expr.kind {
            ExprKind::Int(_) => (Type::INT, Symbol::None),
            ExprKind::Float(_) => (Type::FLOAT, Symbol::None),
            ExprKind::Bool(_) => (Type::BOOL, Symbol::None),
            ExprKind::Text(_) => (Type::TEXT, Symbol::None),
            ExprKind::Name(name) => self.name(name)?,
            ExprKind::SelfEntity => self.self_entity(expr.span)?,
            ExprKind::Field { base, name } => self.field(base, name)?,
            ExprKind::Index { base, index } => self.index(base, index)?,
            ExprKind::Call { callee, arguments } => self.call(callee, arguments, expr.span)?,
            ExprKind::Unary { op, operand } => self.unary(*op, operand)?,
            ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs)?,
            ExprKind::List(items) => self.list(items, expected, expr.span)?,
            ExprKind::Construct { name, .. } => {
                return Err(self.error(
                    name.span,
                    format!("the component {} is built by add", name.name),
                ));
            }
        };
        self.infos.insert(expr.id, ExprInfo { ty, symbol });
        Ok(ty)
    }

    fn name(&mut self, name: &Ident) -> Result<(Type, Symbol), Diagnostic> {
        if let Some(slot) = self.lookup(&name.name) {
            return Ok((self.local_type(slot), Symbol::Local(slot)));
        }
        if let Some(global) = self.global_index.get(&name.name) {
            return Ok((self.globals[*global as usize].ty, Symbol::Global(*global)));
        }
        if self.component_index.contains_key(&name.name) {
            return Err(self.error(
                name.span,
                format!("the component {} is no value", name.name),
            ));
        }
        if self.function_index.contains_key(&name.name) {
            return Err(self.error(name.span, format!("the function {} is called", name.name)));
        }
        if self.host_index.contains_key(&name.name) {
            return Err(self.error(
                name.span,
                format!("the host function {} is called", name.name),
            ));
        }
        if let Some(intrinsic) = Intrinsic::named(&name.name) {
            return Err(self.error(
                name.span,
                format!("the intrinsic {} is called", intrinsic.name()),
            ));
        }
        Err(self.error(name.span, format!("the source holds no {}", name.name)))
    }

    fn self_entity(&self, span: Span) -> Result<(Type, Symbol), Diagnostic> {
        match self.body {
            Body::System { entity: true } => Ok((Type::ENTITY, Symbol::Local(0))),
            _ => Err(self.error(span, "self stands outside the entity a system walks")),
        }
    }

    fn field(&mut self, base: &Expr, name: &Ident) -> Result<(Type, Symbol), Diagnostic> {
        let ty = self.expr(base, None)?;
        let Type::Component(component) = ty else {
            return Err(self.error(
                name.span,
                format!("{} holds no field {}", self.describe(ty), name.name),
            ));
        };
        let field = self.components[component as usize]
            .fields
            .iter()
            .position(|held| held.name == name.name)
            .ok_or_else(|| {
                self.error(
                    name.span,
                    format!(
                        "the component {} holds no field {}",
                        self.components[component as usize].name, name.name,
                    ),
                )
            })?;
        let kind = self.components[component as usize].fields[field].kind;
        Ok((
            Type::Value(kind),
            Symbol::Field {
                component,
                field: field as u16,
            },
        ))
    }

    fn index(&mut self, base: &Expr, index: &Expr) -> Result<(Type, Symbol), Diagnostic> {
        let ty = self.expr(base, None)?;
        let Type::List(kind) = ty else {
            return Err(self.error(base.span, format!("{} holds no item", self.describe(ty))));
        };
        let checked = self.expr(index, Some(Type::INT))?;
        if checked != Type::INT {
            return Err(self.mismatch(index.span, Type::INT, checked));
        }
        Ok((Type::Value(kind), Symbol::None))
    }

    fn list(
        &mut self,
        items: &[Expr],
        expected: Option<Type>,
        span: Span,
    ) -> Result<(Type, Symbol), Diagnostic> {
        let mut element = match expected {
            Some(Type::List(kind)) => Some(kind),
            _ => None,
        };
        if items.is_empty() {
            let Some(kind) = element else {
                return Err(self.error(span, "an empty list names its element type"));
            };
            return Ok((Type::List(kind), Symbol::None));
        }
        for item in items {
            let checked = self.expr(item, element.map(Type::Value))?;
            let Type::Value(kind) = checked else {
                return Err(self.error(
                    item.span,
                    format!("a list holds no {}", self.describe(checked)),
                ));
            };
            match element {
                Some(element) if element != kind => {
                    return Err(self.error(
                        item.span,
                        format!("the list holds {} and {}", element.name(), kind.name(),),
                    ));
                }
                _ => element = Some(kind),
            }
        }
        Ok((
            Type::List(element.expect("a list holds an element")),
            Symbol::None,
        ))
    }

    fn unary(&mut self, op: UnaryOp, operand: &Expr) -> Result<(Type, Symbol), Diagnostic> {
        match op {
            UnaryOp::Not => {
                let checked = self.expr(operand, Some(Type::BOOL))?;
                if checked != Type::BOOL {
                    return Err(self.mismatch(operand.span, Type::BOOL, checked));
                }
                Ok((Type::BOOL, Symbol::None))
            }
            UnaryOp::Negate => {
                let checked = self.expr(operand, None)?;
                match checked {
                    Type::Value(Kind::Int | Kind::Float | Kind::Vector) => {
                        Ok((checked, Symbol::None))
                    }
                    other => Err(self.error(
                        operand.span,
                        format!("{} takes no minus", self.describe(other)),
                    )),
                }
            }
        }
    }

    fn binary(
        &mut self,
        op: BinaryOp,
        lhs: &Expr,
        rhs: &Expr,
    ) -> Result<(Type, Symbol), Diagnostic> {
        let left = self.expr(lhs, None)?;
        let right = self.expr(rhs, Some(left))?;
        let ty = self.binary_type(op, left, right, lhs.span)?;
        Ok((ty, Symbol::None))
    }

    pub(crate) fn binary_type(
        &self,
        op: BinaryOp,
        left: Type,
        right: Type,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        use Kind::{Bool, Entity, Float, Int, Text, Vector};
        let value = Type::Value;
        let both = |kind: Kind| {
            if left == value(kind) && right == value(kind) {
                Ok(value(kind))
            } else {
                Err(self.operands(op, left, right, span))
            }
        };
        match op {
            BinaryOp::And | BinaryOp::Or => {
                if left == Type::BOOL && right == Type::BOOL {
                    Ok(Type::BOOL)
                } else {
                    Err(self.operands(op, left, right, span))
                }
            }
            BinaryOp::Equal | BinaryOp::NotEqual => {
                if left != right {
                    return Err(self.mismatch(span, left, right));
                }
                match left {
                    Type::Value(Bool | Int | Float | Text | Vector | Entity) => Ok(Type::BOOL),
                    other => Err(self.error(
                        span,
                        format!("{} compares equal to nothing", self.describe(other)),
                    )),
                }
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                match (left, right) {
                    (Type::Value(Int), Type::Value(Int)) => Ok(Type::BOOL),
                    (Type::Value(Float), Type::Value(Float)) => Ok(Type::BOOL),
                    _ => Err(self.operands(op, left, right, span)),
                }
            }
            BinaryOp::Add => match (left, right) {
                (Type::Value(Int), Type::Value(Int)) => Ok(value(Int)),
                (Type::Value(Float), Type::Value(Float)) => Ok(value(Float)),
                (Type::Value(Text), Type::Value(Text)) => Ok(value(Text)),
                (Type::Value(Vector), Type::Value(Vector)) => Ok(value(Vector)),
                _ => Err(self.operands(op, left, right, span)),
            },
            BinaryOp::Subtract => match (left, right) {
                (Type::Value(Int), Type::Value(Int)) => Ok(value(Int)),
                (Type::Value(Float), Type::Value(Float)) => Ok(value(Float)),
                (Type::Value(Vector), Type::Value(Vector)) => Ok(value(Vector)),
                _ => Err(self.operands(op, left, right, span)),
            },
            BinaryOp::Multiply => match (left, right) {
                (Type::Value(Int), Type::Value(Int)) => Ok(value(Int)),
                (Type::Value(Float), Type::Value(Float)) => Ok(value(Float)),
                (Type::Value(Vector), Type::Value(Float))
                | (Type::Value(Float), Type::Value(Vector)) => Ok(value(Vector)),
                _ => Err(self.operands(op, left, right, span)),
            },
            BinaryOp::Divide => match (left, right) {
                (Type::Value(Int), Type::Value(Int)) => Ok(value(Int)),
                (Type::Value(Float), Type::Value(Float)) => Ok(value(Float)),
                (Type::Value(Vector), Type::Value(Float)) => Ok(value(Vector)),
                _ => Err(self.operands(op, left, right, span)),
            },
            BinaryOp::Remainder => {
                if left != right {
                    return Err(self.mismatch(span, left, right));
                }
                both(Int).or_else(|_| both(Float))
            }
        }
    }

    fn operands(&self, op: BinaryOp, left: Type, right: Type, span: Span) -> Diagnostic {
        self.error(
            span,
            format!(
                "{} and {} hold no {}",
                self.describe(left),
                self.describe(right),
                op.text(),
            ),
        )
    }

    fn call(
        &mut self,
        callee: &Ident,
        arguments: &[Expr],
        span: Span,
    ) -> Result<(Type, Symbol), Diagnostic> {
        if let Some(intrinsic) = Intrinsic::named(&callee.name) {
            return self.intrinsic(intrinsic, arguments, span);
        }
        if let Some(index) = self.host_index.get(&callee.name).copied() {
            let host = &self.hosts[index as usize];
            let params = host.params.clone();
            let result = host.result;
            self.argument_count(callee, arguments, params.len())?;
            for (argument, param) in arguments.iter().zip(&params) {
                let checked = self.expr(argument, Some(*param))?;
                if checked != *param {
                    return Err(self.mismatch(argument.span, *param, checked));
                }
            }
            return Ok((result, Symbol::Host(index)));
        }
        if let Some(index) = self.function_index.get(&callee.name).copied() {
            let signature = &self.signatures[index as usize];
            let params = signature.params.clone();
            let access = signature.access.clone();
            let result = signature.result;
            self.argument_count(callee, arguments, params.len())?;
            for (position, argument) in arguments.iter().enumerate() {
                let param = params[position];
                match param {
                    Type::Component(component) => {
                        let required =
                            access[position].expect("a component parameter names its access");
                        self.component_argument(argument, component, required)?;
                    }
                    other => {
                        let checked = self.expr(argument, Some(other))?;
                        if checked != other {
                            return Err(self.mismatch(argument.span, other, checked));
                        }
                    }
                }
            }
            return Ok((result, Symbol::Function(index)));
        }
        Err(self.error(
            callee.span,
            format!("the source holds no function {}", callee.name),
        ))
    }

    fn argument_count(
        &self,
        callee: &Ident,
        arguments: &[Expr],
        expected: usize,
    ) -> Result<(), Diagnostic> {
        if arguments.len() == expected {
            return Ok(());
        }
        Err(self.error(
            callee.span,
            format!(
                "{} takes {} and {} stand here",
                callee.name,
                count(expected),
                count(arguments.len()),
            ),
        ))
    }

    fn component_argument(
        &mut self,
        argument: &Expr,
        component: u16,
        required: Access,
    ) -> Result<(), Diagnostic> {
        let checked = self.expr(argument, Some(Type::Component(component)))?;
        if checked != Type::Component(component) {
            return Err(self.mismatch(argument.span, Type::Component(component), checked));
        }
        let symbol = self.symbol(argument);
        let Symbol::Local(slot) = symbol else {
            return Err(self.error(argument.span, "a component passes out of a parameter"));
        };
        let Local::Component { access, .. } = self.locals[slot as usize] else {
            return Err(self.error(argument.span, "the value holds no component"));
        };
        if required == Access::Write && access == Access::Read {
            return Err(self.error(
                argument.span,
                format!(
                    "the {} component is read and written where it passes",
                    self.components[component as usize].name,
                ),
            ));
        }
        Ok(())
    }
}

fn count(count: usize) -> String {
    if count == 1 {
        String::from("one argument")
    } else {
        format!("{count} arguments")
    }
}

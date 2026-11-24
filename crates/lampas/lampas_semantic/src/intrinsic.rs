use crate::Type;
use crate::check::Checker;
use crate::symbol::{ExprInfo, Intrinsic, Symbol};
use lampas_syntax::{Diagnostic, Expr, ExprKind, Span};

impl Checker<'_> {
    pub(crate) fn intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        arguments: &[Expr],
        span: Span,
    ) -> Result<(Type, Symbol), Diagnostic> {
        let ty = match intrinsic {
            Intrinsic::Spawn => {
                self.intrinsic_count(intrinsic, arguments, 0, span)?;
                Type::ENTITY
            }
            Intrinsic::Despawn => {
                self.intrinsic_count(intrinsic, arguments, 1, span)?;
                self.expect(&arguments[0], Type::ENTITY)?;
                Type::VOID
            }
            Intrinsic::Add => {
                self.intrinsic_count(intrinsic, arguments, 2, span)?;
                self.expect(&arguments[0], Type::ENTITY)?;
                self.construct(&arguments[1])?;
                Type::VOID
            }
            Intrinsic::Remove => {
                self.intrinsic_count(intrinsic, arguments, 2, span)?;
                self.expect(&arguments[0], Type::ENTITY)?;
                self.component_name(&arguments[1])?;
                Type::VOID
            }
            Intrinsic::Has => {
                self.intrinsic_count(intrinsic, arguments, 2, span)?;
                self.expect(&arguments[0], Type::ENTITY)?;
                self.component_name(&arguments[1])?;
                Type::BOOL
            }
            Intrinsic::Vector => {
                self.intrinsic_count(intrinsic, arguments, 3, span)?;
                for argument in arguments {
                    self.expect(argument, Type::FLOAT)?;
                }
                Type::VECTOR
            }
            Intrinsic::Length => {
                self.intrinsic_count(intrinsic, arguments, 1, span)?;
                let checked = self.expr(&arguments[0], None)?;
                match checked {
                    Type::List(_) | Type::TEXT => Type::INT,
                    Type::VECTOR => Type::FLOAT,
                    other => {
                        return Err(self.error(
                            arguments[0].span,
                            format!("{} holds no length", self.describe(other)),
                        ));
                    }
                }
            }
            Intrinsic::Push => {
                self.intrinsic_count(intrinsic, arguments, 2, span)?;
                let checked = self.expr(&arguments[0], None)?;
                let Type::List(kind) = checked else {
                    return Err(self.error(
                        arguments[0].span,
                        format!("{} holds no items", self.describe(checked)),
                    ));
                };
                self.expect(&arguments[1], Type::Value(kind))?;
                Type::VOID
            }
            Intrinsic::Int => {
                self.intrinsic_count(intrinsic, arguments, 1, span)?;
                self.expect(&arguments[0], Type::FLOAT)?;
                Type::INT
            }
            Intrinsic::Float => {
                self.intrinsic_count(intrinsic, arguments, 1, span)?;
                self.expect(&arguments[0], Type::INT)?;
                Type::FLOAT
            }
            Intrinsic::Text => {
                self.intrinsic_count(intrinsic, arguments, 1, span)?;
                let checked = self.expr(&arguments[0], None)?;
                if !checked.is_value() {
                    return Err(self.error(
                        arguments[0].span,
                        format!("{} writes no text", self.describe(checked)),
                    ));
                }
                Type::TEXT
            }
            Intrinsic::SquareRoot
            | Intrinsic::Sine
            | Intrinsic::Cosine
            | Intrinsic::Floor
            | Intrinsic::Absolute => {
                self.intrinsic_count(intrinsic, arguments, 1, span)?;
                self.expect(&arguments[0], Type::FLOAT)?;
                Type::FLOAT
            }
            Intrinsic::Minimum | Intrinsic::Maximum => {
                self.intrinsic_count(intrinsic, arguments, 2, span)?;
                for argument in arguments {
                    self.expect(argument, Type::FLOAT)?;
                }
                Type::FLOAT
            }
            Intrinsic::Normalize => {
                self.intrinsic_count(intrinsic, arguments, 1, span)?;
                self.expect(&arguments[0], Type::VECTOR)?;
                Type::VECTOR
            }
            Intrinsic::Dot => {
                self.intrinsic_count(intrinsic, arguments, 2, span)?;
                for argument in arguments {
                    self.expect(argument, Type::VECTOR)?;
                }
                Type::FLOAT
            }
        };
        Ok((ty, Symbol::Intrinsic(intrinsic)))
    }

    fn intrinsic_count(
        &self,
        intrinsic: Intrinsic,
        arguments: &[Expr],
        expected: usize,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if arguments.len() == expected {
            return Ok(());
        }
        Err(self.error(
            span,
            format!(
                "{} takes {} and {} stand here",
                intrinsic.name(),
                expected,
                arguments.len(),
            ),
        ))
    }
    fn expect(&mut self, expr: &Expr, ty: Type) -> Result<(), Diagnostic> {
        let checked = self.expr(expr, Some(ty))?;
        if checked != ty {
            return Err(self.mismatch(expr.span, ty, checked));
        }
        Ok(())
    }

    fn component_name(&mut self, expr: &Expr) -> Result<u16, Diagnostic> {
        if let ExprKind::Name(ident) = &expr.kind
            && self.lookup(&ident.name).is_none()
            && !self.global_index.contains_key(&ident.name)
            && let Some(component) = self.component_index.get(&ident.name).copied()
        {
            self.infos.insert(
                expr.id,
                ExprInfo {
                    ty: Type::Component(component),
                    symbol: Symbol::Component(component),
                },
            );
            return Ok(component);
        }
        Err(self.error(expr.span, "a component name stands here"))
    }

    fn construct(&mut self, expr: &Expr) -> Result<u16, Diagnostic> {
        let ExprKind::Construct { name, fields } = &expr.kind else {
            return Err(self.error(expr.span, "add builds the component it adds"));
        };
        let Some(component) = self.component_index.get(&name.name).copied() else {
            return Err(self.error(
                name.span,
                format!("the source holds no component {}", name.name),
            ));
        };
        let definition = self.components[component as usize]
            .fields
            .iter()
            .map(|field| (field.name.clone(), field.kind, field.default.is_some()))
            .collect::<Vec<_>>();
        let mut seen = vec![false; definition.len()];
        for (field_name, value) in fields {
            let Some(index) = definition.iter().position(|held| held.0 == field_name.name) else {
                return Err(self.error(
                    field_name.span,
                    format!(
                        "the component {} holds no field {}",
                        name.name, field_name.name
                    ),
                ));
            };
            if std::mem::replace(&mut seen[index], true) {
                return Err(self.error(
                    field_name.span,
                    format!("the field {} stands twice", field_name.name),
                ));
            }
            self.expect(value, Type::Value(definition[index].1))?;
        }
        for (index, (field, _, defaulted)) in definition.iter().enumerate() {
            if !seen[index] && !defaulted {
                return Err(self.error(
                    expr.span,
                    format!("the component {} builds no field {field}", name.name),
                ));
            }
        }
        self.infos.insert(
            expr.id,
            ExprInfo {
                ty: Type::Component(component),
                symbol: Symbol::Component(component),
            },
        );
        Ok(component)
    }
}

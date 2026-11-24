use crate::ast::{
    Access, AssignOp, BinaryOp, Block, BootDecl, ComponentDecl, Expr, ExprKind, FieldDecl,
    FunctionDecl, GlobalDecl, Ident, Item, Numbering, ParamDecl, Phase, Program, Statement,
    StatementKind, SystemDecl, TypeExpr, TypeExprKind, UnaryOp,
};
use crate::span::{Diagnostic, Sources, Span};
use crate::token::{Keyword, Punctuation, Token, TokenKind};

impl Program {
    pub fn parse(sources: Sources) -> Result<Self, Diagnostic> {
        let mut numbering = Numbering::default();
        let mut items = Vec::new();
        for source in 0..sources.len() as u32 {
            let tokens = crate::lexer::tokenize(&sources, source)?;
            let parser = Parser::new(&sources, source, tokens, &mut numbering);
            items.extend(parser.items()?);
        }
        Ok(Self { sources, items })
    }
}

struct Parser<'a> {
    sources: &'a Sources,
    source: u32,
    tokens: Vec<Token>,
    at: usize,
    numbering: &'a mut Numbering,
    brace_ends_expression: bool,
}

impl<'a> Parser<'a> {
    fn new(
        sources: &'a Sources,
        source: u32,
        tokens: Vec<Token>,
        numbering: &'a mut Numbering,
    ) -> Self {
        Self {
            sources,
            source,
            tokens,
            at: 0,
            numbering,
            brace_ends_expression: false,
        }
    }

    fn items(mut self) -> Result<Vec<Item>, Diagnostic> {
        let mut items = Vec::new();
        while !self.at_end() {
            items.push(self.item()?);
        }
        Ok(items)
    }

    fn item(&mut self) -> Result<Item, Diagnostic> {
        match self.peek_keyword() {
            Some(Keyword::Component) => self.component().map(Item::Component),
            Some(Keyword::Global) => self.global().map(Item::Global),
            Some(Keyword::Function) => self.function().map(Item::Function),
            Some(Keyword::System) => self.system(Phase::Update).map(Item::System),
            Some(Keyword::Fixed) => {
                self.at += 1;
                self.system(Phase::Fixed).map(Item::System)
            }
            Some(Keyword::Boot) => self.boot().map(Item::Boot),
            _ => Err(self.error_here(
                "a declaration starts with component, global, fn, system, fixed or boot",
            )),
        }
    }

    fn component(&mut self) -> Result<ComponentDecl, Diagnostic> {
        self.expect_keyword(Keyword::Component)?;
        let name = self.expect_name()?;
        self.expect(Punctuation::LeftBrace)?;
        let mut fields = Vec::new();
        while self.peek_punctuation() != Some(Punctuation::RightBrace) {
            fields.push(self.field()?);
            if self.peek_punctuation() == Some(Punctuation::Comma) {
                self.at += 1;
            } else {
                break;
            }
        }
        self.expect(Punctuation::RightBrace)?;
        Ok(ComponentDecl { name, fields })
    }

    fn field(&mut self) -> Result<FieldDecl, Diagnostic> {
        let name = self.expect_name()?;
        self.expect(Punctuation::Colon)?;
        let ty = self.type_expr()?;
        let default = match self.peek_punctuation() {
            Some(Punctuation::Equal) => {
                self.at += 1;
                Some(self.expression()?)
            }
            _ => None,
        };
        Ok(FieldDecl { name, ty, default })
    }

    fn global(&mut self) -> Result<GlobalDecl, Diagnostic> {
        self.expect_keyword(Keyword::Global)?;
        let name = self.expect_name()?;
        self.expect(Punctuation::Colon)?;
        let ty = self.type_expr()?;
        self.expect(Punctuation::Equal)?;
        let value = self.expression()?;
        self.expect(Punctuation::Semicolon)?;
        Ok(GlobalDecl { name, ty, value })
    }

    fn function(&mut self) -> Result<FunctionDecl, Diagnostic> {
        self.expect_keyword(Keyword::Function)?;
        let name = self.expect_name()?;
        let params = self.params()?;
        let result = match self.peek_punctuation() {
            Some(Punctuation::Arrow) => {
                self.at += 1;
                Some(self.type_expr()?)
            }
            _ => None,
        };
        let body = self.block()?;
        Ok(FunctionDecl {
            name,
            params,
            result,
            body,
        })
    }

    fn system(&mut self, phase: Phase) -> Result<SystemDecl, Diagnostic> {
        self.expect_keyword(Keyword::System)?;
        let name = self.expect_name()?;
        let params = self.params()?;
        let body = self.block()?;
        Ok(SystemDecl {
            name,
            phase,
            params,
            body,
        })
    }

    fn boot(&mut self) -> Result<BootDecl, Diagnostic> {
        self.expect_keyword(Keyword::Boot)?;
        let body = self.block()?;
        Ok(BootDecl { body })
    }

    fn params(&mut self) -> Result<Vec<ParamDecl>, Diagnostic> {
        self.expect(Punctuation::LeftParen)?;
        let mut params = Vec::new();
        while self.peek_punctuation() != Some(Punctuation::RightParen) {
            let name = self.expect_name()?;
            self.expect(Punctuation::Colon)?;
            let access = match self.peek_keyword() {
                Some(Keyword::Read) => {
                    self.at += 1;
                    Some(Access::Read)
                }
                Some(Keyword::Write) => {
                    self.at += 1;
                    Some(Access::Write)
                }
                _ => None,
            };
            let ty = self.type_expr()?;
            params.push(ParamDecl { name, access, ty });
            if self.peek_punctuation() == Some(Punctuation::Comma) {
                self.at += 1;
            } else {
                break;
            }
        }
        self.expect(Punctuation::RightParen)?;
        Ok(params)
    }

    fn type_expr(&mut self) -> Result<TypeExpr, Diagnostic> {
        let token = self.peek().clone();
        let span = token.span;
        let kind = match token.kind {
            TokenKind::Keyword(Keyword::Bool) => TypeExprKind::Bool,
            TokenKind::Keyword(Keyword::Int) => TypeExprKind::Int,
            TokenKind::Keyword(Keyword::Float) => TypeExprKind::Float,
            TokenKind::Keyword(Keyword::String) => TypeExprKind::Text,
            TokenKind::Keyword(Keyword::Vector) => TypeExprKind::Vector,
            TokenKind::Keyword(Keyword::Entity) => TypeExprKind::Entity,
            TokenKind::Keyword(Keyword::List) => {
                self.at += 1;
                self.expect(Punctuation::Less)?;
                let element = self.type_expr()?;
                self.expect(Punctuation::Greater)?;
                return Ok(TypeExpr {
                    span: self.span_until(span),
                    kind: TypeExprKind::List(Box::new(element)),
                });
            }
            TokenKind::Identifier(name) => {
                self.at += 1;
                return Ok(TypeExpr {
                    span,
                    kind: TypeExprKind::Named(Ident { span, name }),
                });
            }
            _ => {
                return Err(self.error_here(format!(
                    "{} stands where a type belongs",
                    token.kind.describe(),
                )));
            }
        };
        self.at += 1;
        Ok(TypeExpr { span, kind })
    }

    fn block(&mut self) -> Result<Block, Diagnostic> {
        let start = self.expect(Punctuation::LeftBrace)?;
        let mut statements = Vec::new();
        while self.peek_punctuation() != Some(Punctuation::RightBrace) {
            if self.at_end() {
                return Err(self.error(
                    start,
                    "the block ends with the source instead of a closing brace",
                ));
            }
            statements.push(self.statement()?);
        }
        let end = self.expect(Punctuation::RightBrace)?;
        Ok(Block {
            span: self.span_pair(start, end),
            statements,
        })
    }

    fn statement(&mut self) -> Result<Statement, Diagnostic> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Keyword(Keyword::Let) => self.let_statement(),
            TokenKind::Keyword(Keyword::If) => self.if_statement(),
            TokenKind::Keyword(Keyword::While) => self.while_statement(),
            TokenKind::Keyword(Keyword::For) => self.for_statement(),
            TokenKind::Keyword(Keyword::Return) => self.return_statement(),
            TokenKind::Keyword(Keyword::Break) => {
                self.at += 1;
                self.expect(Punctuation::Semicolon)?;
                Ok(self.statement_at(token.span, StatementKind::Break))
            }
            TokenKind::Keyword(Keyword::Continue) => {
                self.at += 1;
                self.expect(Punctuation::Semicolon)?;
                Ok(self.statement_at(token.span, StatementKind::Continue))
            }
            _ => self.call_statement(),
        }
    }

    fn let_statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_keyword(Keyword::Let)?;
        let name = self.expect_name()?;
        let ty = match self.peek_punctuation() {
            Some(Punctuation::Colon) => {
                self.at += 1;
                Some(self.type_expr()?)
            }
            _ => None,
        };
        self.expect(Punctuation::Equal)?;
        let value = self.expression()?;
        let end = self.expect(Punctuation::Semicolon)?;
        Ok(self.statement_at(
            self.span_pair(start, end),
            StatementKind::Let { name, ty, value },
        ))
    }

    fn if_statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_keyword(Keyword::If)?;
        let condition = self.header_expression()?;
        let then = self.block()?;
        let otherwise = match self.peek_keyword() {
            Some(Keyword::Else) => {
                self.at += 1;
                match self.peek_keyword() {
                    Some(Keyword::If) => {
                        let nested = self.if_statement()?;
                        Some(Block {
                            span: nested.span,
                            statements: vec![nested],
                        })
                    }
                    _ => Some(self.block()?),
                }
            }
            _ => None,
        };
        let end = otherwise.as_ref().map_or(then.span, |block| block.span);
        Ok(self.statement_at(
            self.span_pair(start, end),
            StatementKind::If {
                condition,
                then,
                otherwise,
            },
        ))
    }

    fn while_statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_keyword(Keyword::While)?;
        let condition = self.header_expression()?;
        let body = self.block()?;
        let span = self.span_pair(start, body.span);
        Ok(self.statement_at(span, StatementKind::While { condition, body }))
    }

    fn for_statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_keyword(Keyword::For)?;
        let name = self.expect_name()?;
        self.expect_keyword(Keyword::In)?;
        let list = self.header_expression()?;
        let body = self.block()?;
        let span = self.span_pair(start, body.span);
        Ok(self.statement_at(span, StatementKind::For { name, list, body }))
    }

    fn return_statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_keyword(Keyword::Return)?;
        if self.peek_punctuation() == Some(Punctuation::Semicolon) {
            let end = self.advance();
            return Ok(self.statement_at(self.span_pair(start, end), StatementKind::Return(None)));
        }
        let value = self.expression()?;
        let end = self.expect(Punctuation::Semicolon)?;
        Ok(self.statement_at(
            self.span_pair(start, end),
            StatementKind::Return(Some(value)),
        ))
    }

    fn call_statement(&mut self) -> Result<Statement, Diagnostic> {
        let target = self.expression()?;
        let op = match self.peek_punctuation() {
            Some(Punctuation::Equal) => Some(AssignOp::Set),
            Some(Punctuation::PlusEqual) => Some(AssignOp::Add),
            Some(Punctuation::MinusEqual) => Some(AssignOp::Subtract),
            Some(Punctuation::StarEqual) => Some(AssignOp::Multiply),
            Some(Punctuation::SlashEqual) => Some(AssignOp::Divide),
            Some(Punctuation::PercentEqual) => Some(AssignOp::Remainder),
            _ => None,
        };
        if let Some(op) = op {
            self.at += 1;
            let value = self.expression()?;
            let end = self.expect(Punctuation::Semicolon)?;
            return Ok(self.statement_at(
                self.span_pair(target.span, end),
                StatementKind::Set { target, op, value },
            ));
        }
        let end = self.expect(Punctuation::Semicolon)?;
        let span = self.span_pair(target.span, end);
        if !matches!(target.kind, ExprKind::Call { .. }) {
            return Err(self.error(span, "a statement either sets a place or calls a function"));
        }
        Ok(self.statement_at(span, StatementKind::Call(target)))
    }

    fn header_expression(&mut self) -> Result<Expr, Diagnostic> {
        let held = std::mem::replace(&mut self.brace_ends_expression, true);
        let expression = self.expression();
        self.brace_ends_expression = held;
        expression
    }

    fn expression(&mut self) -> Result<Expr, Diagnostic> {
        self.or_expression()
    }

    fn or_expression(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.and_expression()?;
        while self.peek_punctuation() == Some(Punctuation::OrOr) {
            self.at += 1;
            let rhs = self.and_expression()?;
            lhs = self.binary(BinaryOp::Or, lhs, rhs);
        }
        Ok(lhs)
    }

    fn and_expression(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.equality()?;
        while self.peek_punctuation() == Some(Punctuation::AndAnd) {
            self.at += 1;
            let rhs = self.equality()?;
            lhs = self.binary(BinaryOp::And, lhs, rhs);
        }
        Ok(lhs)
    }

    fn equality(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.comparison()?;
        loop {
            let op = match self.peek_punctuation() {
                Some(Punctuation::EqualEqual) => BinaryOp::Equal,
                Some(Punctuation::BangEqual) => BinaryOp::NotEqual,
                _ => return Ok(lhs),
            };
            self.at += 1;
            let rhs = self.comparison()?;
            lhs = self.binary(op, lhs, rhs);
        }
    }

    fn comparison(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.term()?;
        loop {
            let op = match self.peek_punctuation() {
                Some(Punctuation::Less) => BinaryOp::Less,
                Some(Punctuation::LessEqual) => BinaryOp::LessEqual,
                Some(Punctuation::Greater) => BinaryOp::Greater,
                Some(Punctuation::GreaterEqual) => BinaryOp::GreaterEqual,
                _ => return Ok(lhs),
            };
            self.at += 1;
            let rhs = self.term()?;
            lhs = self.binary(op, lhs, rhs);
        }
    }

    fn term(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.factor()?;
        loop {
            let op = match self.peek_punctuation() {
                Some(Punctuation::Plus) => BinaryOp::Add,
                Some(Punctuation::Minus) => BinaryOp::Subtract,
                _ => return Ok(lhs),
            };
            self.at += 1;
            let rhs = self.factor()?;
            lhs = self.binary(op, lhs, rhs);
        }
    }

    fn factor(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek_punctuation() {
                Some(Punctuation::Star) => BinaryOp::Multiply,
                Some(Punctuation::Slash) => BinaryOp::Divide,
                Some(Punctuation::Percent) => BinaryOp::Remainder,
                _ => return Ok(lhs),
            };
            self.at += 1;
            let rhs = self.unary()?;
            lhs = self.binary(op, lhs, rhs);
        }
    }

    fn unary(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.peek().clone();
        let op = match token.kind {
            TokenKind::Symbol(Punctuation::Minus) => UnaryOp::Negate,
            TokenKind::Symbol(Punctuation::Bang) => UnaryOp::Not,
            _ => return self.postfix(),
        };
        self.at += 1;
        let operand = self.unary()?;
        let span = self.span_pair(token.span, operand.span);
        Ok(self.expr(
            span,
            ExprKind::Unary {
                op,
                operand: Box::new(operand),
            },
        ))
    }

    fn postfix(&mut self) -> Result<Expr, Diagnostic> {
        let mut base = self.primary()?;
        loop {
            match self.peek_punctuation() {
                Some(Punctuation::Dot) => {
                    self.at += 1;
                    let name = self.expect_name()?;
                    let span = self.span_pair(base.span, name.span);
                    base = self.expr(
                        span,
                        ExprKind::Field {
                            base: Box::new(base),
                            name,
                        },
                    );
                }
                Some(Punctuation::LeftBracket) => {
                    self.at += 1;
                    let index = self.nested_expression()?;
                    let end = self.expect(Punctuation::RightBracket)?;
                    let span = self.span_pair(base.span, end);
                    base = self.expr(
                        span,
                        ExprKind::Index {
                            base: Box::new(base),
                            index: Box::new(index),
                        },
                    );
                }
                _ => return Ok(base),
            }
        }
    }

    fn primary(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.peek().clone();
        let span = token.span;
        match token.kind {
            TokenKind::Int(value) => {
                self.at += 1;
                Ok(self.expr(span, ExprKind::Int(value)))
            }
            TokenKind::Float(value) => {
                self.at += 1;
                Ok(self.expr(span, ExprKind::Float(value)))
            }
            TokenKind::Text(text) => {
                self.at += 1;
                Ok(self.expr(span, ExprKind::Text(text)))
            }
            TokenKind::Keyword(Keyword::True) => {
                self.at += 1;
                Ok(self.expr(span, ExprKind::Bool(true)))
            }
            TokenKind::Keyword(Keyword::False) => {
                self.at += 1;
                Ok(self.expr(span, ExprKind::Bool(false)))
            }
            TokenKind::Keyword(Keyword::SelfEntity) => {
                self.at += 1;
                Ok(self.expr(span, ExprKind::SelfEntity))
            }
            TokenKind::Keyword(keyword @ (Keyword::Int | Keyword::Float | Keyword::Vector))
                if self.look_punctuation() == Some(Punctuation::LeftParen) =>
            {
                self.at += 1;
                let callee = Ident {
                    span,
                    name: String::from(keyword.name()),
                };
                let arguments = self.arguments()?;
                let end = self.previous_span();
                Ok(self.expr(
                    self.span_pair(span, end),
                    ExprKind::Call { callee, arguments },
                ))
            }
            TokenKind::Identifier(name) => {
                self.at += 1;
                let callee = Ident { span, name };
                match self.peek_punctuation() {
                    Some(Punctuation::LeftParen) => {
                        let arguments = self.arguments()?;
                        let end = self.previous_span();
                        Ok(self.expr(
                            self.span_pair(span, end),
                            ExprKind::Call { callee, arguments },
                        ))
                    }
                    Some(Punctuation::LeftBrace) if !self.brace_ends_expression => {
                        self.construct(callee)
                    }
                    _ => Ok(self.expr(span, ExprKind::Name(callee))),
                }
            }
            TokenKind::Symbol(Punctuation::LeftParen) => {
                self.at += 1;
                let inner = self.nested_expression()?;
                self.expect(Punctuation::RightParen)?;
                Ok(inner)
            }
            TokenKind::Symbol(Punctuation::LeftBracket) => {
                self.at += 1;
                let mut items = Vec::new();
                while self.peek_punctuation() != Some(Punctuation::RightBracket) {
                    items.push(self.nested_expression()?);
                    if self.peek_punctuation() == Some(Punctuation::Comma) {
                        self.at += 1;
                    } else {
                        break;
                    }
                }
                let end = self.expect(Punctuation::RightBracket)?;
                Ok(self.expr(self.span_pair(span, end), ExprKind::List(items)))
            }
            _ => Err(self.error_here(format!("{} starts no value", token.kind.describe(),))),
        }
    }

    fn construct(&mut self, name: Ident) -> Result<Expr, Diagnostic> {
        self.expect(Punctuation::LeftBrace)?;
        let mut fields = Vec::new();
        while self.peek_punctuation() != Some(Punctuation::RightBrace) {
            let field = self.expect_name()?;
            self.expect(Punctuation::Colon)?;
            let value = self.nested_expression()?;
            fields.push((field, value));
            if self.peek_punctuation() == Some(Punctuation::Comma) {
                self.at += 1;
            } else {
                break;
            }
        }
        let end = self.expect(Punctuation::RightBrace)?;
        Ok(self.expr(
            self.span_pair(name.span, end),
            ExprKind::Construct { name, fields },
        ))
    }

    fn arguments(&mut self) -> Result<Vec<Expr>, Diagnostic> {
        self.expect(Punctuation::LeftParen)?;
        let mut arguments = Vec::new();
        while self.peek_punctuation() != Some(Punctuation::RightParen) {
            arguments.push(self.nested_expression()?);
            if self.peek_punctuation() == Some(Punctuation::Comma) {
                self.at += 1;
            } else {
                break;
            }
        }
        self.expect(Punctuation::RightParen)?;
        Ok(arguments)
    }

    fn nested_expression(&mut self) -> Result<Expr, Diagnostic> {
        let held = std::mem::replace(&mut self.brace_ends_expression, false);
        let expression = self.expression();
        self.brace_ends_expression = held;
        expression
    }

    fn binary(&mut self, op: BinaryOp, lhs: Expr, rhs: Expr) -> Expr {
        let span = self.span_pair(lhs.span, rhs.span);
        self.expr(
            span,
            ExprKind::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
        )
    }

    fn expr(&mut self, span: Span, kind: ExprKind) -> Expr {
        Expr {
            id: self.numbering.expr(),
            span,
            kind,
        }
    }

    fn statement_at(&mut self, span: Span, kind: StatementKind) -> Statement {
        Statement {
            id: self.numbering.statement(),
            span,
            kind,
        }
    }

    fn span_pair(&self, start: Span, end: Span) -> Span {
        Span {
            source: self.source,
            start: start.start,
            end: end.end.max(start.end),
        }
    }

    fn span_until(&self, start: Span) -> Span {
        Span {
            source: self.source,
            start: start.start,
            end: self.tokens[self.at.saturating_sub(1)]
                .span
                .end
                .max(start.end),
        }
    }

    fn previous_span(&self) -> Span {
        self.tokens[self.at.saturating_sub(1)].span
    }

    fn advance(&mut self) -> Span {
        let span = self.peek().span;
        self.at += 1;
        span
    }

    fn expect(&mut self, punctuation: Punctuation) -> Result<Span, Diagnostic> {
        if self.peek_punctuation() == Some(punctuation) {
            return Ok(self.advance());
        }
        Err(self.error_here(format!(
            "{} stands where {} belongs",
            self.peek_kind().describe(),
            punctuation.text(),
        )))
    }

    fn expect_keyword(&mut self, keyword: Keyword) -> Result<Span, Diagnostic> {
        if self.peek_keyword() == Some(keyword) {
            return Ok(self.advance());
        }
        Err(self.error_here(format!(
            "{} stands where {} belongs",
            self.peek_kind().describe(),
            keyword.name(),
        )))
    }

    fn expect_name(&mut self) -> Result<Ident, Diagnostic> {
        if let TokenKind::Identifier(name) = self.peek_kind() {
            let span = self.peek().span;
            self.at += 1;
            return Ok(Ident { span, name });
        }
        Err(self.error_here(format!(
            "{} stands where a name belongs",
            self.peek_kind().describe(),
        )))
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.at]
    }

    fn peek_kind(&self) -> TokenKind {
        self.peek().kind.clone()
    }

    fn peek_keyword(&self) -> Option<Keyword> {
        match self.peek().kind {
            TokenKind::Keyword(keyword) => Some(keyword),
            _ => None,
        }
    }

    fn peek_punctuation(&self) -> Option<Punctuation> {
        match self.peek().kind {
            TokenKind::Symbol(punctuation) => Some(punctuation),
            _ => None,
        }
    }

    fn look_punctuation(&self) -> Option<Punctuation> {
        match self.tokens.get(self.at + 1)?.kind {
            TokenKind::Symbol(punctuation) => Some(punctuation),
            _ => None,
        }
    }

    fn at_end(&self) -> bool {
        matches!(self.peek().kind, TokenKind::End)
    }

    fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        self.sources.diagnostic(span, message)
    }

    fn error_here(&self, message: impl Into<String>) -> Diagnostic {
        self.error(self.peek().span, message)
    }
}

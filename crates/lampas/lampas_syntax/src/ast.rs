use crate::span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExprId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StmtId(pub u32);

#[derive(Default)]
pub struct Numbering {
    exprs: u32,
    statements: u32,
}

impl Numbering {
    pub fn expr(&mut self) -> ExprId {
        let id = ExprId(self.exprs);
        self.exprs += 1;
        id
    }

    pub fn statement(&mut self) -> StmtId {
        let id = StmtId(self.statements);
        self.statements += 1;
        id
    }
}

#[derive(Clone, Debug)]
pub struct Ident {
    pub span: Span,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Program {
    pub sources: crate::span::Sources,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug)]
pub enum Item {
    Component(ComponentDecl),
    Global(GlobalDecl),
    Function(FunctionDecl),
    System(SystemDecl),
    Boot(BootDecl),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Self::Component(decl) => decl.name.span,
            Self::Global(decl) => decl.name.span,
            Self::Function(decl) => decl.name.span,
            Self::System(decl) => decl.name.span,
            Self::Boot(decl) => decl.body.span,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Component(decl) => &decl.name.name,
            Self::Global(decl) => &decl.name.name,
            Self::Function(decl) => &decl.name.name,
            Self::System(decl) => &decl.name.name,
            Self::Boot(_) => "boot",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ComponentDecl {
    pub name: Ident,
    pub fields: Vec<FieldDecl>,
}

#[derive(Clone, Debug)]
pub struct FieldDecl {
    pub name: Ident,
    pub ty: TypeExpr,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct GlobalDecl {
    pub name: Ident,
    pub ty: TypeExpr,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub name: Ident,
    pub params: Vec<ParamDecl>,
    pub result: Option<TypeExpr>,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct SystemDecl {
    pub name: Ident,
    pub phase: Phase,
    pub params: Vec<ParamDecl>,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct BootDecl {
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct ParamDecl {
    pub name: Ident,
    pub access: Option<Access>,
    pub ty: TypeExpr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Update,
    Fixed,
}

#[derive(Clone, Debug)]
pub struct TypeExpr {
    pub span: Span,
    pub kind: TypeExprKind,
}

#[derive(Clone, Debug)]
pub enum TypeExprKind {
    Bool,
    Int,
    Float,
    Text,
    Vector,
    Entity,
    List(Box<TypeExpr>),
    Named(Ident),
}

#[derive(Clone, Debug)]
pub struct Block {
    pub span: Span,
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub struct Statement {
    pub id: StmtId,
    pub span: Span,
    pub kind: StatementKind,
}

#[derive(Clone, Debug)]
pub enum StatementKind {
    Let {
        name: Ident,
        ty: Option<TypeExpr>,
        value: Expr,
    },
    Set {
        target: Expr,
        op: AssignOp,
        value: Expr,
    },
    If {
        condition: Expr,
        then: Block,
        otherwise: Option<Block>,
    },
    While {
        condition: Expr,
        body: Block,
    },
    For {
        name: Ident,
        list: Expr,
        body: Block,
    },
    Return(Option<Expr>),
    Break,
    Continue,
    Call(Expr),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub id: ExprId,
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
    Name(Ident),
    SelfEntity,
    Field {
        base: Box<Expr>,
        name: Ident,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Call {
        callee: Ident,
        arguments: Vec<Expr>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    List(Vec<Expr>),
    Construct {
        name: Ident,
        fields: Vec<(Ident, Expr)>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

impl BinaryOp {
    pub fn text(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Remainder => "%",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
            Self::And => "&&",
            Self::Or => "||",
        }
    }
}

impl AssignOp {
    pub fn binary(self) -> Option<BinaryOp> {
        Some(match self {
            Self::Set => return None,
            Self::Add => BinaryOp::Add,
            Self::Subtract => BinaryOp::Subtract,
            Self::Multiply => BinaryOp::Multiply,
            Self::Divide => BinaryOp::Divide,
            Self::Remainder => BinaryOp::Remainder,
        })
    }

    pub fn text(self) -> &'static str {
        match self {
            Self::Set => "=",
            Self::Add => "+=",
            Self::Subtract => "-=",
            Self::Multiply => "*=",
            Self::Divide => "/=",
            Self::Remainder => "%=",
        }
    }
}

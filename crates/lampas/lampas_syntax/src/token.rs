use crate::span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyword {
    Component,
    System,
    Schedule,
    Update,
    Fixed,
    Boot,
    Global,
    Function,
    Let,
    If,
    Else,
    While,
    For,
    In,
    Return,
    Break,
    Continue,
    True,
    False,
    Read,
    Write,
    SelfEntity,
    Bool,
    Int,
    Float,
    String,
    Vector,
    Entity,
    List,
}

impl Keyword {
    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "component" => Self::Component,
            "system" => Self::System,
            "schedule" => Self::Schedule,
            "update" => Self::Update,
            "fixed" => Self::Fixed,
            "boot" => Self::Boot,
            "global" => Self::Global,
            "fn" => Self::Function,
            "let" => Self::Let,
            "if" => Self::If,
            "else" => Self::Else,
            "while" => Self::While,
            "for" => Self::For,
            "in" => Self::In,
            "return" => Self::Return,
            "break" => Self::Break,
            "continue" => Self::Continue,
            "true" => Self::True,
            "false" => Self::False,
            "read" => Self::Read,
            "write" => Self::Write,
            "self" => Self::SelfEntity,
            "bool" => Self::Bool,
            "int" => Self::Int,
            "float" => Self::Float,
            "string" => Self::String,
            "vec3" => Self::Vector,
            "entity" => Self::Entity,
            "list" => Self::List,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Component => "component",
            Self::System => "system",
            Self::Schedule => "schedule",
            Self::Update => "update",
            Self::Fixed => "fixed",
            Self::Boot => "boot",
            Self::Global => "global",
            Self::Function => "fn",
            Self::Let => "let",
            Self::If => "if",
            Self::Else => "else",
            Self::While => "while",
            Self::For => "for",
            Self::In => "in",
            Self::Return => "return",
            Self::Break => "break",
            Self::Continue => "continue",
            Self::True => "true",
            Self::False => "false",
            Self::Read => "read",
            Self::Write => "write",
            Self::SelfEntity => "self",
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Float => "float",
            Self::String => "string",
            Self::Vector => "vec3",
            Self::Entity => "entity",
            Self::List => "list",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Punctuation {
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Comma,
    Semicolon,
    Colon,
    Dot,
    Arrow,
    Equal,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    PercentEqual,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    AndAnd,
    OrOr,
    Bang,
}

impl Punctuation {
    pub fn text(self) -> &'static str {
        match self {
            Self::LeftBrace => "{",
            Self::RightBrace => "}",
            Self::LeftParen => "(",
            Self::RightParen => ")",
            Self::LeftBracket => "[",
            Self::RightBracket => "]",
            Self::Comma => ",",
            Self::Semicolon => ";",
            Self::Colon => ":",
            Self::Dot => ".",
            Self::Arrow => "->",
            Self::Equal => "=",
            Self::PlusEqual => "+=",
            Self::MinusEqual => "-=",
            Self::StarEqual => "*=",
            Self::SlashEqual => "/=",
            Self::PercentEqual => "%=",
            Self::EqualEqual => "==",
            Self::BangEqual => "!=",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Star => "*",
            Self::Slash => "/",
            Self::Percent => "%",
            Self::AndAnd => "&&",
            Self::OrOr => "||",
            Self::Bang => "!",
        }
    }

    pub fn from_char(held: char) -> Option<Self> {
        Some(match held {
            '{' => Self::LeftBrace,
            '}' => Self::RightBrace,
            '(' => Self::LeftParen,
            ')' => Self::RightParen,
            '[' => Self::LeftBracket,
            ']' => Self::RightBracket,
            ',' => Self::Comma,
            ';' => Self::Semicolon,
            ':' => Self::Colon,
            '.' => Self::Dot,
            '=' => Self::Equal,
            '<' => Self::Less,
            '>' => Self::Greater,
            '+' => Self::Plus,
            '-' => Self::Minus,
            '*' => Self::Star,
            '/' => Self::Slash,
            '%' => Self::Percent,
            '&' => Self::AndAnd,
            '|' => Self::OrOr,
            '!' => Self::Bang,
            _ => return None,
        })
    }

    pub(crate) fn paired(self, held: char) -> Option<Self> {
        Some(match (self, held) {
            (Self::Equal, '=') => Self::EqualEqual,
            (Self::Bang, '=') => Self::BangEqual,
            (Self::Less, '=') => Self::LessEqual,
            (Self::Greater, '=') => Self::GreaterEqual,
            (Self::Plus, '=') => Self::PlusEqual,
            (Self::Minus, '=') => Self::MinusEqual,
            (Self::Star, '=') => Self::StarEqual,
            (Self::Slash, '=') => Self::SlashEqual,
            (Self::Percent, '=') => Self::PercentEqual,
            (Self::AndAnd, '&') => Self::AndAnd,
            (Self::OrOr, '|') => Self::OrOr,
            (Self::Minus, '>') => Self::Arrow,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Int(i64),
    Float(f64),
    Text(String),
    Keyword(Keyword),
    Symbol(Punctuation),
    End,
}

impl TokenKind {
    pub fn describe(&self) -> String {
        match self {
            Self::Identifier(name) => format!("the name {name}"),
            Self::Int(value) => format!("the whole number {value}"),
            Self::Float(value) => format!("the number {value}"),
            Self::Text(_) => String::from("a text"),
            Self::Keyword(keyword) => format!("the word {}", keyword.name()),
            Self::Symbol(held) => format!("the sign {}", held.text()),
            Self::End => String::from("the end of the source"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

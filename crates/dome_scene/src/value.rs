use crate::field::zero;
use dome_math::{Vec3, Vec4};
use dome_toml::Toml;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f32),
    Bool(bool),
    Text(String),
    Vector(Vec3),
    Color(Vec4),
}

impl Value {
    pub fn number(&self) -> Result<f32, String> {
        match self {
            Self::Number(value) => Ok(*value),
            other => Err(format!("a number is read from a {}", other.kind())),
        }
    }

    pub fn boolean(&self) -> Result<bool, String> {
        match self {
            Self::Bool(value) => Ok(*value),
            other => Err(format!("a boolean is read from a {}", other.kind())),
        }
    }

    pub fn text(&self) -> Result<&str, String> {
        match self {
            Self::Text(value) => Ok(value),
            other => Err(format!("a text is read from a {}", other.kind())),
        }
    }

    pub fn vector(&self) -> Result<Vec3, String> {
        match self {
            Self::Vector(value) => Ok(*value),
            other => Err(format!("a vector is read from a {}", other.kind())),
        }
    }

    pub fn rgba(&self) -> Result<Vec4, String> {
        match self {
            Self::Color(value) => Ok(*value),
            Self::Vector(value) => Ok(Vec4::new(value.x, value.y, value.z, 1.0)),
            other => Err(format!("a color is read from a {}", other.kind())),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Number(_) => "number",
            Self::Bool(_) => "boolean",
            Self::Text(_) => "text",
            Self::Vector(_) => "vector",
            Self::Color(_) => "color",
        }
    }

    pub fn parse(value: Toml, what: &str) -> Self {
        if value.is_number() {
            return Self::Number(value.f32_value(what));
        }
        if value.is_boolean() {
            return Self::Bool(value.boolean_value(what));
        }
        if value.is_string() {
            return Self::Text(value.string_value(what));
        }
        if value.is_array() {
            let numbers = value
                .array_items(what)
                .into_iter()
                .map(|item| item.f32_value(what))
                .collect::<Vec<_>>();
            return match numbers.as_slice() {
                [x, y, z] => Self::Vector(Vec3::new(*x, *y, *z)),
                [x, y, z, w] => Self::Color(Vec4::new(*x, *y, *z, *w)),
                _ => panic!("{what} holds {} numbers as a value", numbers.len()),
            };
        }
        panic!("{what} holds {} as a value", value.kind());
    }

    pub(crate) fn to_toml(&self) -> Toml {
        match self {
            Self::Number(value) => Toml::from(zero(*value)),
            Self::Bool(value) => Toml::boolean(*value),
            Self::Text(value) => Toml::string(value.as_str()),
            Self::Vector(value) => Toml::array(
                value
                    .to_array()
                    .into_iter()
                    .map(|held| Toml::from(zero(held)))
                    .collect(),
            ),
            Self::Color(value) => Toml::array(
                value
                    .to_array()
                    .into_iter()
                    .map(|held| Toml::from(zero(held)))
                    .collect(),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Value,
    Structure,
}

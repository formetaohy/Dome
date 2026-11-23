mod fields;
mod number;
mod parse;
mod write;

pub use fields::{Fields, floats};

use toml::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Toml(Value);

impl Toml {
    pub fn parse(text: &str, what: &str) -> Self {
        Self(parse::document(text, what))
    }

    pub fn string(value: impl Into<String>) -> Self {
        Self(Value::String(value.into()))
    }

    pub fn boolean(value: bool) -> Self {
        Self(Value::Boolean(value))
    }

    pub fn float(value: f32) -> Self {
        Self(Value::Float(number::printed(value)))
    }

    pub fn whole(value: u64) -> Self {
        Self(Value::Integer(number::whole(value)))
    }

    pub fn array(items: Vec<Toml>) -> Self {
        Self(Value::Array(items.into_iter().map(|item| item.0).collect()))
    }

    pub fn object(fields: impl IntoIterator<Item = (impl Into<String>, Toml)>) -> Self {
        let mut table = toml::Table::new();
        for (key, value) in fields {
            let key = key.into();
            assert!(
                table.insert(key.clone(), value.0).is_none(),
                "a TOML table holds the key {key} twice",
            );
        }
        Self(Value::Table(table))
    }

    pub fn kind(&self) -> &'static str {
        match self.0 {
            Value::String(_) => "a string",
            Value::Integer(_) => "an integer",
            Value::Float(_) => "a float",
            Value::Boolean(_) => "a boolean",
            Value::Datetime(_) => "a date-time",
            Value::Array(_) => "an array",
            Value::Table(_) => "a table",
        }
    }

    pub fn is_string(&self) -> bool {
        matches!(self.0, Value::String(_))
    }

    pub fn is_number(&self) -> bool {
        matches!(self.0, Value::Integer(_) | Value::Float(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self.0, Value::Array(_))
    }

    pub fn is_boolean(&self) -> bool {
        matches!(self.0, Value::Boolean(_))
    }

    pub fn is_table(&self) -> bool {
        matches!(self.0, Value::Table(_))
    }

    pub fn object_fields(self, what: &str) -> Vec<(String, Toml)> {
        match self.0 {
            Value::Table(fields) => fields
                .into_iter()
                .map(|(key, value)| (key, Self(value)))
                .collect(),
            other => panic!("{what} holds {} where it holds a table", Self(other).kind()),
        }
    }

    pub fn array_items(self, what: &str) -> Vec<Toml> {
        match self.0 {
            Value::Array(items) => items.into_iter().map(Self).collect(),
            other => panic!(
                "{what} holds {} where it holds an array",
                Self(other).kind()
            ),
        }
    }

    pub fn string_value(self, what: &str) -> String {
        match self.0 {
            Value::String(value) => value,
            other => panic!(
                "{what} holds {} where it holds a string",
                Self(other).kind()
            ),
        }
    }

    pub fn boolean_value(self, what: &str) -> bool {
        match self.0 {
            Value::Boolean(value) => value,
            other => panic!(
                "{what} holds {} where it holds a boolean",
                Self(other).kind()
            ),
        }
    }

    pub fn f32_value(self, what: &str) -> f32 {
        match self.0 {
            Value::Float(value) => number::narrow(value, what),
            Value::Integer(value) => number::narrow(value as f64, what),
            other => panic!(
                "{what} holds {} where it holds a number",
                Self(other).kind()
            ),
        }
    }

    pub fn u64_value(self, what: &str) -> u64 {
        match self.0 {
            Value::Integer(value) => u64::try_from(value)
                .unwrap_or_else(|_| panic!("{what} holds {value}, which is not a count")),
            other => panic!(
                "{what} holds {} where it holds a whole number",
                Self(other).kind(),
            ),
        }
    }

    pub fn usize_value(self, what: &str) -> usize {
        let value = self.u64_value(what);
        usize::try_from(value)
            .unwrap_or_else(|_| panic!("{what} holds {value}, which outruns this machine"))
    }

    pub fn write(&self) -> String {
        assert!(
            matches!(self.0, Value::Table(_)),
            "a TOML document holds {} at its root where it holds a table",
            self.kind(),
        );
        write::document(&self.0)
    }
}

impl From<&str> for Toml {
    fn from(value: &str) -> Self {
        Self::string(value)
    }
}

impl From<String> for Toml {
    fn from(value: String) -> Self {
        Self::string(value)
    }
}

impl From<bool> for Toml {
    fn from(value: bool) -> Self {
        Self::boolean(value)
    }
}

impl From<f32> for Toml {
    fn from(value: f32) -> Self {
        Self::float(value)
    }
}

impl From<u32> for Toml {
    fn from(value: u32) -> Self {
        Self::whole(u64::from(value))
    }
}

impl From<u64> for Toml {
    fn from(value: u64) -> Self {
        Self::whole(value)
    }
}

impl From<usize> for Toml {
    fn from(value: usize) -> Self {
        Self::whole(
            u64::try_from(value)
                .unwrap_or_else(|_| panic!("a TOML integer holds {value}, which outruns 64 bits")),
        )
    }
}

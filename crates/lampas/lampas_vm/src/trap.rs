use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trap {
    location: Option<Location>,
    message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Location {
    function: String,
    instruction: usize,
}

impl Trap {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            location: None,
            message: message.into(),
        }
    }

    pub(crate) fn located(function: &str, instruction: usize, message: impl Into<String>) -> Self {
        Self {
            location: Some(Location {
                function: String::from(function),
                instruction,
            }),
            message: message.into(),
        }
    }

    pub(crate) fn locate(mut self, function: &str, instruction: usize) -> Self {
        self.location = Some(Location {
            function: String::from(function),
            instruction,
        });
        self
    }

    pub fn function(&self) -> Option<&str> {
        self.location.as_ref().map(|held| held.function.as_str())
    }

    pub fn instruction(&self) -> Option<usize> {
        self.location.as_ref().map(|held| held.instruction)
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Trap {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.location {
            Some(held) => write!(
                out,
                "the lampas function {} fails at instruction {}: {}",
                held.function, held.instruction, self.message,
            ),
            None => out.write_str(&self.message),
        }
    }
}

impl std::error::Error for Trap {}

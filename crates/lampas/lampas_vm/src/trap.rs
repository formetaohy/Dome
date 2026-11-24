use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trap {
    function: String,
    instruction: usize,
    message: String,
}

impl Trap {
    pub(crate) fn new(function: &str, instruction: usize, message: impl Into<String>) -> Self {
        Self {
            function: String::from(function),
            instruction,
            message: message.into(),
        }
    }

    pub fn function(&self) -> &str {
        &self.function
    }

    pub fn instruction(&self) -> usize {
        self.instruction
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Trap {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "the lampas function {} fails at instruction {}: {}",
            self.function, self.instruction, self.message,
        )
    }
}

impl std::error::Error for Trap {}

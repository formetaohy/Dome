use std::fmt::{self, Display, Formatter};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct GpuFailure {
    message: String,
}

impl GpuFailure {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for GpuFailure {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        write!(out, "the graphics device fails: {}", self.message)
    }
}

impl std::error::Error for GpuFailure {}

#[derive(Clone, Default)]
pub struct Failure {
    message: Arc<Mutex<Option<String>>>,
}

impl Failure {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn raise(&self, message: impl Into<String>) -> GpuFailure {
        let message = message.into();
        let mut held = self
            .message
            .lock()
            .expect("a graphics failure slot is never poisoned");
        GpuFailure::new(held.get_or_insert(message).clone())
    }

    pub fn take(&self) -> Option<GpuFailure> {
        self.message
            .lock()
            .expect("a graphics failure slot is never poisoned")
            .take()
            .map(GpuFailure::new)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Error,
    Notice,
}

pub(crate) struct Message {
    text: String,
    kind: Kind,
}

impl Message {
    pub(crate) fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: Kind::Error,
        }
    }

    pub(crate) fn notice(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: Kind::Notice,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn kind(&self) -> Kind {
        self.kind
    }
}

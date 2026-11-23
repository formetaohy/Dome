use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct SoundEntry {
    pub id: String,
    pub path: PathBuf,
    pub stream: bool,
}

impl SoundEntry {
    pub fn new(id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            stream: false,
        }
    }

    pub fn streamed(id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            stream: true,
        }
    }
}

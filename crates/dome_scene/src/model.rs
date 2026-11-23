use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct ModelEntry {
    pub id: String,
    pub path: PathBuf,
}

impl ModelEntry {
    pub fn new(id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
        }
    }
}

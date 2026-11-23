use dome_render::environment::Atmosphere;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum EnvironmentSource {
    Panorama(PathBuf),
    Atmosphere(Atmosphere),
    Capture,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentEntry {
    pub id: String,
    pub source: EnvironmentSource,
}

impl EnvironmentEntry {
    pub fn new(id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id: id.into(),
            source: EnvironmentSource::Panorama(path.into()),
        }
    }

    pub fn with_atmosphere(id: impl Into<String>, atmosphere: Atmosphere) -> Self {
        Self {
            id: id.into(),
            source: EnvironmentSource::Atmosphere(atmosphere),
        }
    }

    pub fn captured(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            source: EnvironmentSource::Capture,
        }
    }

    pub fn panorama(&self) -> Option<&Path> {
        match &self.source {
            EnvironmentSource::Panorama(path) => Some(path),
            EnvironmentSource::Atmosphere(_) | EnvironmentSource::Capture => None,
        }
    }

    pub fn atmosphere(&self) -> Option<&Atmosphere> {
        match &self.source {
            EnvironmentSource::Panorama(_) | EnvironmentSource::Capture => None,
            EnvironmentSource::Atmosphere(atmosphere) => Some(atmosphere),
        }
    }

    pub fn capture(&self) -> bool {
        matches!(self.source, EnvironmentSource::Capture)
    }
}

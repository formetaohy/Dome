use crate::reference::ModelId;
use dome_render::{AddressMode, FilterMode, SamplerDesc};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum ImageSource {
    Path(PathBuf),
    Model { model: ModelId, image: u32 },
}

impl ImageSource {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Path(path) => Some(path),
            Self::Model { .. } => None,
        }
    }

    pub fn model(&self) -> Option<(&ModelId, u32)> {
        match self {
            Self::Path(_) => None,
            Self::Model { model, image } => Some((model, *image)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageEntry {
    pub id: String,
    pub source: ImageSource,
    pub sampler: SamplerDesc,
}

impl ImageEntry {
    pub fn new(id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id: id.into(),
            source: ImageSource::Path(path.into()),
            sampler: material_sampler(),
        }
    }

    pub fn embedded(id: impl Into<String>, model: ModelId, image: u32) -> Self {
        Self {
            id: id.into(),
            source: ImageSource::Model { model, image },
            sampler: material_sampler(),
        }
    }

    pub fn with_sampler(mut self, sampler: SamplerDesc) -> Self {
        self.sampler = sampler;
        self
    }
}

pub fn material_sampler() -> SamplerDesc {
    SamplerDesc {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        address_mode_u: AddressMode::Repeat,
        address_mode_v: AddressMode::Repeat,
        compare: None,
    }
}

pub fn filter_name(filter: FilterMode) -> &'static str {
    match filter {
        FilterMode::Linear => "linear",
        FilterMode::Nearest => "nearest",
    }
}

pub fn filter_of(name: &str) -> Option<FilterMode> {
    match name {
        "linear" => Some(FilterMode::Linear),
        "nearest" => Some(FilterMode::Nearest),
        _ => None,
    }
}

pub fn address_name(address: AddressMode) -> &'static str {
    match address {
        AddressMode::Repeat => "repeat",
        AddressMode::MirrorRepeat => "mirror",
        AddressMode::ClampToEdge => "clamp",
        AddressMode::ClampToBorder => panic!("a scene sampler holds no border clamp"),
    }
}

pub fn address_of(name: &str) -> Option<AddressMode> {
    match name {
        "repeat" => Some(AddressMode::Repeat),
        "mirror" => Some(AddressMode::MirrorRepeat),
        "clamp" => Some(AddressMode::ClampToEdge),
        _ => None,
    }
}

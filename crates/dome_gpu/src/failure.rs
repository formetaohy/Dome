use std::fmt::{self, Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuFailure {
    SurfaceOutdated,
    SurfaceLost,
    SurfaceFormat,
    Fault(String),
}

impl Display for GpuFailure {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::SurfaceOutdated => write!(
                out,
                "the present surface stays outdated after reconfiguring it"
            ),
            Self::SurfaceLost => write!(out, "the present surface stays lost after recreating it"),
            Self::SurfaceFormat => write!(out, "the present surface changes its format"),
            Self::Fault(message) => write!(out, "the graphics device fails: {message}"),
        }
    }
}

impl std::error::Error for GpuFailure {}

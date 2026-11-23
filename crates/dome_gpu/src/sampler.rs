use crate::{Device, NativeSampler, ResourceId};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Filter {
    Nearest,
    Linear,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CompareOp {
    Never,
    Less,
    Equal,
    LessOrEqual,
    Greater,
    NotEqual,
    GreaterOrEqual,
    Always,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SamplerDesc {
    pub min_filter: Filter,
    pub mag_filter: Filter,
    pub mip_filter: Filter,
    pub address_u: AddressMode,
    pub address_v: AddressMode,
    pub compare: Option<CompareOp>,
}

impl SamplerDesc {
    pub const fn trilinear(&self) -> bool {
        matches!(self.min_filter, Filter::Linear)
            && matches!(self.mag_filter, Filter::Linear)
            && matches!(self.mip_filter, Filter::Linear)
    }
}

struct SamplerState {
    native: NativeSampler,
    device: Device,
    desc: SamplerDesc,
    identity: ResourceId,
}

#[derive(Clone)]
pub struct Sampler {
    state: Arc<SamplerState>,
}

impl Sampler {
    pub fn new(device: &Device, desc: SamplerDesc) -> Self {
        let anisotropy = if desc.trilinear() {
            device.limits().max_sampler_anisotropy
        } else {
            1
        };
        Self {
            state: Arc::new(SamplerState {
                native: device.native().create_sampler(&desc, anisotropy),
                device: device.clone(),
                desc,
                identity: ResourceId::next(),
            }),
        }
    }

    pub fn desc(&self) -> SamplerDesc {
        self.state.desc
    }

    pub fn identity(&self) -> ResourceId {
        self.state.identity
    }

    pub(crate) fn device(&self) -> &Device {
        &self.state.device
    }

    pub(crate) fn native(&self) -> &NativeSampler {
        &self.state.native
    }
}

use crate::context::Device;
use crate::identity::ResourceId;
pub use wgpu::{AddressMode, CompareFunction, FilterMode, MipmapFilterMode};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SamplerDesc {
    pub mag_filter: FilterMode,
    pub min_filter: FilterMode,
    pub mipmap_filter: FilterMode,
    pub address_mode_u: AddressMode,
    pub address_mode_v: AddressMode,
    pub compare: Option<CompareFunction>,
}

impl SamplerDesc {
    pub const fn trilinear(&self) -> bool {
        matches!(self.min_filter, FilterMode::Linear)
            && matches!(self.mag_filter, FilterMode::Linear)
            && matches!(self.mipmap_filter, FilterMode::Linear)
    }
}

#[derive(Clone)]
pub struct Sampler {
    device: Device,
    native: wgpu::Sampler,
    desc: SamplerDesc,
    identity: ResourceId,
}

impl Sampler {
    pub fn new(device: &Device, desc: SamplerDesc) -> Self {
        let anisotropy = if desc.trilinear() { 16 } else { 1 };
        let native = device.native().create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: desc.address_mode_u,
            address_mode_v: desc.address_mode_v,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: desc.mag_filter,
            min_filter: desc.min_filter,
            mipmap_filter: match desc.mipmap_filter {
                FilterMode::Nearest => MipmapFilterMode::Nearest,
                FilterMode::Linear => MipmapFilterMode::Linear,
            },
            compare: desc.compare,
            anisotropy_clamp: anisotropy,
            ..Default::default()
        });
        Self {
            device: device.clone(),
            native,
            desc,
            identity: ResourceId::next(),
        }
    }

    pub fn desc(&self) -> SamplerDesc {
        self.desc
    }

    pub fn identity(&self) -> ResourceId {
        self.identity
    }

    pub(crate) fn device(&self) -> &Device {
        &self.device
    }

    pub(crate) fn native(&self) -> &wgpu::Sampler {
        &self.native
    }
}

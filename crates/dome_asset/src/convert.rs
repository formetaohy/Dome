use dome_gltf::{AlphaMode, Filter, Sampler, SkinVertex, Vertex, Wrap};
use dome_render::{AddressMode, SamplerDesc, SkinVertexRecord};

pub(crate) fn vertex(vertex: Vertex) -> dome_render::Vertex {
    dome_render::Vertex {
        position: vertex.position,
        normal: vertex.normal,
        uv: vertex.uv,
    }
}

pub(crate) fn skin_vertex(vertex: SkinVertex) -> SkinVertexRecord {
    SkinVertexRecord::new(vertex.joints, vertex.weights)
}

pub(crate) fn sampler(sampler: Sampler) -> SamplerDesc {
    SamplerDesc {
        min_filter: filter(sampler.min_filter),
        mag_filter: filter(sampler.mag_filter),
        mip_filter: filter(sampler.mip_filter),
        address_u: wrap(sampler.wrap_u),
        address_v: wrap(sampler.wrap_v),
    }
}

pub(crate) fn alpha_mode(mode: AlphaMode) -> dome_render::AlphaMode {
    match mode {
        AlphaMode::Opaque => dome_render::AlphaMode::Opaque,
        AlphaMode::Mask => dome_render::AlphaMode::Mask,
        AlphaMode::Blend => dome_render::AlphaMode::Blend,
    }
}

fn filter(filter: Filter) -> dome_render::Filter {
    match filter {
        Filter::Nearest => dome_render::Filter::Nearest,
        Filter::Linear => dome_render::Filter::Linear,
    }
}

fn wrap(wrap: Wrap) -> AddressMode {
    match wrap {
        Wrap::Repeat => AddressMode::Repeat,
        Wrap::MirroredRepeat => AddressMode::MirrorRepeat,
        Wrap::ClampToEdge => AddressMode::ClampToEdge,
    }
}

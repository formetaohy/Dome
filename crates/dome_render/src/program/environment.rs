use super::*;

const PREFILTER: GraphicsShader = GraphicsShader {
    label: "prefilter",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/prefilter.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, PREFILTER_LEVEL),
        BindingSpec::sampled_texture(1, PREFILTER_PANORAMA),
        BindingSpec::filtering_sampler(1, PREFILTER_SAMPLER),
    ],
    targets: 1,
};

const INTEGRATION: GraphicsShader = GraphicsShader {
    label: "integration",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/irradiance_integrate.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, INTEGRATION_SETUP),
        BindingSpec::writable_storage(0, INTEGRATION_PARTIALS),
        BindingSpec::sampled_texture(1, INTEGRATION_PANORAMA),
        BindingSpec::filtering_sampler(1, INTEGRATION_SAMPLER),
    ],
    targets: 1,
};

const IRRADIANCE_REDUCTION: ComputeShader = ComputeShader {
    label: "irradiance_reduction",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/irradiance_reduce.wgsl"),
    ),
    entry: "compute",
    bindings: &[
        BindingSpec::storage(0, REDUCTION_PARTIALS),
        BindingSpec::writable_storage(0, REDUCTION_TOTAL),
    ],
    workgroup: 64,
};

const TRANSMITTANCE: GraphicsShader = GraphicsShader {
    label: "atmosphere_transmittance",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/atmosphere_transmittance.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[BindingSpec::storage(0, ATMOSPHERE_SETUP)],
    targets: 1,
};

const SCATTERING: GraphicsShader = GraphicsShader {
    label: "atmosphere_scattering",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/atmosphere_scattering.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, ATMOSPHERE_SETUP),
        BindingSpec::sampled_texture(1, ATMOSPHERE_TRANSMITTANCE),
        BindingSpec::filtering_sampler(1, ATMOSPHERE_LUT_SAMPLER),
    ],
    targets: 1,
};

const ATMOSPHERE_SKY: GraphicsShader = GraphicsShader {
    label: "atmosphere_sky",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/atmosphere_sky.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, ATMOSPHERE_SETUP),
        BindingSpec::sampled_texture(1, ATMOSPHERE_TRANSMITTANCE),
        BindingSpec::sampled_texture(1, ATMOSPHERE_SCATTERING),
        BindingSpec::filtering_sampler(1, ATMOSPHERE_SKY_SAMPLER),
    ],
    targets: 1,
};

pub(crate) fn prefiltering() -> &'static GraphicsShader {
    &PREFILTER
}

pub(crate) fn integrating() -> &'static GraphicsShader {
    &INTEGRATION
}

pub(crate) fn irradiance_reduction_program() -> &'static ComputeShader {
    &IRRADIANCE_REDUCTION
}

pub(crate) fn atmosphere_transmittance_program() -> &'static GraphicsShader {
    &TRANSMITTANCE
}

pub(crate) fn atmosphere_scattering_program() -> &'static GraphicsShader {
    &SCATTERING
}

pub(crate) fn atmosphere_sky_program() -> &'static GraphicsShader {
    &ATMOSPHERE_SKY
}

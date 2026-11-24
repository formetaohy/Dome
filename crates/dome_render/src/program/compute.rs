use super::*;

const CULL: ComputeShader = ComputeShader {
    label: "cull",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/cull.wgsl"),
    ),
    entry: "compute",
    bindings: &[
        BindingSpec::storage(0, CULL_VIEW),
        BindingSpec::storage(0, CULL_INSTANCES),
        BindingSpec::writable_storage(0, CULL_DRAWN),
        BindingSpec::writable_storage(0, CULL_ARGUMENTS),
        BindingSpec::unfilterable_texture(0, CULL_HZB),
        BindingSpec::non_filtering_sampler(0, CULL_FILTER),
    ],
    workgroup: 64,
};

const CLUSTER: ComputeShader = ComputeShader {
    label: "cluster",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/cluster.wgsl"),
    ),
    entry: "compute",
    bindings: &[
        BindingSpec::storage(0, CLUSTER_VIEW),
        BindingSpec::storage(0, CLUSTER_POINTS),
        BindingSpec::writable_storage(0, CLUSTER_GRID),
    ],
    workgroup: 64,
};

pub(crate) fn culling() -> &'static ComputeShader {
    &CULL
}
pub(crate) fn clustering() -> &'static ComputeShader {
    &CLUSTER
}

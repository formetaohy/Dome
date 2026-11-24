use super::*;

const HZB_BASE: GraphicsShader = GraphicsShader {
    label: "hzb_base",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/hzb_base.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, HZB_ORIENTATION_BINDING),
        BindingSpec::depth_texture(1, HZB_SOURCE_BINDING),
        BindingSpec::non_filtering_sampler(1, HZB_SAMPLER_BINDING),
    ],
    targets: 1,
};

const HZB_REDUCE: GraphicsShader = GraphicsShader {
    label: "hzb_reduce",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/hzb_reduce.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, HZB_ORIENTATION_BINDING),
        BindingSpec::unfilterable_texture(1, HZB_SOURCE_BINDING),
        BindingSpec::non_filtering_sampler(1, HZB_SAMPLER_BINDING),
    ],
    targets: 1,
};

pub(crate) fn hzb_base() -> &'static GraphicsShader {
    &HZB_BASE
}
pub(crate) fn hzb_reduce() -> &'static GraphicsShader {
    &HZB_REDUCE
}

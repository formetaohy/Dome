use super::*;

const CAPTURE: GraphicsShader = GraphicsShader {
    label: "capture",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/capture.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, CAPTURE_SETUP),
        BindingSpec::sampled_texture(1, CAPTURE_ATLAS),
        BindingSpec::filtering_sampler(1, CAPTURE_SAMPLER),
    ],
    targets: 1,
};

pub(crate) fn capture_conversion() -> &'static GraphicsShader {
    &CAPTURE
}

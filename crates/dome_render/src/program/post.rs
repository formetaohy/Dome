use super::*;

const TRANSPARENCY_RESOLVE: GraphicsShader = GraphicsShader {
    label: "transparency_resolve",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/transparency_resolve.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, RESOLVE_SETTINGS),
        BindingSpec::storage(0, RESOLVE_KEYS),
        BindingSpec::storage(0, RESOLVE_COLORS),
        BindingSpec::sampled_texture(1, RESOLVE_SOURCE),
        BindingSpec::filtering_sampler(1, RESOLVE_FILTER),
    ],
    targets: 1,
};

const BLOOM_EXTRACT: GraphicsShader = GraphicsShader {
    label: "bloom_extract",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/bloom_extract.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, BLOOM_SETTINGS),
        BindingSpec::sampled_texture(1, EXTRACT_SOURCE),
        BindingSpec::filtering_sampler(1, EXTRACT_FILTER),
    ],
    targets: 1,
};

const BLOOM_DOWN: GraphicsShader = GraphicsShader {
    label: "bloom_down",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/bloom_down.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, BLOOM_SETTINGS),
        BindingSpec::sampled_texture(1, DOWN_SOURCE),
        BindingSpec::filtering_sampler(1, DOWN_FILTER),
    ],
    targets: 1,
};

const BLOOM_UP: GraphicsShader = GraphicsShader {
    label: "bloom_up",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/bloom_up.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, BLOOM_SETTINGS),
        BindingSpec::sampled_texture(1, UP_LOW),
        BindingSpec::sampled_texture(1, UP_HIGH),
        BindingSpec::filtering_sampler(1, UP_FILTER),
    ],
    targets: 1,
};

const TONEMAP: GraphicsShader = GraphicsShader {
    label: "tonemap",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/tonemap.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, TONEMAP_SETTINGS),
        BindingSpec::sampled_texture(1, TONEMAP_LIT),
        BindingSpec::sampled_texture(1, TONEMAP_BLOOM),
        BindingSpec::filtering_sampler(1, TONEMAP_FILTER),
    ],
    targets: 1,
};

const SKY: GraphicsShader = GraphicsShader {
    label: "sky",
    source: concat!(include_str!("wgsl/abi.wgsl"), include_str!("wgsl/sky.wgsl"),),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, SKY_VIEW),
        BindingSpec::sampled_texture(1, SKY_RADIANCE),
        BindingSpec::filtering_sampler(1, SKY_SAMPLER),
    ],
    targets: 1,
};

const TEMPORAL_RESOLVE: GraphicsShader = GraphicsShader {
    label: "temporal_resolve",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/temporal_resolve.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, TEMPORAL_FRAME),
        BindingSpec::sampled_texture(1, TEMPORAL_SOURCE),
        BindingSpec::sampled_texture(1, TEMPORAL_HISTORY),
        BindingSpec::filtering_sampler(1, TEMPORAL_FILTER),
        BindingSpec::sampled_texture(2, TEMPORAL_MOTION),
        BindingSpec::sampled_texture(2, TEMPORAL_PREVIOUS),
        BindingSpec::filtering_sampler(2, TEMPORAL_NEAREST),
    ],
    targets: 1,
};

pub(crate) fn transparency_resolve() -> &'static GraphicsShader {
    &TRANSPARENCY_RESOLVE
}
pub(crate) fn bloom_extract() -> &'static GraphicsShader {
    &BLOOM_EXTRACT
}
pub(crate) fn bloom_down() -> &'static GraphicsShader {
    &BLOOM_DOWN
}
pub(crate) fn bloom_up() -> &'static GraphicsShader {
    &BLOOM_UP
}
pub(crate) fn tonemap() -> &'static GraphicsShader {
    &TONEMAP
}
pub(crate) fn sky_program() -> &'static GraphicsShader {
    &SKY
}
pub(crate) fn temporal_resolve() -> &'static GraphicsShader {
    &TEMPORAL_RESOLVE
}

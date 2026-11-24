use super::*;

const INSTANCING: GraphicsShader = GraphicsShader {
    label: "instancing",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/instancing.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, DRAW_LIGHT),
        BindingSpec::storage(0, DRAW_CASCADES),
        BindingSpec::storage(0, DRAW_DRAWN),
        BindingSpec::storage(0, DRAW_MATERIALS),
        BindingSpec::storage(0, DRAW_VIEW),
        BindingSpec::storage(0, DRAW_PALETTE),
        BindingSpec::storage(0, DRAW_SKINS),
        BindingSpec::writable_storage(0, DRAW_FRAGMENTS),
        BindingSpec::writable_storage(0, DRAW_COLORS),
        BindingSpec::writable_storage(0, DRAW_DROPPED),
        BindingSpec::storage(0, DRAW_SETTINGS),
        BindingSpec::storage(0, DRAW_PROBES),
        BindingSpec::storage(0, DRAW_VERTICES),
        BindingSpec::storage(0, DRAW_LIGHTS),
        BindingSpec::storage(0, DRAW_POINT_LIGHTS),
        BindingSpec::texture_table(1, SHADE_TEXTURE),
        BindingSpec::sampler_table(1, SHADE_SAMPLER),
        BindingSpec::depth_texture(2, SHADOW_SHADOW),
        BindingSpec::comparison_sampler(2, SHADOW_SAMPLER),
        BindingSpec::sampled_texture(2, SHADE_RADIANCE),
        BindingSpec::filtering_sampler(2, SHADE_RADIANCE_SAMPLER),
        BindingSpec::sampled_texture(2, SHADE_BRDF),
        BindingSpec::filtering_sampler(2, SHADE_BRDF_SAMPLER),
        BindingSpec::depth_texture(2, POINT_SHADOW_TEXTURE),
        BindingSpec::comparison_sampler(2, POINT_SHADOW_SAMPLER),
        BindingSpec::texture_table(2, PROBE_TEXTURES),
        BindingSpec::sampler_table(2, PROBE_SAMPLERS),
    ],
    targets: 1,
};

const SHADOWING: GraphicsShader = GraphicsShader {
    label: "shadowing",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/shadowing.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, SHADOW_PALETTE),
        BindingSpec::storage(0, SHADOW_SKINS),
        BindingSpec::storage(0, SHADOW_DRAWN),
        BindingSpec::storage(0, SHADOW_VERTICES),
        BindingSpec::storage(0, SHADOW_MATERIALS),
        BindingSpec::texture_table(1, SHADE_TEXTURE),
        BindingSpec::sampler_table(1, SHADE_SAMPLER),
    ],
    targets: 0,
};

const DEPTH_PREPASS: GraphicsShader = GraphicsShader {
    label: "depth_prepass",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/depth_prepass.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, DEPTH_PALETTE),
        BindingSpec::storage(0, DEPTH_SKINS),
        BindingSpec::storage(0, DEPTH_DRAWN),
        BindingSpec::storage(0, DEPTH_VERTICES),
        BindingSpec::storage(0, DEPTH_MATERIALS),
        BindingSpec::texture_table(1, SHADE_TEXTURE),
        BindingSpec::sampler_table(1, SHADE_SAMPLER),
    ],
    targets: 0,
};

const PICKING: GraphicsShader = GraphicsShader {
    label: "picking",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/picking.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, PICK_VIEW),
        BindingSpec::storage(0, PICK_PALETTE),
        BindingSpec::storage(0, PICK_SKINS),
        BindingSpec::storage(0, PICK_DRAWN),
        BindingSpec::storage(0, PICK_VERTICES),
    ],
    targets: 1,
};

const DEBUG_LINES: GraphicsShader = GraphicsShader {
    label: "debug_lines",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/debug_lines.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, LINE_VIEW),
        BindingSpec::storage(0, LINE_SEGMENTS),
    ],
    targets: 1,
};

const MOTION_VECTORS: GraphicsShader = GraphicsShader {
    label: "motion_vectors",
    source: concat!(
        include_str!("wgsl/abi.wgsl"),
        include_str!("wgsl/motion_vectors.wgsl"),
    ),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, MOTION_VIEW),
        BindingSpec::storage(0, MOTION_DRAWN),
        BindingSpec::storage(0, MOTION_PALETTE),
        BindingSpec::storage(0, MOTION_SKINS),
        BindingSpec::storage(0, MOTION_MATERIALS),
        BindingSpec::storage(0, MOTION_VERTICES),
        BindingSpec::texture_table(1, MOTION_TEXTURE),
        BindingSpec::sampler_table(1, MOTION_SAMPLER),
    ],
    targets: 1,
};

pub(crate) fn instancing(tables: TableCapacity) -> &'static GraphicsShader {
    INSTANCING.specialized(tables)
}
pub(crate) fn shadowing(tables: TableCapacity) -> &'static GraphicsShader {
    SHADOWING.specialized(tables)
}
pub(crate) fn depth_prepass(tables: TableCapacity) -> &'static GraphicsShader {
    DEPTH_PREPASS.specialized(tables)
}
pub(crate) fn picking() -> &'static GraphicsShader {
    &PICKING
}
pub(crate) fn debug_lines() -> &'static GraphicsShader {
    &DEBUG_LINES
}
pub(crate) fn motion_vectors(tables: TableCapacity) -> &'static GraphicsShader {
    MOTION_VECTORS.specialized(tables)
}

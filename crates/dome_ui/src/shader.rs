use dome_gpu::{BindingSpec, GraphicsShader, TableCapacity};

const PAINT: GraphicsShader = GraphicsShader {
    label: "paint",
    source: concat!(include_str!("wgsl/paint.wgsl")),
    vertex: "vertex",
    fragment: "fragment",
    bindings: &[
        BindingSpec::storage(0, 0),
        BindingSpec::storage(0, 1),
        BindingSpec::texture_table(1, 0),
        BindingSpec::sampler_table(1, 1),
    ],
    targets: 1,
};

pub(crate) fn program(tables: TableCapacity) -> &'static GraphicsShader {
    PAINT.specialized(tables)
}

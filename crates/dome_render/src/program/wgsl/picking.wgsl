struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) picked: vec4<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> VertexOut {
    let index = visible[instance];
    let held = instances[index];
    let camera = view[0u];
    let base = vertex * 8u;
    let rest = vec4<f32>(
        vertices[base],
        vertices[base + 1u],
        vertices[base + 2u],
        1.0,
    );
    let id = index;
    var out: VertexOut;
    out.picked = vec4<f32>(
        f32(id % 256u) / 255.0,
        f32((id / 256u) % 256u) / 255.0,
        f32((id / 65536u) % 256u) / 255.0,
        1.0,
    );
    let local = skinned_position(held, vertex, camera.palette_base, rest);
    let moved = held.model0 * local.x
        + held.model1 * local.y
        + held.model2 * local.z
        + held.model3 * local.w;
    out.position = camera.clip0 * moved.x
        + camera.clip1 * moved.y
        + camera.clip2 * moved.z
        + camera.clip3;
    return out;
}

struct FragmentIn {
    @location(0) space: vec4<f32>,
};

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    return input.space;
}

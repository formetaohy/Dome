struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) surface: vec4<f32>,
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
    let local = skinned_position(held, vertex, camera.palette_base, rest);
    let moved = held.model0 * local.x
        + held.model1 * local.y
        + held.model2 * local.z
        + held.model3 * local.w;
    var out: VertexOut;
    out.position = camera.clip0 * moved.x
        + camera.clip1 * moved.y
        + camera.clip2 * moved.z
        + camera.clip3;
    out.surface = vec4<f32>(
        vertices[base + 6u],
        vertices[base + 7u],
        f32(held.material),
        0.0,
    );
    return out;
}

struct FragmentIn {
    @location(0) shaped: vec4<f32>,
};

@fragment
fn fragment(input: FragmentIn) {
    let shape = input.shaped;
    let material = u32(shape.z);
    let entry = materials[material];
    if entry.alpha_mode == 1u {
        let sampled = textureSample(
            textures[entry.base_color_image],
            samplers[entry.base_color_sampler],
            vec2<f32>(shape.x, shape.y),
        );
        if sampled.w * entry.base_color.w < entry.alpha_cutoff {
            discard;
        }
    }
}

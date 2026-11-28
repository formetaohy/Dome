struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) surface: vec4<f32>,
    @location(1) travel: vec4<f32>,
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
    let local = skinned_position(held, vertex, camera.base, rest);
    let carried = skinned_position(held, vertex, camera.previous, rest);
    let current_world = held.model0 * local.x
        + held.model1 * local.y
        + held.model2 * local.z
        + held.model3 * local.w;
    let current_clip = camera.clip0 * current_world.x
        + camera.clip1 * current_world.y
        + camera.clip2 * current_world.z
        + camera.clip3;
    let previous_world = held.previous0 * carried.x
        + held.previous1 * carried.y
        + held.previous2 * carried.z
        + held.previous3 * carried.w;
    let previous_clip = camera.previous0 * previous_world.x
        + camera.previous1 * previous_world.y
        + camera.previous2 * previous_world.z
        + camera.previous3 * previous_world.w;
    let current_inverse = 1.0 / max(current_clip.w, 0.0001);
    let previous_inverse = 1.0 / max(previous_clip.w, 0.0001);
    let reach = (current_world.x - camera.eye.x) * camera.forward.x
        + (current_world.y - camera.eye.y) * camera.forward.y
        + (current_world.z - camera.eye.z) * camera.forward.z;
    var out: VertexOut;
    out.position = current_clip;
    out.surface = vec4<f32>(
        vertices[base + 6u],
        vertices[base + 7u],
        f32(held.material),
        reach,
    );
    out.travel = vec4<f32>(
        current_clip.x * current_inverse * 0.5 + 0.5,
        current_clip.y * current_inverse * camera.vertical + 0.5,
        previous_clip.x * previous_inverse * 0.5 + 0.5,
        previous_clip.y * previous_inverse * camera.vertical + 0.5,
    );
    return out;
}

struct FragmentIn {
    @location(0) shaped: vec4<f32>,
    @location(1) moved: vec4<f32>,
};

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    let entry = materials[u32(input.shaped.z)];
    if entry.alpha_mode == 1u {
        let sampled = textureSample(
            textures[entry.base_color_image],
            samplers[entry.base_color_sampler],
            vec2<f32>(input.shaped.x, input.shaped.y),
        );
        if sampled.w * entry.base_color.w < entry.alpha_cutoff {
            discard;
        }
    }
    return vec4<f32>(
        input.moved.z - input.moved.x,
        input.moved.w - input.moved.y,
        input.shaped.w,
        1.0,
    );
}

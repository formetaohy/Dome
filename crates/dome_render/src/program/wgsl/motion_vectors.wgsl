@group(0) @binding(0) var<storage, read> view: array<Motion>;
@group(0) @binding(1) var<storage, read> visible: array<u32>;
@group(0) @binding(2) var<storage, read> palette: array<Joint>;
@group(0) @binding(3) var<storage, read> skins: array<SkinVertex>;
@group(0) @binding(4) var<storage, read> materials: array<Material>;
@group(0) @binding(5) var<storage, read> vertices: array<f32>;
@group(0) @binding(6) var<storage, read> instances: array<Instance>;
@group(1) @binding(0) var textures: binding_array<texture_2d<f32>, TEXTURE_TABLE_EXTENT>;
@group(1) @binding(1) var samplers: binding_array<sampler, SAMPLER_TABLE_EXTENT>;

fn deformed(col0: vec4<f32>, col1: vec4<f32>, col2: vec4<f32>, col3: vec4<f32>, point: vec4<f32>) -> vec4<f32> {
    return col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w;
}

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
    var local = rest;
    var carried = rest;
    if held.joints > 0u {
        let skin = skins[vertex];
        let first = palette[held.palette + camera.base + skin.joints.x];
        let second = palette[held.palette + camera.base + skin.joints.y];
        let third = palette[held.palette + camera.base + skin.joints.z];
        let fourth = palette[held.palette + camera.base + skin.joints.w];
        let place = deformed(first.col0, first.col1, first.col2, first.col3, rest)
            * skin.weights.x
            + deformed(second.col0, second.col1, second.col2, second.col3, rest)
                * skin.weights.y
            + deformed(third.col0, third.col1, third.col2, third.col3, rest) * skin.weights.z
            + deformed(fourth.col0, fourth.col1, fourth.col2, fourth.col3, rest)
                * skin.weights.w;
        local = vec4<f32>(place.x, place.y, place.z, 1.0);
        let before = held.palette + camera.previous;
        let was_first = palette[before + skin.joints.x];
        let was_second = palette[before + skin.joints.y];
        let was_third = palette[before + skin.joints.z];
        let was_fourth = palette[before + skin.joints.w];
        let was = deformed(
            was_first.col0,
            was_first.col1,
            was_first.col2,
            was_first.col3,
            rest,
        ) * skin.weights.x
            + deformed(
                was_second.col0,
                was_second.col1,
                was_second.col2,
                was_second.col3,
                rest,
            ) * skin.weights.y
            + deformed(
                was_third.col0,
                was_third.col1,
                was_third.col2,
                was_third.col3,
                rest,
            ) * skin.weights.z
            + deformed(
                was_fourth.col0,
                was_fourth.col1,
                was_fourth.col2,
                was_fourth.col3,
                rest,
            ) * skin.weights.w;
        carried = vec4<f32>(was.x, was.y, was.z, 1.0);
    }
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

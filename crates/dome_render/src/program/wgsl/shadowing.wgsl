@group(0) @binding(0) var<storage, read> palette: array<Joint>;
@group(0) @binding(1) var<storage, read> skins: array<SkinVertex>;
@group(0) @binding(2) var<storage, read> visible: array<u32>;
@group(0) @binding(3) var<storage, read> vertices: array<f32>;
@group(0) @binding(4) var<storage, read> materials: array<Material>;
@group(0) @binding(5) var<storage, read> view: array<View>;
@group(0) @binding(6) var<storage, read> instances: array<Instance>;
@group(1) @binding(0) var textures: binding_array<texture_2d<f32>, TEXTURE_TABLE_EXTENT>;
@group(1) @binding(1) var samplers: binding_array<sampler, SAMPLER_TABLE_EXTENT>;

fn deformed(col0: vec4<f32>, col1: vec4<f32>, col2: vec4<f32>, col3: vec4<f32>, point: vec4<f32>) -> vec4<f32> {
    return col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w;
}

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
    var local = rest;
    if held.joints > 0u {
        let skin = skins[vertex];
        let palette_base = held.palette + camera.palette_base;
        let first = palette[palette_base + skin.joints.x];
        let second = palette[palette_base + skin.joints.y];
        let third = palette[palette_base + skin.joints.z];
        let fourth = palette[palette_base + skin.joints.w];
        let place = deformed(first.col0, first.col1, first.col2, first.col3, rest)
            * skin.weights.x
            + deformed(second.col0, second.col1, second.col2, second.col3, rest)
                * skin.weights.y
            + deformed(third.col0, third.col1, third.col2, third.col3, rest) * skin.weights.z
            + deformed(fourth.col0, fourth.col1, fourth.col2, fourth.col3, rest)
                * skin.weights.w;
        local = vec4<f32>(place.x, place.y, place.z, 1.0);
    }
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

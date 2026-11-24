@group(0) @binding(0) var<storage, read> settings: array<Tonemap>;
@group(1) @binding(0) var lit: texture_2d<f32>;
@group(1) @binding(1) var bloom: texture_2d<f32>;
@group(1) @binding(2) var filtering_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) surface: vec2<f32>,
};

struct FragmentIn {
    @location(0) coverage: vec2<f32>,
};

fn pbr_neutral(colour: vec4<f32>) -> vec4<f32> {
    let start = 0.76;
    let desaturation = 0.15;
    let lowest = min(colour.x, min(colour.y, colour.z));
    let offset = select(0.04, lowest - 6.25 * lowest * lowest, lowest < 0.08);
    let shifted = vec4<f32>(colour.x - offset, colour.y - offset, colour.z - offset, 1.0);
    let peak = max(shifted.x, max(shifted.y, shifted.z));
    if peak < start {
        return shifted;
    }
    let headroom = 1.0 - start;
    let compressed = 1.0 - headroom * headroom / (peak + headroom - start);
    let scale = compressed / peak;
    let grey = 1.0 - 1.0 / (desaturation * (peak - compressed) + 1.0);
    return vec4<f32>(
        shifted.x * scale + (compressed - shifted.x * scale) * grey,
        shifted.y * scale + (compressed - shifted.y * scale) * grey,
        shifted.z * scale + (compressed - shifted.z * scale) * grey,
        1.0,
    );
}

fn encoded(value: f32) -> f32 {
    let held = max(value, 0.0);
    return select(
        1.055 * pow(held, 0.41666666) - 0.055,
        held * 12.92,
        held <= 0.0031308,
    );
}

@vertex
fn vertex(@builtin(vertex_index) vertex: u32) -> VertexOut {
    let setup = settings[0u];
    var x = -1.0;
    var y = -1.0;
    if vertex == 1u {
        x = 3.0;
    }
    if vertex == 2u {
        y = 3.0;
    }
    var output: VertexOut;
    output.position = vec4<f32>(x, y, 0.0, 1.0);
    output.surface = vec2<f32>(x * 0.5 + 0.5, y * setup.vertical + 0.5);
    return output;
}

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    let setup = settings[0u];
    let at = input.coverage;
    let scene = textureSample(lit, filtering_sampler, at);
    let glow = textureSample(bloom, filtering_sampler, at);
    let held = vec4<f32>(
        scene.x + glow.x * setup.intensity,
        scene.y + glow.y * setup.intensity,
        scene.z + glow.z * setup.intensity,
        1.0,
    );
    let neutral = pbr_neutral(held);
    let step = vec2<f32>(1.0 / setup.source_width, 1.0 / setup.source_height);
    let left = textureSample(lit, filtering_sampler, vec2<f32>(at.x - 2.0 * step.x, at.y));
    let right = textureSample(lit, filtering_sampler, vec2<f32>(at.x + 2.0 * step.x, at.y));
    let down = textureSample(lit, filtering_sampler, vec2<f32>(at.x, at.y - 2.0 * step.y));
    let up = textureSample(lit, filtering_sampler, vec2<f32>(at.x, at.y + 2.0 * step.y));
    let near = max(max(left.w, right.w), max(down.w, up.w));
    let outlined = select(0.0, 1.0, near > 0.5 && scene.w < 0.5);
    let shaded = vec4<f32>(encoded(neutral.x), encoded(neutral.y), encoded(neutral.z), 1.0);
    let outline = vec4<f32>(0.30, 0.62, 1.0, 1.0);
    return vec4<f32>(
        shaded.x + (outline.x - shaded.x) * outlined,
        shaded.y + (outline.y - shaded.y) * outlined,
        shaded.z + (outline.z - shaded.z) * outlined,
        1.0,
    );
}

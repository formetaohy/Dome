@group(0) @binding(0) var<storage, read> settings: array<BloomUp>;
@group(1) @binding(0) var low: texture_2d<f32>;
@group(1) @binding(1) var high: texture_2d<f32>;
@group(1) @binding(2) var filtering_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) surface: vec2<f32>,
};

struct FragmentIn {
    @location(0) coverage: vec2<f32>,
};

fn tap(at: vec2<f32>, step: vec2<f32>, x: f32, y: f32) -> vec4<f32> {
    return textureSample(
        low,
        filtering_sampler,
        vec2<f32>(at.x + step.x * x, at.y + step.y * y),
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
    let step = vec2<f32>(1.0 / setup.source_width, 1.0 / setup.source_height);
    let at = input.coverage;
    let a = tap(at, step, -1.0, -1.0);
    let b = tap(at, step, 0.0, -1.0);
    let c = tap(at, step, 1.0, -1.0);
    let d = tap(at, step, -1.0, 0.0);
    let e = tap(at, step, 0.0, 0.0);
    let f = tap(at, step, 1.0, 0.0);
    let g = tap(at, step, -1.0, 1.0);
    let h = tap(at, step, 0.0, 1.0);
    let i = tap(at, step, 1.0, 1.0);
    let tent = (a + c + g + i) * 0.0625 + (b + d + f + h) * 0.125 + e * 0.25;
    return tent + textureSample(high, filtering_sampler, at);
}

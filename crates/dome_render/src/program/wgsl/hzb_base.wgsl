@group(0) @binding(0) var<storage, read> orientation: array<f32>;
@group(1) @binding(0) var depth: texture_depth_2d;
@group(1) @binding(1) var depth_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) surface: vec4<f32>,
};

struct FragmentIn {
    @location(0) coverage: vec4<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) vertex: u32) -> VertexOut {
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
    output.surface = vec4<f32>(x * 0.5 + 0.5, y * orientation[0u] + 0.5, 0.0, 0.0);
    return output;
}

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    let at = vec2<f32>(input.coverage.x, input.coverage.y);
    let step = dpdx(input.coverage);
    var deepest = 0.0;
    for (var y = 0u; y < 4u; y = y + 1u) {
        for (var x = 0u; x < 4u; x = x + 1u) {
            let offset = vec2<f32>(
                ((f32(x) + 0.5) / 4.0 - 0.5) * step.x,
                ((f32(y) + 0.5) / 4.0 - 0.5) * step.y,
            );
            deepest = max(
                deepest,
                textureSample(depth, depth_sampler, vec2<f32>(at.x + offset.x, at.y + offset.y)),
            );
        }
    }
    return vec4<f32>(deepest, 0.0, 0.0, 1.0);
}

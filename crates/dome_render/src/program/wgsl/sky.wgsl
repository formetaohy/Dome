@group(0) @binding(0) var<storage, read> view: array<Sky>;
@group(1) @binding(0) var radiance: texture_2d<f32>;
@group(1) @binding(1) var radiance_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) screen: vec2<f32>,
};

struct FragmentIn {
    @location(0) at: vec2<f32>,
};

fn clamped(value: f32, low: f32, high: f32) -> f32 {
    return min(max(value, low), high);
}

fn normalized(vector: vec4<f32>) -> vec4<f32> {
    let inverse =
        1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
    return vector * inverse;
}

fn turned(
    first: vec4<f32>,
    second: vec4<f32>,
    third: vec4<f32>,
    vector: vec4<f32>,
) -> vec4<f32> {
    return vec4<f32>(
        first.x * vector.x + second.x * vector.y + third.x * vector.z,
        first.y * vector.x + second.y * vector.y + third.y * vector.z,
        first.z * vector.x + second.z * vector.y + third.z * vector.z,
        0.0,
    );
}

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
    output.position = vec4<f32>(x, y, 1.0, 1.0);
    output.screen = vec2<f32>(x, y);
    return output;
}

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    let held = view[0u];
    let ray = input.at;
    let far = held.inverse0 * ray.x + held.inverse1 * ray.y + held.inverse2 + held.inverse3;
    let at = vec4<f32>(far.x / far.w, far.y / far.w, far.z / far.w, 1.0) - held.eye;
    let unit = turned(held.environment0, held.environment1, held.environment2, normalized(at));
    let held_radiance = textureSampleLevel(
        radiance,
        radiance_sampler,
        vec2<f32>(
            atan2(unit.x, -unit.z) * (113.0 / 710.0) + 0.5,
            acos(clamped(unit.y, -1.0, 1.0)) * (113.0 / 355.0),
        ),
        0.0,
    );
    return vec4<f32>(
        held_radiance.x * held.intensity,
        held_radiance.y * held.intensity,
        held_radiance.z * held.intensity,
        0.0,
    );
}

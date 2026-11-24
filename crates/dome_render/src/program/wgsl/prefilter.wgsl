@group(0) @binding(0) var<storage, read> level: array<BakeLevel>;
@group(1) @binding(0) var panorama: texture_2d<f32>;
@group(1) @binding(1) var panorama_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) surface: vec2<f32>,
};

struct FragmentIn {
    @location(0) at: vec2<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) vertex: u32) -> VertexOut {
    var x = -1.0;
    var y = -1.0;
    if (vertex == 1u) {
        x = 3.0;
    }
    if (vertex == 2u) {
        y = 3.0;
    }
    var output: VertexOut;
    output.position = vec4<f32>(x, y, 0.0, 1.0);
    output.surface = vec2<f32>(x, y);
    return output;
}

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    let held = level[0u];
    let normal = ray(vec2<f32>(input.at.x * 0.5 + 0.5, input.at.y * held.vertical + 0.5));
    let radiance = prefiltered(panorama, panorama_sampler, normal, held.roughness);
    return vec4<f32>(radiance.x, radiance.y, radiance.z, 1.0);
}

fn prefiltered(
    panorama: texture_2d<f32>,
    sampler: sampler,
    normal: vec4<f32>,
    roughness: f32,
) -> vec4<f32> {
    if (roughness <= 0.0) {
        return textureSampleLevel(panorama, sampler, equirect(normal), 0.0);
    }
    let view = normalized(normal);
    var sign = -1.0;
    if (view.z >= 0.0) {
        sign = 1.0;
    }
    let frame = -1.0 / (sign + view.z);
    let cross = view.x * view.y * frame;
    let tangent = vec4<f32>(
        1.0 + sign * view.x * view.x * frame,
        sign * cross,
        -sign * view.x,
        0.0,
    );
    let bitangent = vec4<f32>(cross, sign + view.y * view.y * frame, -view.y, 0.0);
    let alpha = roughness * roughness;
    var sum = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var weight = 0.0;
    for (var index = 0u; index < 128u; index = index + 1u) {
        let rank = radical(index);
        let phi = (710.0 / 113.0) * (f32(index) * (1.0 / 128.0));
        let cosine = sqrt(max(
            (1.0 - rank) / (1.0 + (alpha * alpha - 1.0) * rank),
            0.0,
        ));
        let sine = sqrt(max(1.0 - cosine * cosine, 0.0));
        let half = tangent * (sine * cos(phi)) + bitangent * (sine * sin(phi)) + view * cosine;
        let light = normalized(half * (2.0 * dot3(view, half)) - view);
        let incidence = max(dot3(view, light), 0.0);
        if (incidence > 0.0) {
            sum = sum + textureSampleLevel(panorama, sampler, equirect(light), 0.0) * incidence;
            weight += incidence;
        }
    }
    if (weight == 0.0) {
        return textureSampleLevel(panorama, sampler, equirect(view), 0.0);
    }
    return sum * (1.0 / weight);
}

fn radical(bits: u32) -> f32 {
    var weight = 0.5;
    var value = 0.0;
    for (var bit = 0u; bit < 7u; bit = bit + 1u) {
        if (((bits >> bit) & 1u) == 1u) {
            value += weight;
        }
        weight *= 0.5;
    }
    return value;
}

fn ray(at: vec2<f32>) -> vec4<f32> {
    let phi = (at.x - 0.5) * (710.0 / 113.0);
    let theta = at.y * (355.0 / 113.0);
    let sine = sin(theta);
    return vec4<f32>(sine * sin(phi), cos(theta), -sine * cos(phi), 0.0);
}

fn equirect(direction: vec4<f32>) -> vec2<f32> {
    let unit = normalized(direction);
    return vec2<f32>(
        atan2(unit.x, -unit.z) * (113.0 / 710.0) + 0.5,
        acos(clamped(unit.y, -1.0, 1.0)) * (113.0 / 355.0),
    );
}

fn dot3(left: vec4<f32>, right: vec4<f32>) -> f32 {
    return left.x * right.x + left.y * right.y + left.z * right.z;
}

fn normalized(vector: vec4<f32>) -> vec4<f32> {
    let inverse =
        1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
    return vector * inverse;
}

fn clamped(value: f32, low: f32, high: f32) -> f32 {
    return min(max(value, low), high);
}

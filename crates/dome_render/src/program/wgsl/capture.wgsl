@group(0) @binding(0) var<storage, read> setup: array<Atlas>;
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) surface: vec2<f32>,
};

struct FragmentIn {
    @location(0) at: vec2<f32>,
};

fn face_of(direction: vec4<f32>, face: f32, texel: f32) -> vec2<f32> {
    let unit = normalized(direction);
    let across = abs(unit.x);
    let up = abs(unit.y);
    let forward = abs(unit.z);
    var index = 0.0;
    var horizontal = -unit.z;
    var vertical = -unit.y;
    var major = across;
    if across >= up && across >= forward {
        if unit.x < 0.0 {
            index = 1.0;
            horizontal = unit.z;
        }
    } else if up >= forward {
        major = up;
        index = 2.0;
        horizontal = unit.x;
        vertical = unit.z;
        if unit.y < 0.0 {
            index = 3.0;
            vertical = -unit.z;
        }
    } else {
        major = forward;
        index = 4.0;
        horizontal = unit.x;
        vertical = -unit.y;
        if unit.z < 0.0 {
            index = 5.0;
            horizontal = -unit.x;
        }
    }
    let span = max(major, 0.000001);
    let left = 0.5 + 0.5 * horizontal / span;
    let top = 0.5 - 0.5 * vertical / span;
    return vec2<f32>(
        (index + clamped(left, texel, 1.0 - texel)) * face,
        clamped(top, texel, 1.0 - texel),
    );
}

fn ray(at: vec2<f32>) -> vec4<f32> {
    let phi = (at.x - 0.5) * (710.0 / 113.0);
    let theta = at.y * (355.0 / 113.0);
    let sine = sin(theta);
    return vec4<f32>(sine * sin(phi), cos(theta), -sine * cos(phi), 0.0);
}

fn normalized(vector: vec4<f32>) -> vec4<f32> {
    let inverse =
        1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
    return vector * inverse;
}

fn clamped(value: f32, low: f32, high: f32) -> f32 {
    return min(max(value, low), high);
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
    output.position = vec4<f32>(x, y, 0.0, 1.0);
    output.surface = vec2<f32>(x, y);
    return output;
}

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    let held = setup[0u];
    let direction = ray(vec2<f32>(input.at.x * 0.5 + 0.5, input.at.y * held.vertical + 0.5));
    let lit = textureSampleLevel(
        atlas,
        atlas_sampler,
        face_of(direction, held.face, held.texel),
        0.0,
    );
    return vec4<f32>(lit.x, lit.y, lit.z, 1.0);
}

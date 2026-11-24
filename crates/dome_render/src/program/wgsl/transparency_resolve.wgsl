@group(0) @binding(0) var<storage, read> settings: array<TransparentSetup>;
@group(0) @binding(1) var<storage, read> keys: array<u32>;
@group(0) @binding(2) var<storage, read> colors: array<u32>;
@group(1) @binding(0) var source: texture_2d<f32>;
@group(1) @binding(1) var filtering_sampler: sampler;

const LAYERS: u32 = 4u;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) surface: vec2<f32>,
};

struct FragmentIn {
    @location(0) coverage: vec2<f32>,
};

fn decoded(color: u32, shift: u32) -> f32 {
    let exponent = color >> 24u;
    let scale = select(
        0.0,
        bitcast<f32>((exponent - 1u) << 23u),
        exponent > 1u,
    ) * (1.0 / 255.0);
    return f32((color >> shift) & 255u) * scale;
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
    let width = f32(setup.width);
    let height = f32(setup.height);
    let pixel_x = u32(min(max(floor(at.x * width), 0.0), width - 1.0));
    let pixel_y = u32(min(max(floor(at.y * height), 0.0), height - 1.0));
    let pixel = pixel_y * setup.width + pixel_x;
    let held = pixel * setup.layers;
    var depth = array<u32, LAYERS>(0xFFFFFFFFu, 0xFFFFFFFFu, 0xFFFFFFFFu, 0xFFFFFFFFu);
    var color = array<u32, LAYERS>(0u, 0u, 0u, 0u);
    for (var layer = 0u; layer < setup.layers; layer = layer + 1u) {
        let occupant = keys[held + layer];
        if occupant <= 1u {
            continue;
        }
        var placed_depth = occupant;
        var placed_color = colors[held + layer];
        for (var slot = 0u; slot < LAYERS; slot = slot + 1u) {
            if placed_depth < depth[slot] {
                let held_depth = depth[slot];
                let held_color = color[slot];
                depth[slot] = placed_depth;
                color[slot] = placed_color;
                placed_depth = held_depth;
                placed_color = held_color;
            }
        }
    }
    var through = 1.0;
    var resolved = vec3<f32>(0.0, 0.0, 0.0);
    for (var slot = 0u; slot < setup.layers; slot = slot + 1u) {
        if depth[slot] == 0xFFFFFFFFu {
            break;
        }
        let coverage = f32(depth[slot] & 255u) * (1.0 / 255.0);
        let weight = coverage * through;
        resolved = resolved + vec3<f32>(
            decoded(color[slot], 16u),
            decoded(color[slot], 8u),
            decoded(color[slot], 0u),
        ) * weight;
        through = through * (1.0 - coverage);
    }
    let base = textureSample(source, filtering_sampler, at);
    return vec4<f32>(
        resolved.x + base.x * through,
        resolved.y + base.y * through,
        resolved.z + base.z * through,
        through,
    );
}

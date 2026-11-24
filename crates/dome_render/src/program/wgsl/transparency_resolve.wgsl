@group(0) @binding(0) var<storage, read> settings: array<TransparentSetup>;
@group(0) @binding(1) var<storage, read> keys: array<u32>;
@group(0) @binding(2) var<storage, read> colors: array<u32>;
@group(1) @binding(0) var source: texture_2d<f32>;
@group(1) @binding(1) var filtering_sampler: sampler;

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
    let slot0 = keys[held];
    let slot1 = keys[held + 1u];
    let slot2 = keys[held + 2u];
    let slot3 = keys[held + 3u];
    let filled0 = slot0 > 1u;
    let filled1 = slot1 > 1u;
    let filled2 = slot2 > 1u;
    let filled3 = slot3 > 1u;
    var color0 = select(0u, colors[held], filled0);
    var depth0 = select(0u, slot0, filled0);
    var color1 = select(0u, colors[held + 1u], filled1);
    var depth1 = select(0u, slot1, filled1);
    var color2 = select(0u, colors[held + 2u], filled2);
    var depth2 = select(0u, slot2, filled2);
    var color3 = select(0u, colors[held + 3u], filled3);
    var depth3 = select(0u, slot3, filled3);
    let ordering01 = depth1 < depth0;
    let low_color01 = select(color0, color1, ordering01);
    let low_depth01 = select(depth0, depth1, ordering01);
    color1 = select(color1, color0, ordering01);
    depth1 = select(depth1, depth0, ordering01);
    color0 = low_color01;
    depth0 = low_depth01;
    let ordering23 = depth3 < depth2;
    let low_color23 = select(color2, color3, ordering23);
    let low_depth23 = select(depth2, depth3, ordering23);
    color3 = select(color3, color2, ordering23);
    depth3 = select(depth3, depth2, ordering23);
    color2 = low_color23;
    depth2 = low_depth23;
    let ordering02 = depth2 < depth0;
    let low_color02 = select(color0, color2, ordering02);
    let low_depth02 = select(depth0, depth2, ordering02);
    color2 = select(color2, color0, ordering02);
    depth2 = select(depth2, depth0, ordering02);
    color0 = low_color02;
    depth0 = low_depth02;
    let ordering13 = depth3 < depth1;
    let low_color13 = select(color1, color3, ordering13);
    let low_depth13 = select(depth1, depth3, ordering13);
    color3 = select(color3, color1, ordering13);
    depth3 = select(depth3, depth1, ordering13);
    color1 = low_color13;
    depth1 = low_depth13;
    let ordering12 = depth2 < depth1;
    let low_color12 = select(color1, color2, ordering12);
    let low_depth12 = select(depth1, depth2, ordering12);
    color2 = select(color2, color1, ordering12);
    depth2 = select(depth2, depth1, ordering12);
    color1 = low_color12;
    depth1 = low_depth12;
    let first = f32(depth0 & 255u) * (1.0 / 255.0);
    let second = f32(depth1 & 255u) * (1.0 / 255.0);
    let third = f32(depth2 & 255u) * (1.0 / 255.0);
    let fourth = f32(depth3 & 255u) * (1.0 / 255.0);
    let through_first = 1.0 - first;
    let through_second = through_first * (1.0 - second);
    let through_third = through_second * (1.0 - third);
    let transmit = through_third * (1.0 - fourth);
    let weight0 = first;
    let weight1 = second * through_first;
    let weight2 = third * through_second;
    let weight3 = fourth * through_third;
    let base = textureSample(source, filtering_sampler, at);
    return vec4<f32>(
        decoded(color0, 16u) * weight0
            + decoded(color1, 16u) * weight1
            + decoded(color2, 16u) * weight2
            + decoded(color3, 16u) * weight3
            + base.x * transmit,
        decoded(color0, 8u) * weight0
            + decoded(color1, 8u) * weight1
            + decoded(color2, 8u) * weight2
            + decoded(color3, 8u) * weight3
            + base.y * transmit,
        decoded(color0, 0u) * weight0
            + decoded(color1, 0u) * weight1
            + decoded(color2, 0u) * weight2
            + decoded(color3, 0u) * weight3
            + base.z * transmit,
        base.w * transmit,
    );
}

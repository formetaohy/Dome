@group(0) @binding(0) var<storage, read> setup: array<IrradianceSetup>;
@group(0) @binding(1) var<storage, read_write> partials: array<Irradiance>;
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
    output.surface = vec2<f32>(x * 0.5 + 0.5, y * 0.5 + 0.5);
    return output;
}

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    let held = setup[0u];
    let index = u32(floor(input.at.x * f32(held.partials)));
    let width = held.width;
    let height = held.height;
    let width_f = f32(width);
    let height_f = f32(height);
    let solid = ((710.0 / 113.0) / width_f) * ((355.0 / 113.0) / height_f);
    var first = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var second = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var third = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var fourth = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var fifth = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var sixth = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var seventh = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var eighth = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var ninth = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    for (var texel = index; texel < held.texels; texel = texel + held.partials) {
        let column = texel % width;
        let row = texel / width;
        let u = (f32(column) + 0.5) / width_f;
        let v = (f32(row) + 0.5) / height_f;
        let phi = (u - 0.5) * (710.0 / 113.0);
        let theta = v * (355.0 / 113.0);
        let sine = sin(theta);
        let axis_x = sine * sin(phi);
        let axis_y = cos(theta);
        let axis_z = -sine * cos(phi);
        let radiance = textureSampleLevel(panorama, panorama_sampler, vec2<f32>(u, v), 0.0)
            * (sin(v * (355.0 / 113.0)) * solid);
        first = first + radiance * 0.886227;
        second = second + radiance * (1.0233276 * axis_y);
        third = third + radiance * (1.0233276 * axis_z);
        fourth = fourth + radiance * (1.0233276 * axis_x);
        fifth = fifth + radiance * (0.8580851 * axis_x * axis_y);
        sixth = sixth + radiance * (0.8580851 * axis_y * axis_z);
        seventh = seventh + radiance * (0.2477083 * (3.0 * axis_z * axis_z - 1.0));
        eighth = eighth + radiance * (0.8580851 * axis_x * axis_z);
        ninth = ninth + radiance * (0.429041 * (axis_x * axis_x - axis_y * axis_y));
    }
    partials[index].coefficient0 = first;
    partials[index].coefficient1 = second;
    partials[index].coefficient2 = third;
    partials[index].coefficient3 = fourth;
    partials[index].coefficient4 = fifth;
    partials[index].coefficient5 = sixth;
    partials[index].coefficient6 = seventh;
    partials[index].coefficient7 = eighth;
    partials[index].coefficient8 = ninth;
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

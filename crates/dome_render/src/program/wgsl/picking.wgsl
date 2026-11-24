@group(0) @binding(0) var<storage, read> view: array<View>;
@group(0) @binding(1) var<storage, read> palette: array<Joint>;
@group(0) @binding(2) var<storage, read> skins: array<SkinVertex>;
@group(0) @binding(3) var<storage, read> drawn: array<Draw>;
@group(0) @binding(4) var<storage, read> vertices: array<f32>;

fn deformed(col0: vec4<f32>, col1: vec4<f32>, col2: vec4<f32>, col3: vec4<f32>, point: vec4<f32>) -> vec4<f32> {
    return col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w;
}

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) picked: vec4<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> VertexOut {
    let held = drawn[instance];
    let camera = view[0u];
    let base = vertex * 8u;
    let rest = vec4<f32>(
        vertices[base],
        vertices[base + 1u],
        vertices[base + 2u],
        1.0,
    );
    let id = held.pick;
    var out: VertexOut;
    out.picked = vec4<f32>(
        f32(id % 256u) / 255.0,
        f32((id / 256u) % 256u) / 255.0,
        f32((id / 65536u) % 256u) / 255.0,
        1.0,
    );
    if held.joints > 0u {
        let skin = skins[vertex];
        let first = palette[held.palette + skin.joints.x];
        let second = palette[held.palette + skin.joints.y];
        let third = palette[held.palette + skin.joints.z];
        let fourth = palette[held.palette + skin.joints.w];
        let place = deformed(first.col0, first.col1, first.col2, first.col3, rest)
            * skin.weights.x
            + deformed(second.col0, second.col1, second.col2, second.col3, rest)
                * skin.weights.y
            + deformed(third.col0, third.col1, third.col2, third.col3, rest) * skin.weights.z
            + deformed(fourth.col0, fourth.col1, fourth.col2, fourth.col3, rest)
                * skin.weights.w;
        let local = vec4<f32>(place.x, place.y, place.z, 1.0);
        let moved = held.model0 * local.x
            + held.model1 * local.y
            + held.model2 * local.z
            + held.model3 * local.w;
        out.position = camera.clip0 * moved.x
            + camera.clip1 * moved.y
            + camera.clip2 * moved.z
            + camera.clip3;
    } else {
        let moved = held.model0 * rest.x
            + held.model1 * rest.y
            + held.model2 * rest.z
            + held.model3 * rest.w;
        out.position = camera.clip0 * moved.x
            + camera.clip1 * moved.y
            + camera.clip2 * moved.z
            + camera.clip3;
    }
    return out;
}

struct FragmentIn {
    @location(0) space: vec4<f32>,
};

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    return input.space;
}

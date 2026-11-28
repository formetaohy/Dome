@group(0) @binding(0) var<storage, read> settings: array<Bloom>;
@group(1) @binding(0) var source: texture_2d<f32>;
@group(1) @binding(1) var filtering_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) surface: vec2<f32>,
};

struct FragmentIn {
    @location(0) coverage: vec2<f32>,
};

fn tap(at: vec2<f32>, step: vec2<f32>, x: f32, y: f32) -> vec4<f32> {
    return textureSample(
        source,
        filtering_sampler,
        vec2<f32>(at.x + step.x * x, at.y + step.y * y),
    );
}

fn luma(value: vec4<f32>) -> f32 {
    return 0.2126 * value.x + 0.7152 * value.y + 0.0722 * value.z;
}

fn karis(a: vec4<f32>, b: vec4<f32>, c: vec4<f32>, d: vec4<f32>) -> vec4<f32> {
    let wa = 1.0 / (1.0 + luma(a));
    let wb = 1.0 / (1.0 + luma(b));
    let wc = 1.0 / (1.0 + luma(c));
    let wd = 1.0 / (1.0 + luma(d));
    let total = wa + wb + wc + wd;
    return vec4<f32>(
        (a.x * wa + b.x * wb + c.x * wc + d.x * wd) / total,
        (a.y * wa + b.y * wb + c.y * wc + d.y * wd) / total,
        (a.z * wa + b.z * wb + c.z * wc + d.z * wd) / total,
        1.0,
    );
}

fn clamped(value: f32, low: f32, high: f32) -> f32 {
    return min(max(value, low), high);
}

fn thresholded(value: vec4<f32>, threshold: f32, knee: f32) -> vec4<f32> {
    let brightness = max(value.x, max(value.y, value.z));
    let soft = clamped(brightness - threshold + knee, 0.0, 2.0 * knee);
    let softened = soft * soft / (4.0 * knee + 0.00001);
    let contribution = max(softened, brightness - threshold) / max(brightness, 0.00001);
    return value * contribution;
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
    let a = tap(at, step, -2.0, 2.0);
    let b = tap(at, step, 0.0, 2.0);
    let c = tap(at, step, 2.0, 2.0);
    let d = tap(at, step, -2.0, 0.0);
    let e = tap(at, step, 0.0, 0.0);
    let f = tap(at, step, 2.0, 0.0);
    let g = tap(at, step, -2.0, -2.0);
    let h = tap(at, step, 0.0, -2.0);
    let i = tap(at, step, 2.0, -2.0);
    let j = tap(at, step, -1.0, 1.0);
    let k = tap(at, step, 1.0, 1.0);
    let l = tap(at, step, -1.0, -1.0);
    let m = tap(at, step, 1.0, -1.0);
    let gathered = karis(a, b, d, e) * 0.125
        + karis(b, c, e, f) * 0.125
        + karis(d, e, g, h) * 0.125
        + karis(e, f, h, i) * 0.125
        + karis(j, k, l, m) * 0.5;
    return thresholded(gathered, setup.threshold, setup.knee);
}

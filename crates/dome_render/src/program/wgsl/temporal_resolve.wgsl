@group(0) @binding(0) var<storage, read> frame: array<Temporal>;
@group(1) @binding(0) var source: texture_2d<f32>;
@group(1) @binding(1) var history: texture_2d<f32>;
@group(1) @binding(2) var filtering_sampler: sampler;
@group(2) @binding(0) var motion: texture_2d<f32>;
@group(2) @binding(1) var previous: texture_2d<f32>;
@group(2) @binding(2) var nearest_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) surface: vec2<f32>,
};

struct FragmentIn {
    @location(0) coverage: vec2<f32>,
};

fn column(a: vec4<f32>, b: vec4<f32>, c: vec4<f32>, d: vec4<f32>, local: vec4<f32>) -> vec4<f32> {
    return a * local.x + b * local.y + c * local.z + d * local.w;
}

fn lowest(left: vec4<f32>, right: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        min(left.x, right.x),
        min(left.y, right.y),
        min(left.z, right.z),
        0.0,
    );
}

fn highest(left: vec4<f32>, right: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        max(left.x, right.x),
        max(left.y, right.y),
        max(left.z, right.z),
        0.0,
    );
}

fn clamped(value: f32, low: f32, high: f32) -> f32 {
    return min(max(value, low), high);
}

fn dot3(left: vec4<f32>, right: vec4<f32>) -> f32 {
    return left.x * right.x + left.y * right.y + left.z * right.z;
}

fn normalized(vector: vec4<f32>) -> vec4<f32> {
    let inverse =
        1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
    return vector * inverse;
}

@vertex
fn vertex(@builtin(vertex_index) vertex: u32) -> VertexOut {
    let held = frame[0u];
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
    output.surface = vec2<f32>(x * 0.5 + 0.5, y * held.vertical + 0.5);
    return output;
}

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    let setup = frame[0u];
    let at = input.coverage;
    let recorded = textureSample(motion, nearest_sampler, at);
    let sky = recorded.w < 0.5;
    var reach = setup.far;
    if !sky {
        reach = recorded.z;
    }
    var history_at = vec2<f32>(at.x + recorded.x, at.y + recorded.y);
    let ndc_x = at.x * 2.0 - 1.0;
    let ndc_y = (at.y - 0.5) / setup.vertical;
    let head = column(
        setup.inverse0,
        setup.inverse1,
        setup.inverse2,
        setup.inverse3,
        vec4<f32>(ndc_x, ndc_y, 0.0, 1.0),
    );
    let tail = column(
        setup.inverse0,
        setup.inverse1,
        setup.inverse2,
        setup.inverse3,
        vec4<f32>(ndc_x, ndc_y, 1.0, 1.0),
    );
    let head_point = vec4<f32>(head.x / head.w, head.y / head.w, head.z / head.w, 1.0);
    let tail_point = vec4<f32>(tail.x / tail.w, tail.y / tail.w, tail.z / tail.w, 1.0);
    let ray = normalized(tail_point - head_point);
    var travel = setup.far;
    if !sky {
        travel = reach / max(dot3(ray, setup.forward), 0.0001);
    }
    let world = vec4<f32>(
        setup.eye.x + ray.x * travel,
        setup.eye.y + ray.y * travel,
        setup.eye.z + ray.z * travel,
        1.0,
    );
    let previous_clip = column(
        setup.previous0,
        setup.previous1,
        setup.previous2,
        setup.previous3,
        world,
    );
    let previous_inverse = 1.0 / max(previous_clip.w, 0.0001);
    let previous_at = vec2<f32>(
        previous_clip.x * previous_inverse * 0.5 + 0.5,
        previous_clip.y * previous_inverse * setup.vertical + 0.5,
    );
    if sky {
        history_at = previous_at;
    }
    let previous_reach = (world.x - setup.previous_eye.x) * setup.previous_forward.x
        + (world.y - setup.previous_eye.y) * setup.previous_forward.y
        + (world.z - setup.previous_eye.z) * setup.previous_forward.z;
    let carried = textureSample(previous, nearest_sampler, history_at);
    var valid = history_at.x > 0.0
        && history_at.x < 1.0
        && history_at.y > 0.0
        && history_at.y < 1.0
        && previous_clip.w > 0.0001;
    if carried.w > 0.5 {
        valid = valid
            && abs(carried.z - previous_reach) <= setup.tolerance * max(previous_reach, setup.near);
    } else {
        valid = valid && sky;
    }
    let current = textureSample(source, filtering_sampler, at);
    let held = textureSample(history, filtering_sampler, history_at);
    let step = vec2<f32>(1.0 / setup.width, 1.0 / setup.height);
    let left = textureSample(source, filtering_sampler, vec2<f32>(at.x - step.x, at.y));
    let right = textureSample(source, filtering_sampler, vec2<f32>(at.x + step.x, at.y));
    let below = textureSample(source, filtering_sampler, vec2<f32>(at.x, at.y - step.y));
    let above = textureSample(source, filtering_sampler, vec2<f32>(at.x, at.y + step.y));
    let shade_low = lowest(lowest(lowest(left, right), lowest(below, above)), current);
    let shade_high = highest(highest(highest(left, right), highest(below, above)), current);
    let bounded = vec4<f32>(
        clamped(held.x, shade_low.x, shade_high.x),
        clamped(held.y, shade_low.y, shade_high.y),
        clamped(held.z, shade_low.z, shade_high.z),
        1.0,
    );
    let weight = select(0.0, setup.blend, valid);
    return vec4<f32>(
        bounded.x * weight + current.x * (1.0 - weight),
        bounded.y * weight + current.y * (1.0 - weight),
        bounded.z * weight + current.z * (1.0 - weight),
        current.w,
    );
}

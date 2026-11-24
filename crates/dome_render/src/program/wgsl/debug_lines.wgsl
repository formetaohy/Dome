@group(0) @binding(0) var<storage, read> view: array<View>;
@group(0) @binding(1) var<storage, read> segments: array<DebugLine>;

fn column(a: vec4<f32>, b: vec4<f32>, c: vec4<f32>, d: vec4<f32>, local: vec4<f32>) -> vec4<f32> {
    return a * local.x + b * local.y + c * local.z + d * local.w;
}

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) tint: vec4<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) vertex: u32) -> VertexOut {
    let camera = view[0u];
    let segment = segments[vertex / 6u];
    let corner = vertex % 6u;
    let head = column(camera.clip0, camera.clip1, camera.clip2, camera.clip3, segment.start);
    let tail = column(camera.clip0, camera.clip1, camera.clip2, camera.clip3, segment.end);
    var x = 2.0;
    var y = 2.0;
    var z = 1.0;
    var alpha = 0.0;
    if head.w > 0.0001 && tail.w > 0.0001 {
        let head_x = head.x / head.w;
        let head_y = head.y / head.w;
        let tail_x = tail.x / tail.w;
        let tail_y = tail.y / tail.w;
        let span_x = (tail_x - head_x) * camera.view_width;
        let span_y = (tail_y - head_y) * camera.view_height;
        let reach = max(sqrt(span_x * span_x + span_y * span_y), 0.0001);
        let normal_x = -span_y / reach * 2.0 / camera.view_width;
        let normal_y = span_x / reach * 2.0 / camera.view_height;
        var end = 0.0;
        var side = -1.0;
        if corner == 1u {
            side = 1.0;
        }
        if corner == 2u {
            end = 1.0;
            side = 1.0;
        }
        if corner == 4u {
            end = 1.0;
            side = 1.0;
        }
        if corner == 5u {
            end = 1.0;
            side = -1.0;
        }
        x = head_x + (tail_x - head_x) * end + normal_x * side;
        y = head_y + (tail_y - head_y) * end + normal_y * side;
        z = head.z / head.w + (tail.z / tail.w - head.z / head.w) * end;
        alpha = segment.color.w;
    }
    var out: VertexOut;
    out.position = vec4<f32>(x, y, z, 1.0);
    out.tint = vec4<f32>(segment.color.x, segment.color.y, segment.color.z, alpha);
    return out;
}

struct FragmentIn {
    @location(0) color: vec4<f32>,
};

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    let color = input.color;
    return vec4<f32>(color.x * color.w, color.y * color.w, color.z * color.w, color.w);
}

@group(0) @binding(0) var<storage, read> view: array<View>;
@group(0) @binding(1) var<storage, read> points: array<u32>;
@group(0) @binding(2) var<storage, read_write> clusters: array<atomic<u32>>;

fn column(a: vec4<f32>, b: vec4<f32>, c: vec4<f32>, d: vec4<f32>, local: vec4<f32>) -> vec4<f32> {
    return a * local.x + b * local.y + c * local.z + d * local.w;
}

@compute @workgroup_size(64)
fn compute(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) lid: u32,
) {
    let held = view[0u];
    let tiles = held.cluster_x * held.cluster_y;
    if tiles == 0u || held.point_lights == 0u {
        return;
    }
    let tile = group.x;
    let slice = group.y;
    if tile >= tiles || slice >= 24u {
        return;
    }
    let tile_x = tile % held.cluster_x;
    let tile_y = tile / held.cluster_x;
    let index = (slice * held.cluster_y + tile_y) * held.cluster_x + tile_x;
    let cluster_base = index * 33u;
    let low_x = f32(tile_x) * 64.0;
    let high_x = low_x + 64.0;
    let low_y = f32(tile_y) * 64.0;
    let high_y = low_y + 64.0;
    let ratio = held.far / held.near;
    let slice_near = held.near * pow(ratio, f32(slice) / 24.0);
    let slice_far = held.near * pow(ratio, f32(slice + 1u) / 24.0);
    for (var light = lid; light < held.point_lights; light = light + 64u) {
        let at = light * 12u;
        let point_x = bitcast<f32>(points[at]);
        let point_y = bitcast<f32>(points[at + 1u]);
        let point_z = bitcast<f32>(points[at + 2u]);
        let point_range = bitcast<f32>(points[at + 3u]);
        let relative = vec4<f32>(
            point_x - held.eye.x,
            point_y - held.eye.y,
            point_z - held.eye.z,
            0.0,
        );
        let reach = relative.x * held.forward.x
            + relative.y * held.forward.y
            + relative.z * held.forward.z;
        let nearest = reach - point_range;
        let deepest = reach + point_range;
        if deepest >= slice_near && nearest <= slice_far {
            var covers = nearest <= held.near;
            if !covers {
                let center = vec4<f32>(point_x, point_y, point_z, 1.0);
                let clip = column(held.clip0, held.clip1, held.clip2, held.clip3, center);
                if clip.w > 0.0 {
                    let inverse = 1.0 / clip.w;
                    let middle = vec2<f32>(
                        clip.x * inverse * 0.5 + 0.5,
                        clip.y * inverse * held.vertical + 0.5,
                    );
                    let half_y = point_range / (nearest * held.tan_vertical) * 0.5;
                    let half_x = half_y / held.aspect;
                    let pad = vec2<f32>(1.0 / held.view_width, 1.0 / held.view_height);
                    let low = vec2<f32>(middle.x - half_x - pad.x, middle.y - half_y - pad.y);
                    let high = vec2<f32>(middle.x + half_x + pad.x, middle.y + half_y + pad.y);
                    covers = high.x * held.view_width >= low_x
                        && low.x * held.view_width <= high_x
                        && high.y * held.view_height >= low_y
                        && low.y * held.view_height <= high_y;
                } else {
                    covers = true;
                }
            }
            if covers {
                let slot = atomicAdd(&clusters[cluster_base], 1u);
                if slot < 32u {
                    atomicStore(&clusters[cluster_base + 1u + slot], light);
                }
            }
        }
    }
}

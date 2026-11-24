@group(0) @binding(0) var<storage, read> view: array<View>;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;
@group(0) @binding(2) var<storage, read_write> drawn: array<Draw>;
@group(0) @binding(3) var<storage, read_write> arguments: array<atomic<u32>>;
@group(0) @binding(4) var hzb: texture_2d<f32>;
@group(0) @binding(5) var hzb_sampler: sampler;

fn inside(plane: vec4<f32>, sphere: vec4<f32>) -> bool {
    return plane.x * sphere.x + plane.y * sphere.y + plane.z * sphere.z + plane.w > -sphere.w;
}

fn column(a: vec4<f32>, b: vec4<f32>, c: vec4<f32>, d: vec4<f32>, local: vec4<f32>) -> vec4<f32> {
    return a * local.x + b * local.y + c * local.z + d * local.w;
}

@compute @workgroup_size(64)
fn compute(@builtin(global_invocation_id) global: vec3<u32>) {
    let index = global.x;
    let held = view[0u];
    if index < held.instances {
        let instance = instances[index];
        let bounds = instance.bounds;
        if inside(held.plane0, bounds)
            && inside(held.plane1, bounds)
            && inside(held.plane2, bounds)
            && inside(held.plane3, bounds)
            && inside(held.plane4, bounds)
            && inside(held.plane5, bounds)
        {
            var occluded = false;
            if held.hzb_levels > 0u && instance.hzb > 0u {
                let relative = vec4<f32>(
                    bounds.x - held.eye.x,
                    bounds.y - held.eye.y,
                    bounds.z - held.eye.z,
                    0.0,
                );
                let reach = relative.x * held.forward.x
                    + relative.y * held.forward.y
                    + relative.z * held.forward.z;
                let nearest = reach - bounds.w;
                if nearest > held.near && reach < held.far {
                    let center = vec4<f32>(bounds.x, bounds.y, bounds.z, 1.0);
                    let clip = column(held.clip0, held.clip1, held.clip2, held.clip3, center);
                    if clip.w > 0.0 {
                        let inverse = 1.0 / clip.w;
                        let middle = vec2<f32>(
                            clip.x * inverse * 0.5 + 0.5,
                            clip.y * inverse * held.vertical + 0.5,
                        );
                        let half_y = bounds.w / (nearest * held.tan_vertical) * 0.5;
                        let half_x = half_y / held.aspect;
                        let pad = vec2<f32>(2.0 / held.view_width, 2.0 / held.view_height);
                        let low = vec2<f32>(middle.x - half_x - pad.x, middle.y - half_y - pad.y);
                        let high = vec2<f32>(middle.x + half_x + pad.x, middle.y + half_y + pad.y);
                        if low.x >= 0.0 && high.x <= 1.0 && low.y >= 0.0 && high.y <= 1.0 {
                            let base = vec2<f32>(
                                floor((held.view_width + 3.0) * 0.25),
                                floor((held.view_height + 3.0) * 0.25),
                            );
                            let low_x = low.x * base.x;
                            let low_y = low.y * base.y;
                            let high_x = high.x * base.x;
                            let high_y = high.y * base.y;
                            var level = 0u;
                            var factor = 1.0;
                            var extent = max(high_x - low_x, high_y - low_y);
                            while extent > 4.0 && level + 1u < held.hzb_levels {
                                extent = extent * 0.5;
                                factor = factor * 2.0;
                                level = level + 1u;
                            }
                            let chosen = vec2<f32>(
                                max(floor(base.x / factor), 1.0),
                                max(floor(base.y / factor), 1.0),
                            );
                            let first_x = max(floor(low_x / factor), 0.0);
                            let last_x = min(floor(high_x / factor), chosen.x - 1.0);
                            let first_y = max(floor(low_y / factor), 0.0);
                            let last_y = min(floor(high_y / factor), chosen.y - 1.0);
                            var deepest = 0.0;
                            for (var y = u32(first_y); y < u32(last_y) + 1u; y = y + 1u) {
                                for (var x = u32(first_x); x < u32(last_x) + 1u; x = x + 1u) {
                                    let at = vec2<f32>(
                                        (f32(x) + 0.5) / chosen.x,
                                        (f32(y) + 0.5) / chosen.y,
                                    );
                                    deepest = max(
                                        deepest,
                                        textureSampleLevel(hzb, hzb_sampler, at, f32(level)).x,
                                    );
                                }
                            }
                            let near_z = held.far * (nearest - held.near)
                                / (nearest * (held.far - held.near));
                            occluded = deepest < near_z;
                        }
                    }
                }
            }
            if !occluded {
                let batch = instance.batch;
                let slot = atomicAdd(&arguments[batch * 4u + 1u], 1u);
                let at = instance.base + slot;
                drawn[at].clip0 = column(
                    held.clip0,
                    held.clip1,
                    held.clip2,
                    held.clip3,
                    instance.model0,
                );
                drawn[at].clip1 = column(
                    held.clip0,
                    held.clip1,
                    held.clip2,
                    held.clip3,
                    instance.model1,
                );
                drawn[at].clip2 = column(
                    held.clip0,
                    held.clip1,
                    held.clip2,
                    held.clip3,
                    instance.model2,
                );
                drawn[at].clip3 = column(
                    held.clip0,
                    held.clip1,
                    held.clip2,
                    held.clip3,
                    instance.model3,
                );
                drawn[at].model0 = instance.model0;
                drawn[at].model1 = instance.model1;
                drawn[at].model2 = instance.model2;
                drawn[at].model3 = instance.model3;
                drawn[at].previous0 = instance.previous0;
                drawn[at].previous1 = instance.previous1;
                drawn[at].previous2 = instance.previous2;
                drawn[at].previous3 = instance.previous3;
                drawn[at].normal0 = instance.normal0;
                drawn[at].normal1 = instance.normal1;
                drawn[at].normal2 = instance.normal2;
                drawn[at].tint = instance.tint;
                drawn[at].material = instance.material;
                drawn[at].pick = index;
                drawn[at].palette = instance.palette + held.palette_base;
                drawn[at].joints = instance.joints;
            }
        }
    }
}

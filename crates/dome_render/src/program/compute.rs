use super::*;

#[dome_gpu::kernel(workgroup_size = 64)]
mod cull {
    #[kernel]
    fn cull(
        global: Uvec3,
        view: Read<ViewRecord>,
        instances: Read<InstanceRecord>,
        drawn: ReadWrite<DrawRecord>,
        arguments: ReadWrite<AtomicU32>,
        levels: Read<HzbLevelRecord>,
        depths: Read<f32>,
    ) {
        let index = global.x;
        let held = view[0u32];
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
                let mut occluded = false;
                if held.hzb_levels > 0u32 && instance.hzb > 0u32 {
                    let relative = fvec4(
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
                        let center = fvec4(bounds.x, bounds.y, bounds.z, 1.0);
                        let clip = column(held.clip0, held.clip1, held.clip2, held.clip3, center);
                        if clip.w > 0.0 {
                            let inverse = 1.0 / clip.w;
                            let middle = fvec2(
                                clip.x * inverse * 0.5 + 0.5,
                                clip.y * inverse * held.vertical + 0.5,
                            );
                            let half_y = bounds.w / (nearest * held.tan_vertical) * 0.5;
                            let half_x = half_y / held.aspect;
                            let pad = fvec2(2.0 / held.view_width, 2.0 / held.view_height);
                            let low = fvec2(middle.x - half_x - pad.x, middle.y - half_y - pad.y);
                            let high = fvec2(middle.x + half_x + pad.x, middle.y + half_y + pad.y);
                            if low.x >= 0.0 && high.x <= 1.0 && low.y >= 0.0 && high.y <= 1.0 {
                                let base = levels[0u32];
                                let width = base.width as f32;
                                let height = base.height as f32;
                                let low_x = low.x * width;
                                let low_y = low.y * height;
                                let high_x = high.x * width;
                                let high_y = high.y * height;
                                let mut level = 0u32;
                                let mut factor = 1.0;
                                let mut extent = max(high_x - low_x, high_y - low_y);
                                while extent > 4.0 && level + 1u32 < held.hzb_levels {
                                    extent *= 0.5;
                                    factor *= 2.0;
                                    level += 1u32;
                                }
                                let chosen = levels[level];
                                let chosen_width = chosen.width as f32;
                                let chosen_height = chosen.height as f32;
                                let first_x = max(floor(low_x / factor), 0.0) as u32;
                                let last_x = min(floor(high_x / factor), chosen_width - 1.0) as u32;
                                let first_y = max(floor(low_y / factor), 0.0) as u32;
                                let last_y =
                                    min(floor(high_y / factor), chosen_height - 1.0) as u32;
                                let mut deepest = 0.0;
                                for y in stride(first_y, last_y + 1u32, 1u32) {
                                    for x in stride(first_x, last_x + 1u32, 1u32) {
                                        deepest = max(
                                            deepest,
                                            depths[chosen.offset + y * chosen.stride + x],
                                        );
                                    }
                                }
                                occluded = deepest < (nearest - held.near) / (held.far - held.near);
                            }
                        }
                    }
                }
                if !occluded {
                    let batch = instance.batch;
                    let slot = atomic_add(&arguments[batch * 4u32 + 1u32], 1u32);
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

    fn inside(plane: Fvec4, sphere: Fvec4) -> bool {
        plane.x * sphere.x + plane.y * sphere.y + plane.z * sphere.z + plane.w > -sphere.w
    }

    fn column(a: Fvec4, b: Fvec4, c: Fvec4, d: Fvec4, local: Fvec4) -> Fvec4 {
        a * local.x + b * local.y + c * local.z + d * local.w
    }
}
#[dome_gpu::kernel(workgroup_size = 64)]
mod cluster {
    #[kernel]
    fn cluster(
        group: Uvec3,
        lid: u32,
        view: Read<ViewRecord>,
        points: Read<u32>,
        clusters: ReadWrite<AtomicU32>,
    ) {
        let held = view[0u32];
        let tiles = held.cluster_x * held.cluster_y;
        if tiles == 0u32 || held.point_lights == 0u32 {
            return;
        }
        let tile = group.x;
        let slice = group.y;
        if tile >= tiles || slice >= 24u32 {
            return;
        }
        let tile_x = tile % held.cluster_x;
        let tile_y = tile / held.cluster_x;
        let index = (slice * held.cluster_y + tile_y) * held.cluster_x + tile_x;
        let cluster_base = index * 33u32;
        let low_x = tile_x as f32 * 64.0;
        let high_x = low_x + 64.0;
        let low_y = tile_y as f32 * 64.0;
        let high_y = low_y + 64.0;
        let ratio = held.far / held.near;
        let slice_near = held.near * pow(ratio, slice as f32 / 24.0);
        let slice_far = held.near * pow(ratio, (slice + 1u32) as f32 / 24.0);
        for light in stride(lid, held.point_lights, 64u32) {
            let at = light * 12u32;
            let point_x = bitcast_f32(points[at]);
            let point_y = bitcast_f32(points[at + 1u32]);
            let point_z = bitcast_f32(points[at + 2u32]);
            let point_range = bitcast_f32(points[at + 3u32]);
            let relative = fvec4(
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
                let mut covers = nearest <= held.near;
                if !covers {
                    let center = fvec4(point_x, point_y, point_z, 1.0);
                    let clip = column(held.clip0, held.clip1, held.clip2, held.clip3, center);
                    if clip.w > 0.0 {
                        let inverse = 1.0 / clip.w;
                        let middle = fvec2(
                            clip.x * inverse * 0.5 + 0.5,
                            clip.y * inverse * held.vertical + 0.5,
                        );
                        let half_y = point_range / (nearest * held.tan_vertical) * 0.5;
                        let half_x = half_y / held.aspect;
                        let pad = fvec2(1.0 / held.view_width, 1.0 / held.view_height);
                        let low = fvec2(middle.x - half_x - pad.x, middle.y - half_y - pad.y);
                        let high = fvec2(middle.x + half_x + pad.x, middle.y + half_y + pad.y);
                        covers = high.x * held.view_width >= low_x
                            && low.x * held.view_width <= high_x
                            && high.y * held.view_height >= low_y
                            && low.y * held.view_height <= high_y;
                    } else {
                        covers = true;
                    }
                }
                if covers {
                    let slot = atomic_add(&clusters[cluster_base], 1u32);
                    if slot < 32u32 {
                        atomic_store(&clusters[cluster_base + 1u32 + slot], light);
                    }
                }
            }
        }
    }

    fn column(a: Fvec4, b: Fvec4, c: Fvec4, d: Fvec4, local: Fvec4) -> Fvec4 {
        a * local.x + b * local.y + c * local.z + d * local.w
    }
}
pub(crate) fn culling() -> ComputeProgram {
    cull()
}
pub(crate) fn clustering() -> ComputeProgram {
    cluster()
}

use crate::abi::{UiInstanceRecord, UiScreenRecord};
use dome_gpu::{Fvec2, Fvec4, GraphicsProgram};

#[dome_gpu::graphics]
mod paint {
    #[binding]
    fn screen(screen: Read<UiScreenRecord>) {}

    #[binding]
    fn instances(instances: Read<UiInstanceRecord>) {}

    #[vertex]
    fn vertex(
        vertex: u32,
        instance: u32,
        position: Out<Position>,
        placed: Out<Fvec2>,
        glyph_uv: Out<Fvec2>,
        instance_index: Out<u32>,
    ) {
        let held = instances[instance];
        let setup = screen[0u32];
        let kind = held.meta.x;
        let column = select(0.0, 1.0, vertex == 1u32 || vertex == 3u32 || vertex == 4u32);
        let row = select(0.0, 1.0, vertex == 2u32 || vertex == 4u32 || vertex == 5u32);
        if is_segment(kind) {
            let ax = held.rect.x;
            let ay = held.rect.y;
            let bx = held.rect.z;
            let by = held.rect.w;
            let dx = bx - ax;
            let dy = by - ay;
            let length = max(sqrt(dx * dx + dy * dy), 0.0001);
            let nx = -(dy / length) * held.extra.x * 0.5;
            let ny = (dx / length) * held.extra.x * 0.5;
            let side = column * 2.0 - 1.0;
            placed = fvec2(
                mixed(ax, bx, row) + nx * side,
                mixed(ay, by, row) + ny * side,
            );
            glyph_uv = fvec2(row, side * 0.5 + 0.5);
        } else if is_ring(kind) {
            let extent = held.rect.z + held.rect.w;
            placed = fvec2(
                held.rect.x + (column * 2.0 - 1.0) * extent,
                held.rect.y + (row * 2.0 - 1.0) * extent,
            );
            glyph_uv = fvec2(column, row);
        } else if is_glyph(kind) {
            let center_x = held.rect.x + held.rect.z * 0.5;
            let center_y = held.rect.y + held.rect.w * 0.5;
            let local_x = (column - 0.5) * held.rect.z;
            let local_y = (row - 0.5) * held.rect.w;
            let cosine = held.extra.y;
            let sine = held.extra.z;
            placed = fvec2(
                center_x + local_x * cosine - local_y * sine,
                center_y + local_x * sine + local_y * cosine,
            );
            glyph_uv = fvec2(
                mixed(held.uv.x, held.uv.z, column),
                mixed(held.uv.y, held.uv.w, row),
            );
        } else {
            placed = fvec2(
                held.rect.x + column * held.rect.z,
                held.rect.y + row * held.rect.w,
            );
            glyph_uv = fvec2(column, row);
            if is_image(kind) {
                glyph_uv = fvec2(
                    mixed(held.uv.x, held.uv.z, column),
                    mixed(held.uv.y, held.uv.w, row),
                );
            }
        }
        let x = placed.x * setup.scale;
        let y = placed.y * setup.scale;
        position = fvec4(
            x / setup.width * 2.0 - 1.0,
            (y / setup.height * 2.0 - 1.0) * setup.vertical * 2.0,
            0.0,
            1.0,
        );
        placed = fvec2(x, y);
        instance_index = instance;
    }

    #[fragment]
    fn fragment(
        images: TextureTable,
        samplers: SamplerTable,
        pixel: In<Fvec2>,
        uv: In<Fvec2>,
        slot: In<u32>,
        target: Out<Fvec4>,
    ) {
        let held = instances[slot];
        let setup = screen[0u32];
        let kind = held.meta.x;
        let flags = held.meta.y;
        let scale = setup.scale;
        let clip_center = fvec2(
            (held.clip.x + held.clip.z * 0.5) * scale,
            (held.clip.y + held.clip.w * 0.5) * scale,
        );
        let clip_half = fvec2(held.clip.z * scale * 0.5, held.clip.w * scale * 0.5);
        let clip_corners = fvec4(
            held.clip_corners.x * scale,
            held.clip_corners.y * scale,
            held.clip_corners.z * scale,
            held.clip_corners.w * scale,
        );
        let clip_distance = squircle(
            fvec2(pixel.x - clip_center.x, pixel.y - clip_center.y),
            clip_half,
            clip_corners,
        );
        if clip_distance > 0.5 {
            discard();
        }
        let clip_mask = coverage(clip_distance);
        if is_glyph(kind) {
            let glyph = sample(images[held.meta.z], samplers[held.meta.w], uv);
            let alpha = pow(max(glyph.w, 0.0), held.extra.x);
            target = premultiplied(held.color, alpha * clip_mask);
        } else if is_segment(kind) {
            let ax = held.rect.x * scale;
            let ay = held.rect.y * scale;
            let bx = held.rect.z * scale;
            let by = held.rect.w * scale;
            let dx = bx - ax;
            let dy = by - ay;
            let length_squared = max(dx * dx + dy * dy, 0.0001);
            let along = clamped(
                ((pixel.x - ax) * dx + (pixel.y - ay) * dy) / length_squared,
                0.0,
                1.0,
            );
            let nearest_x = ax + dx * along;
            let nearest_y = ay + dy * along;
            let offset_x = pixel.x - nearest_x;
            let offset_y = pixel.y - nearest_y;
            let distance =
                sqrt(offset_x * offset_x + offset_y * offset_y) - held.extra.x * scale * 0.5;
            if distance > 0.5 {
                discard();
            }
            target = premultiplied(
                fvec4(
                    mixed(held.color.x, held.color2.x, along),
                    mixed(held.color.y, held.color2.y, along),
                    mixed(held.color.z, held.color2.z, along),
                    mixed(held.color.w, held.color2.w, along),
                ),
                coverage(distance) * clip_mask,
            );
        } else if is_ring(kind) {
            let center_x = held.rect.x * scale;
            let center_y = held.rect.y * scale;
            let offset_x = pixel.x - center_x;
            let offset_y = pixel.y - center_y;
            let distance =
                abs(sqrt(offset_x * offset_x + offset_y * offset_y) - held.rect.z * scale)
                    - held.rect.w * scale * 0.5;
            if distance > 0.5 {
                discard();
            }
            let mut phase = atan2(offset_y, offset_x) - held.extra.x;
            while phase < 0.0 {
                phase += 6.2831855;
            }
            if phase > held.extra.y {
                discard();
            }
            target = premultiplied(held.color, coverage(distance) * clip_mask);
        } else if is_triangle(kind) {
            let coverage_amount = triangle_coverage(
                fvec2(held.uv.x * scale, held.uv.y * scale),
                fvec2(held.uv.z * scale, held.uv.w * scale),
                fvec2(held.extra.x * scale, held.extra.y * scale),
                pixel,
            );
            if coverage_amount <= 0.0 {
                discard();
            }
            target = premultiplied(held.color, coverage_amount * clip_mask);
        } else {
            let center_x = (held.rect.x + held.rect.z * 0.5) * scale;
            let center_y = (held.rect.y + held.rect.w * 0.5) * scale;
            let half = fvec2(held.rect.z * scale * 0.5, held.rect.w * scale * 0.5);
            let corners = fvec4(
                held.corners.x * scale,
                held.corners.y * scale,
                held.corners.z * scale,
                held.corners.w * scale,
            );
            let distance = squircle(fvec2(pixel.x - center_x, pixel.y - center_y), half, corners);
            if distance > 0.5 {
                discard();
            }
            let mask = coverage(distance) * clip_mask;
            if is_scene(kind) {
                let sampled = sample(images[held.meta.z], samplers[held.meta.w], uv);
                target = fvec4(sampled.x * mask, sampled.y * mask, sampled.z * mask, mask);
            } else if is_image(kind) {
                let sampled = sample(images[held.meta.z], samplers[held.meta.w], uv);
                let alpha = sampled.w * held.color.w * mask;
                target = fvec4(
                    sampled.x * alpha,
                    sampled.y * alpha,
                    sampled.z * alpha,
                    alpha,
                );
            } else {
                let mut shaded = held.color;
                if has_flag(flags, 1u32) {
                    let along = clamped(
                        (pixel.y - held.rect.y * scale) / max(held.rect.w * scale, 1.0),
                        0.0,
                        1.0,
                    );
                    shaded = fvec4(
                        mixed(held.color.x, held.color2.x, along),
                        mixed(held.color.y, held.color2.y, along),
                        mixed(held.color.z, held.color2.z, along),
                        mixed(held.color.w, held.color2.w, along),
                    );
                }
                if has_flag(flags, 2u32) {
                    let width = max(held.extra.x * scale, 0.5);
                    let depth = -distance;
                    let band = smooth(width + 0.5, width - 0.5, depth) * smooth(-0.5, 0.5, depth);
                    let ink = band * held.color2.w * mask;
                    let fill = premultiplied(shaded, mask);
                    target = fill * (1.0 - ink)
                        + fvec4(
                            held.color2.x * ink,
                            held.color2.y * ink,
                            held.color2.z * ink,
                            ink,
                        );
                } else {
                    target = premultiplied(shaded, mask);
                }
            }
        }
    }

    fn is_glyph(kind: u32) -> bool {
        kind == 2u32
    }

    fn is_scene(kind: u32) -> bool {
        kind == 3u32
    }

    fn is_segment(kind: u32) -> bool {
        kind == 4u32
    }

    fn is_ring(kind: u32) -> bool {
        kind == 5u32
    }

    fn is_triangle(kind: u32) -> bool {
        kind == 6u32
    }

    fn is_image(kind: u32) -> bool {
        kind == 7u32
    }

    fn has_flag(flags: u32, flag: u32) -> bool {
        (flags & flag) != 0u32
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }

    fn mixed(from: f32, to: f32, amount: f32) -> f32 {
        from + (to - from) * amount
    }

    fn smooth(edge0: f32, edge1: f32, value: f32) -> f32 {
        let t = clamped((value - edge0) / (edge1 - edge0), 0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn coverage(distance: f32) -> f32 {
        clamped(0.5 - distance, 0.0, 1.0)
    }

    fn premultiplied(color: Fvec4, mask: f32) -> Fvec4 {
        let alpha = clamped(color.w * mask, 0.0, 1.0);
        fvec4(color.x * alpha, color.y * alpha, color.z * alpha, alpha)
    }

    fn squircle(at: Fvec2, half: Fvec2, corners: Fvec4) -> f32 {
        let mut radius = corners.x;
        if at.x > 0.0 && at.y < 0.0 {
            radius = corners.y;
        } else if at.x > 0.0 && at.y >= 0.0 {
            radius = corners.z;
        } else if at.x <= 0.0 && at.y >= 0.0 {
            radius = corners.w;
        }
        radius = min(radius, min(half.x, half.y));
        let qx = abs(at.x) - half.x + radius;
        let qy = abs(at.y) - half.y + radius;
        if qx <= 0.0 || qy <= 0.0 {
            max(qx, qy) - radius
        } else {
            let ratio = clamped(radius / max(min(half.x, half.y), 0.001), 0.0, 1.0);
            let exponent = mixed(2.0, 2.8, clamped((1.0 - ratio) * 2.0, 0.0, 1.0));
            pow(pow(qx, exponent) + pow(qy, exponent), 1.0 / exponent) - radius
        }
    }

    fn cross2(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
        ax * by - ay * bx
    }

    fn triangle_coverage(a: Fvec2, b: Fvec2, c: Fvec2, point: Fvec2) -> f32 {
        let area = cross2(b.x - a.x, b.y - a.y, c.x - a.x, c.y - a.y);
        if abs(area) < 0.00001 {
            0.0
        } else {
            let sign = select(-1.0, 1.0, area > 0.0);
            let ab = sign * cross2(b.x - a.x, b.y - a.y, point.x - a.x, point.y - a.y)
                / max(
                    sqrt((b.x - a.x) * (b.x - a.x) + (b.y - a.y) * (b.y - a.y)),
                    0.0001,
                );
            let bc = sign * cross2(c.x - b.x, c.y - b.y, point.x - b.x, point.y - b.y)
                / max(
                    sqrt((c.x - b.x) * (c.x - b.x) + (c.y - b.y) * (c.y - b.y)),
                    0.0001,
                );
            let ca = sign * cross2(a.x - c.x, a.y - c.y, point.x - c.x, point.y - c.y)
                / max(
                    sqrt((a.x - c.x) * (a.x - c.x) + (a.y - c.y) * (a.y - c.y)),
                    0.0001,
                );
            clamped(min(ab, min(bc, ca)) + 0.5, 0.0, 1.0)
        }
    }
}

pub(crate) fn program() -> GraphicsProgram {
    paint()
}

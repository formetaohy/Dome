enable wgpu_binding_array;

fn mixed(start: f32, end: f32, amount: f32) -> f32 {
    return start + (end - start) * amount;
}

struct UiScreen {
    width: f32,
    height: f32,
    scale: f32,
    vertical: f32,
};

struct UiInstance {
    rect: vec4<f32>,
    corners: vec4<f32>,
    color: vec4<f32>,
    color2: vec4<f32>,
    uv: vec4<f32>,
    clip: vec4<f32>,
    clip_corners: vec4<f32>,
    metadata: vec4<u32>,
    extra: vec4<f32>,
};

@group(0) @binding(0) var<storage, read> screen: array<UiScreen>;
@group(0) @binding(1) var<storage, read> instances: array<UiInstance>;
@group(1) @binding(0) var images: binding_array<texture_2d<f32>, TEXTURE_TABLE_EXTENT>;
@group(1) @binding(1) var samplers: binding_array<sampler, SAMPLER_TABLE_EXTENT>;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) placed: vec2<f32>,
    @location(1) glyph_uv: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
};

struct FragmentIn {
    @location(0) pixel: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) slot: u32,
};

fn is_glyph(kind: u32) -> bool {
    return kind == 2u;
}

fn is_scene(kind: u32) -> bool {
    return kind == 3u;
}

fn is_segment(kind: u32) -> bool {
    return kind == 4u;
}

fn is_ring(kind: u32) -> bool {
    return kind == 5u;
}

fn is_triangle(kind: u32) -> bool {
    return kind == 6u;
}

fn is_image(kind: u32) -> bool {
    return kind == 7u;
}

fn has_flag(flags: u32, flag: u32) -> bool {
    return (flags & flag) != 0u;
}

fn smoothed(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = clamp((value - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn coverage(distance: f32) -> f32 {
    return clamp(0.5 - distance, 0.0, 1.0);
}

fn premultiplied(color: vec4<f32>, mask: f32) -> vec4<f32> {
    let alpha = clamp(color.w * mask, 0.0, 1.0);
    return vec4<f32>(color.x * alpha, color.y * alpha, color.z * alpha, alpha);
}

fn squircle(at: vec2<f32>, half: vec2<f32>, corners: vec4<f32>) -> f32 {
    var radius = corners.x;
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
        return max(qx, qy) - radius;
    } else {
        let ratio = clamp(radius / max(min(half.x, half.y), 0.001), 0.0, 1.0);
        let exponent = mixed(2.0, 2.8, clamp((1.0 - ratio) * 2.0, 0.0, 1.0));
        return pow(pow(qx, exponent) + pow(qy, exponent), 1.0 / exponent) - radius;
    }
}

fn cross2(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    return ax * by - ay * bx;
}

fn triangle_coverage(a: vec2<f32>, b: vec2<f32>, c: vec2<f32>, point: vec2<f32>) -> f32 {
    let area = cross2(b.x - a.x, b.y - a.y, c.x - a.x, c.y - a.y);
    if abs(area) < 0.00001 {
        return 0.0;
    } else {
        let sign = select(-1.0, 1.0, area > 0.0);
        let ab = sign * cross2(b.x - a.x, b.y - a.y, point.x - a.x, point.y - a.y)
            / max(sqrt((b.x - a.x) * (b.x - a.x) + (b.y - a.y) * (b.y - a.y)), 0.0001);
        let bc = sign * cross2(c.x - b.x, c.y - b.y, point.x - b.x, point.y - b.y)
            / max(sqrt((c.x - b.x) * (c.x - b.x) + (c.y - b.y) * (c.y - b.y)), 0.0001);
        let ca = sign * cross2(a.x - c.x, a.y - c.y, point.x - c.x, point.y - c.y)
            / max(sqrt((a.x - c.x) * (a.x - c.x) + (a.y - c.y) * (a.y - c.y)), 0.0001);
        return clamp(min(ab, min(bc, ca)) + 0.5, 0.0, 1.0);
    }
}

@vertex
fn vertex(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> VertexOut {
    let held = instances[instance];
    let setup = screen[0u];
    let kind = held.metadata.x;
    let column = select(0.0, 1.0, vertex == 1u || vertex == 3u || vertex == 4u);
    let row = select(0.0, 1.0, vertex == 2u || vertex == 4u || vertex == 5u);
    var output: VertexOut;
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
        output.placed = vec2<f32>(
            mixed(ax, bx, row) + nx * side,
            mixed(ay, by, row) + ny * side,
        );
        output.glyph_uv = vec2<f32>(row, side * 0.5 + 0.5);
    } else if is_ring(kind) {
        let extent = held.rect.z + held.rect.w;
        output.placed = vec2<f32>(
            held.rect.x + (column * 2.0 - 1.0) * extent,
            held.rect.y + (row * 2.0 - 1.0) * extent,
        );
        output.glyph_uv = vec2<f32>(column, row);
    } else if is_glyph(kind) {
        let center_x = held.rect.x + held.rect.z * 0.5;
        let center_y = held.rect.y + held.rect.w * 0.5;
        let local_x = (column - 0.5) * held.rect.z;
        let local_y = (row - 0.5) * held.rect.w;
        let cosine = held.extra.y;
        let sine = held.extra.z;
        output.placed = vec2<f32>(
            center_x + local_x * cosine - local_y * sine,
            center_y + local_x * sine + local_y * cosine,
        );
        output.glyph_uv = vec2<f32>(
            mixed(held.uv.x, held.uv.z, column),
            mixed(held.uv.y, held.uv.w, row),
        );
    } else {
        output.placed = vec2<f32>(
            held.rect.x + column * held.rect.z,
            held.rect.y + row * held.rect.w,
        );
        output.glyph_uv = vec2<f32>(column, row);
        if is_image(kind) {
            output.glyph_uv = vec2<f32>(
                mixed(held.uv.x, held.uv.z, column),
                mixed(held.uv.y, held.uv.w, row),
            );
        }
    }
    let x = output.placed.x * setup.scale;
    let y = output.placed.y * setup.scale;
    output.position = vec4<f32>(
        x / setup.width * 2.0 - 1.0,
        (y / setup.height * 2.0 - 1.0) * setup.vertical * 2.0,
        0.0,
        1.0,
    );
    output.placed = vec2<f32>(x, y);
    output.instance_index = instance;
    return output;
}

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    let pixel = input.pixel;
    let uv = input.uv;
    let slot = input.slot;
    let held = instances[slot];
    let setup = screen[0u];
    let kind = held.metadata.x;
    let flags = held.metadata.y;
    let scale = setup.scale;
    let clip_center = vec2<f32>(
        (held.clip.x + held.clip.z * 0.5) * scale,
        (held.clip.y + held.clip.w * 0.5) * scale,
    );
    let clip_half = vec2<f32>(held.clip.z * scale * 0.5, held.clip.w * scale * 0.5);
    let clip_corners = vec4<f32>(
        held.clip_corners.x * scale,
        held.clip_corners.y * scale,
        held.clip_corners.z * scale,
        held.clip_corners.w * scale,
    );
    let clip_distance = squircle(
        vec2<f32>(pixel.x - clip_center.x, pixel.y - clip_center.y),
        clip_half,
        clip_corners,
    );
    if clip_distance > 0.5 {
        discard;
    }
    let clip_mask = coverage(clip_distance);
    var painted: vec4<f32>;
    if is_glyph(kind) {
        let glyph = textureSample(images[held.metadata.z], samplers[held.metadata.w], uv);
        let alpha = pow(max(glyph.x, 0.0), held.extra.x);
        painted = premultiplied(held.color, alpha * clip_mask);
    } else if is_segment(kind) {
        let ax = held.rect.x * scale;
        let ay = held.rect.y * scale;
        let bx = held.rect.z * scale;
        let by = held.rect.w * scale;
        let dx = bx - ax;
        let dy = by - ay;
        let length_squared = max(dx * dx + dy * dy, 0.0001);
        let along = clamp(
            ((pixel.x - ax) * dx + (pixel.y - ay) * dy) / length_squared,
            0.0,
            1.0,
        );
        let nearest_x = ax + dx * along;
        let nearest_y = ay + dy * along;
        let offset_x = pixel.x - nearest_x;
        let offset_y = pixel.y - nearest_y;
        let distance = sqrt(offset_x * offset_x + offset_y * offset_y) - held.extra.x * scale * 0.5;
        if distance > 0.5 {
            discard;
        }
        painted = premultiplied(
            vec4<f32>(
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
        let distance = abs(sqrt(offset_x * offset_x + offset_y * offset_y) - held.rect.z * scale)
            - held.rect.w * scale * 0.5;
        if distance > 0.5 {
            discard;
        }
        var phase = atan2(offset_y, offset_x) - held.extra.x;
        while phase < 0.0 {
            phase += 6.2831855;
        }
        if phase > held.extra.y {
            discard;
        }
        painted = premultiplied(held.color, coverage(distance) * clip_mask);
    } else if is_triangle(kind) {
        let coverage_amount = triangle_coverage(
            vec2<f32>(held.uv.x * scale, held.uv.y * scale),
            vec2<f32>(held.uv.z * scale, held.uv.w * scale),
            vec2<f32>(held.extra.x * scale, held.extra.y * scale),
            pixel,
        );
        if coverage_amount <= 0.0 {
            discard;
        }
        painted = premultiplied(held.color, coverage_amount * clip_mask);
    } else {
        let center_x = (held.rect.x + held.rect.z * 0.5) * scale;
        let center_y = (held.rect.y + held.rect.w * 0.5) * scale;
        let half = vec2<f32>(held.rect.z * scale * 0.5, held.rect.w * scale * 0.5);
        let corners = vec4<f32>(
            held.corners.x * scale,
            held.corners.y * scale,
            held.corners.z * scale,
            held.corners.w * scale,
        );
        let distance = squircle(
            vec2<f32>(pixel.x - center_x, pixel.y - center_y),
            half,
            corners,
        );
        if distance > 0.5 {
            discard;
        }
        let mask = coverage(distance) * clip_mask;
        if is_scene(kind) {
            let sampled = textureSample(images[held.metadata.z], samplers[held.metadata.w], uv);
            painted = vec4<f32>(sampled.x * mask, sampled.y * mask, sampled.z * mask, mask);
        } else if is_image(kind) {
            let sampled = textureSample(images[held.metadata.z], samplers[held.metadata.w], uv);
            let alpha = sampled.w * held.color.w * mask;
            painted = vec4<f32>(
                sampled.x * alpha,
                sampled.y * alpha,
                sampled.z * alpha,
                alpha,
            );
        } else {
            var shaded = held.color;
            if has_flag(flags, 1u) {
                let along = clamp(
                    (pixel.y - held.rect.y * scale) / max(held.rect.w * scale, 1.0),
                    0.0,
                    1.0,
                );
                shaded = vec4<f32>(
                    mixed(held.color.x, held.color2.x, along),
                    mixed(held.color.y, held.color2.y, along),
                    mixed(held.color.z, held.color2.z, along),
                    mixed(held.color.w, held.color2.w, along),
                );
            }
            if has_flag(flags, 2u) {
                let width = max(held.extra.x * scale, 0.5);
                let depth = -distance;
                let band = smoothed(width + 0.5, width - 0.5, depth) * smoothed(-0.5, 0.5, depth);
                let ink = band * held.color2.w * mask;
                let fill = premultiplied(shaded, mask);
                painted = fill * (1.0 - ink)
                    + vec4<f32>(
                        held.color2.x * ink,
                        held.color2.y * ink,
                        held.color2.z * ink,
                        ink,
                    );
            } else {
                painted = premultiplied(shaded, mask);
            }
        }
    }
    return painted;
}


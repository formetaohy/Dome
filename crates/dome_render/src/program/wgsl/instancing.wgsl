@group(0) @binding(0) var<storage, read> light: array<Light>;
@group(0) @binding(1) var<storage, read> cascades: array<Cascade>;
@group(0) @binding(2) var<storage, read> drawn: array<Draw>;
@group(0) @binding(3) var<storage, read> materials: array<Material>;
@group(0) @binding(4) var<storage, read> view: array<View>;
@group(0) @binding(5) var<storage, read> palette: array<Joint>;
@group(0) @binding(6) var<storage, read> skins: array<SkinVertex>;
@group(0) @binding(7) var<storage, read_write> keys: array<atomic<u32>>;
@group(0) @binding(8) var<storage, read_write> colors: array<u32>;
@group(0) @binding(9) var<storage, read_write> dropped: array<atomic<u32>>;
@group(0) @binding(10) var<storage, read> settings: array<TransparentSetup>;
@group(0) @binding(11) var<storage, read> probes: array<Probe>;
@group(0) @binding(12) var<storage, read> vertices: array<f32>;
@group(0) @binding(13) var<storage, read> lights: array<u32>;
@group(0) @binding(14) var<storage, read> point_light: array<u32>;
@group(1) @binding(0) var textures: binding_array<texture_2d<f32>, TEXTURE_TABLE_EXTENT>;
@group(1) @binding(1) var samplers: binding_array<sampler, SAMPLER_TABLE_EXTENT>;
@group(2) @binding(0) var shadows: texture_depth_2d;
@group(2) @binding(1) var shadower: sampler_comparison;
@group(2) @binding(2) var radiance: texture_2d<f32>;
@group(2) @binding(3) var radiance_sampler: sampler;
@group(2) @binding(4) var brdf: texture_2d<f32>;
@group(2) @binding(5) var brdf_sampler: sampler;
@group(2) @binding(6) var point_shadows: texture_depth_2d;
@group(2) @binding(7) var point_shadower: sampler_comparison;
@group(2) @binding(8) var probe_textures: binding_array<texture_2d<f32>, TEXTURE_TABLE_EXTENT>;
@group(2) @binding(9) var probe_samplers: binding_array<sampler, SAMPLER_TABLE_EXTENT>;

fn clamped(value: f32, low: f32, high: f32) -> f32 {
    return min(max(value, low), high);
}

fn dot3(left: vec4<f32>, right: vec4<f32>) -> f32 {
    return left.x * right.x + left.y * right.y + left.z * right.z;
}

fn cross3(left: vec4<f32>, right: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        left.y * right.z - left.z * right.y,
        left.z * right.x - left.x * right.z,
        left.x * right.y - left.y * right.x,
        0.0,
    );
}

fn normalized(vector: vec4<f32>) -> vec4<f32> {
    let inverse =
        1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
    return vector * inverse;
}

fn deformed(col0: vec4<f32>, col1: vec4<f32>, col2: vec4<f32>, col3: vec4<f32>, point: vec4<f32>) -> vec4<f32> {
    return col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w;
}

fn turned(first: vec4<f32>, second: vec4<f32>, third: vec4<f32>, vector: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        first.x * vector.x + second.x * vector.y + third.x * vector.z,
        first.y * vector.x + second.y * vector.y + third.y * vector.z,
        first.z * vector.x + second.z * vector.y + third.z * vector.z,
        0.0,
    );
}

fn irradiance_low(c0: vec4<f32>, c1: vec4<f32>, c2: vec4<f32>, c3: vec4<f32>, normal: vec4<f32>) -> vec4<f32> {
    return c0 * 0.282095
        + c1 * (0.488603 * normal.y)
        + c2 * (0.488603 * normal.z)
        + c3 * (0.488603 * normal.x);
}

fn irradiance_high(
    c4: vec4<f32>,
    c5: vec4<f32>,
    c6: vec4<f32>,
    c7: vec4<f32>,
    c8: vec4<f32>,
    normal: vec4<f32>,
) -> vec4<f32> {
    let x = normal.x;
    let y = normal.y;
    let z = normal.z;
    return c4 * (1.092548 * x * y)
        + c5 * (1.092548 * y * z)
        + c6 * (0.315392 * (3.0 * z * z - 1.0))
        + c7 * (1.092548 * x * z)
        + c8 * (0.546274 * (x * x - y * y));
}

fn split_sum(f0: f32, split: vec4<f32>) -> f32 {
    let first = f0 * split.x + split.y;
    let energy = 1.0 - (split.x + split.y);
    let average = f0 + (1.0 - f0) * 0.047619;
    return first + first * energy * average / (1.0 - energy * average);
}

fn pbr(
    normal: vec4<f32>,
    view: vec4<f32>,
    light: vec4<f32>,
    base: vec4<f32>,
    metallic: f32,
    roughness: f32,
) -> vec4<f32> {
    let pi = 355.0 / 113.0;
    let n_dot_v = max(dot3(normal, view), 0.0001);
    let n_dot_l = max(dot3(normal, light), 0.0);
    let albedo = vec4<f32>(
        base.x * (1.0 - metallic),
        base.y * (1.0 - metallic),
        base.z * (1.0 - metallic),
        1.0,
    );
    let f0 = vec4<f32>(
        0.04 + (base.x - 0.04) * metallic,
        0.04 + (base.y - 0.04) * metallic,
        0.04 + (base.z - 0.04) * metallic,
        1.0,
    );
    let half = normalized(vec4<f32>(
        view.x + light.x,
        view.y + light.y,
        view.z + light.z,
        0.0,
    ));
    let n_dot_h = max(dot3(normal, half), 0.0);
    let v_dot_h = max(dot3(view, half), 0.0);
    let alpha = max(roughness * roughness, 0.0025);
    let alpha2 = alpha * alpha;
    let denominator = n_dot_h * n_dot_h * (alpha2 - 1.0) + 1.0;
    let distribution = alpha2 / (pi * denominator * denominator);
    let k = (roughness + 1.0) * (roughness + 1.0) * 0.125;
    let geometry =
        n_dot_v * n_dot_l / ((n_dot_v * (1.0 - k) + k) * (n_dot_l * (1.0 - k) + k) + 0.0001);
    let fresnel_power = pow(1.0 - v_dot_h, 5.0);
    let fresnel = vec4<f32>(
        f0.x + (1.0 - f0.x) * fresnel_power,
        f0.y + (1.0 - f0.y) * fresnel_power,
        f0.z + (1.0 - f0.z) * fresnel_power,
        0.0,
    );
    let specular = vec4<f32>(
        distribution * geometry * fresnel.x / (4.0 * n_dot_v * n_dot_l + 0.0001),
        distribution * geometry * fresnel.y / (4.0 * n_dot_v * n_dot_l + 0.0001),
        distribution * geometry * fresnel.z / (4.0 * n_dot_v * n_dot_l + 0.0001),
        0.0,
    );
    let diffuse = vec4<f32>(
        (1.0 - fresnel.x) * albedo.x / pi,
        (1.0 - fresnel.y) * albedo.y / pi,
        (1.0 - fresnel.z) * albedo.z / pi,
        0.0,
    );
    return vec4<f32>(
        diffuse.x + specular.x,
        diffuse.y + specular.y,
        diffuse.z + specular.z,
        0.0,
    );
}

fn equirect(direction: vec4<f32>) -> vec2<f32> {
    let unit = normalized(direction);
    return vec2<f32>(
        atan2(unit.x, -unit.z) * (113.0 / 710.0) + 0.5,
        acos(clamped(unit.y, -1.0, 1.0)) * (113.0 / 355.0),
    );
}

fn encoded_fixed(value: f32, ceiling: f32) -> u32 {
    return u32(min(max(floor(value + 0.5), 0.0), ceiling));
}

fn encoded_byte(value: f32) -> u32 {
    return encoded_fixed(value, 255.0);
}

fn encoded_color(red: f32, green: f32, blue: f32) -> u32 {
    let peak = max(max(red, green), blue);
    let exponent = min(max((bitcast<u32>(peak) >> 23u) & 255u, 8u), 253u) + 2u;
    let unit = 255.0 / bitcast<f32>((exponent - 1u) << 23u);
    return (exponent << 24u)
        | (encoded_byte(red * unit) << 16u)
        | (encoded_byte(green * unit) << 8u)
        | encoded_byte(blue * unit);
}

fn encoded_depth(reach: f32, near: f32, far: f32) -> u32 {
    let normalized = min(max((reach - near) / (far - near), 0.0), 1.0);
    return encoded_fixed(normalized * 4194303.0, 4194303.0) << 8u;
}

fn encoded_coverage(coverage: f32) -> u32 {
    return encoded_byte(coverage * 255.0);
}

fn optical(climb: f32, low: f32, span: f32, scale: f32) -> f32 {
    let flat = span * exp(0.0 - low / scale);
    let slanted =
        (span * scale / climb) * (exp(0.0 - low / scale) - exp(0.0 - (low + climb) / scale));
    return select(slanted, flat, abs(climb) < 0.001);
}

fn length3(value: vec4<f32>) -> f32 {
    return sqrt(dot3(value, value));
}

fn box_projected(position: vec4<f32>, direction: vec4<f32>, extent: vec4<f32>) -> vec4<f32> {
    var nearest = 1.0e30;
    var hit = true;
    if abs(direction.x) > 0.00001 {
        let bound = select(extent.x, -extent.x, direction.x < 0.0);
        let travel = (bound - position.x) / direction.x;
        if travel > 0.0 {
            nearest = min(nearest, travel);
        }
    } else if abs(position.x) > extent.x {
        hit = false;
    }
    if abs(direction.y) > 0.00001 {
        let bound = select(extent.y, -extent.y, direction.y < 0.0);
        let travel = (bound - position.y) / direction.y;
        if travel > 0.0 {
            nearest = min(nearest, travel);
        }
    } else if abs(position.y) > extent.y {
        hit = false;
    }
    if abs(direction.z) > 0.00001 {
        let bound = select(extent.z, -extent.z, direction.z < 0.0);
        let travel = (bound - position.z) / direction.z;
        if travel > 0.0 {
            nearest = min(nearest, travel);
        }
    } else if abs(position.z) > extent.z {
        hit = false;
    }
    if hit && nearest < 1.0e29 {
        return normalized(vec4<f32>(
            position.x + direction.x * nearest,
            position.y + direction.y * nearest,
            position.z + direction.z * nearest,
            0.0,
        ));
    }
    return direction;
}

fn compared(shadows: texture_depth_2d, shadower: sampler_comparison, at: vec2<f32>, depth: f32) -> f32 {
    return textureSampleCompare(shadows, shadower, at, depth);
}

fn shadowed(
    shadows: texture_depth_2d,
    shadower: sampler_comparison,
    at: vec2<f32>,
    depth: f32,
    texel: f32,
    low: vec2<f32>,
    high: vec2<f32>,
) -> f32 {
    let left = clamped(at.x - texel, low.x, high.x);
    let right = clamped(at.x + texel, low.x, high.x);
    let down = clamped(at.y - texel, low.y, high.y);
    let up = clamped(at.y + texel, low.y, high.y);
    let x = clamped(at.x, low.x, high.x);
    let y = clamped(at.y, low.y, high.y);
    let total = compared(shadows, shadower, vec2<f32>(left, down), depth)
        + compared(shadows, shadower, vec2<f32>(x, down), depth)
        + compared(shadows, shadower, vec2<f32>(right, down), depth)
        + compared(shadows, shadower, vec2<f32>(left, y), depth)
        + compared(shadows, shadower, vec2<f32>(x, y), depth)
        + compared(shadows, shadower, vec2<f32>(right, y), depth)
        + compared(shadows, shadower, vec2<f32>(left, up), depth)
        + compared(shadows, shadower, vec2<f32>(x, up), depth)
        + compared(shadows, shadower, vec2<f32>(right, up), depth);
    return total * (1.0 / 9.0);
}

fn point_compared(
    atlas: texture_depth_2d,
    atlas_sampler: sampler_comparison,
    at: vec2<f32>,
    depth: f32,
) -> f32 {
    return textureSampleCompare(atlas, atlas_sampler, at, depth);
}

fn point_visible(
    atlas: texture_depth_2d,
    atlas_sampler: sampler_comparison,
    fragment: vec4<f32>,
    center: vec4<f32>,
    shadow: vec4<f32>,
    slope: f32,
) -> f32 {
    let origin = shadow.x;
    let scale = shadow.y;
    let near = shadow.z;
    let far = shadow.w;
    let dx = fragment.x - center.x;
    let dy = fragment.y - center.y;
    let dz = fragment.z - center.z;
    let across = abs(dx);
    let rising = abs(dy);
    let forward = abs(dz);
    var index = 0.0;
    var horizontal = -dz;
    var sideways = -dy;
    var major = across;
    if across >= rising && across >= forward {
        if dx < 0.0 {
            index = 1.0;
            horizontal = dz;
        }
    } else if rising >= forward {
        major = rising;
        index = 2.0;
        horizontal = dx;
        sideways = dz;
        if dy < 0.0 {
            index = 3.0;
            sideways = -dz;
        }
    } else {
        major = forward;
        index = 4.0;
        horizontal = dx;
        sideways = -dy;
        if dz < 0.0 {
            index = 5.0;
            horizontal = -dx;
        }
    }
    let span = max(major, 0.000001);
    let margin = 1.0 / 512.0;
    let left = clamped(0.5 + 0.5 * horizontal / span, margin, 1.0 - margin);
    let top = clamped(0.5 - 0.5 * sideways / span, margin, 1.0 - margin);
    let at = vec2<f32>((index + left) * (1.0 / 6.0), origin + top * scale);
    let texel = vec2<f32>((1.0 / 6.0) / 256.0, scale / 256.0);
    let low = vec2<f32>(index * (1.0 / 6.0) + texel.x * 0.5, origin + texel.y * 0.5);
    let high = vec2<f32>(
        (index + 1.0) * (1.0 / 6.0) - texel.x * 0.5,
        origin + scale - texel.y * 0.5,
    );
    let bias = span * (2.0 / 256.0) * (1.5 + 2.0 * slope);
    let reach = max(span - bias, near * 0.5);
    let depth = (far * near / reach - far) / (near - far);
    let left_x = clamped(at.x - texel.x, low.x, high.x);
    let right_x = clamped(at.x + texel.x, low.x, high.x);
    let down_y = clamped(at.y - texel.y, low.y, high.y);
    let up_y = clamped(at.y + texel.y, low.y, high.y);
    let total = point_compared(atlas, atlas_sampler, vec2<f32>(left_x, down_y), depth)
        + point_compared(atlas, atlas_sampler, vec2<f32>(at.x, down_y), depth)
        + point_compared(atlas, atlas_sampler, vec2<f32>(right_x, down_y), depth)
        + point_compared(atlas, atlas_sampler, vec2<f32>(left_x, at.y), depth)
        + point_compared(atlas, atlas_sampler, vec2<f32>(at.x, at.y), depth)
        + point_compared(atlas, atlas_sampler, vec2<f32>(right_x, at.y), depth)
        + point_compared(atlas, atlas_sampler, vec2<f32>(left_x, up_y), depth)
        + point_compared(atlas, atlas_sampler, vec2<f32>(at.x, up_y), depth)
        + point_compared(atlas, atlas_sampler, vec2<f32>(right_x, up_y), depth);
    return total * (1.0 / 9.0);
}

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) placed: vec4<f32>,
    @location(1) oriented: vec4<f32>,
    @location(2) tinted: vec4<f32>,
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
    let resting = held.normal0 * vertices[base + 3u]
        + held.normal1 * vertices[base + 4u]
        + held.normal2 * vertices[base + 5u];
    let material = f32(held.material);
    var out: VertexOut;
    out.tinted = vec4<f32>(
        held.tint.x,
        held.tint.y,
        held.tint.z,
        select(material, -(material + 1.0), held.pick == camera.highlight),
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
        let normal = deformed(first.col0, first.col1, first.col2, first.col3, resting)
            * skin.weights.x
            + deformed(second.col0, second.col1, second.col2, second.col3, resting)
                * skin.weights.y
            + deformed(third.col0, third.col1, third.col2, third.col3, resting)
                * skin.weights.z
            + deformed(fourth.col0, fourth.col1, fourth.col2, fourth.col3, resting)
                * skin.weights.w;
        let local = vec4<f32>(place.x, place.y, place.z, 1.0);
        let moved = held.model0 * local.x
            + held.model1 * local.y
            + held.model2 * local.z
            + held.model3 * local.w;
        let projected = held.clip0 * local.x
            + held.clip1 * local.y
            + held.clip2 * local.z
            + held.clip3 * local.w;
        out.position = projected;
        out.placed = vec4<f32>(moved.x, moved.y, moved.z, vertices[base + 6u]);
        out.oriented = vec4<f32>(normal.x, normal.y, normal.z, vertices[base + 7u]);
    } else {
        let moved = held.model0 * rest.x
            + held.model1 * rest.y
            + held.model2 * rest.z
            + held.model3 * rest.w;
        let projected = held.clip0 * rest.x
            + held.clip1 * rest.y
            + held.clip2 * rest.z
            + held.clip3 * rest.w;
        out.position = projected;
        out.placed = vec4<f32>(moved.x, moved.y, moved.z, vertices[base + 6u]);
        out.oriented = vec4<f32>(resting.x, resting.y, resting.z, vertices[base + 7u]);
    }
    return out;
}

struct FragmentIn {
    @location(0) world: vec4<f32>,
    @location(1) surface: vec4<f32>,
    @location(2) shaded: vec4<f32>,
};

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    let position = input.world;
    let direction = input.surface;
    let tint = input.shaded;
    let stored = tint.w;
    let marked = stored < 0.0;
    let material = u32(abs(stored) + 0.5) - select(0u, 1u, marked);
    let camera = view[0u];
    let projected = camera.clip0 * position.x
        + camera.clip1 * position.y
        + camera.clip2 * position.z
        + camera.clip3;
    let screen = vec2<f32>(
        projected.x / projected.w * 0.5 + 0.5,
        projected.y / projected.w * camera.vertical + 0.5,
    );
    let reach = (position.x - camera.eye.x) * camera.forward.x
        + (position.y - camera.eye.y) * camera.forward.y
        + (position.z - camera.eye.z) * camera.forward.z;
    let entry = materials[material];
    let lamp = light[0u];
    let unit = normalized(direction);
    let eye_direction = normalized(vec4<f32>(
        camera.eye.x - position.x,
        camera.eye.y - position.y,
        camera.eye.z - position.z,
        0.0,
    ));
    let uv = vec2<f32>(position.w, direction.w);
    let sampled = textureSample(
        textures[entry.base_color_image],
        samplers[entry.base_color_sampler],
        uv,
    );
    let base = vec4<f32>(
        sampled.x * tint.x * entry.base_color.x,
        sampled.y * tint.y * entry.base_color.y,
        sampled.z * tint.z * entry.base_color.z,
        sampled.w * entry.base_color.w,
    );
    let coverage = clamped(base.w, 0.0, 1.0);
    let properties = textureSample(
        textures[entry.metallic_roughness_image],
        samplers[entry.metallic_roughness_sampler],
        uv,
    );
    let metallic = clamped(entry.metallic * properties.z, 0.0, 1.0);
    let roughness = clamped(entry.roughness * properties.y, 0.045, 1.0);
    let position_dx = dpdx(position);
    let position_dy = dpdy(position);
    let surface_dx = dpdx(direction);
    let surface_dy = dpdy(direction);
    let uv_dx = vec2<f32>(position_dx.w, surface_dx.w);
    let uv_dy = vec2<f32>(position_dy.w, surface_dy.w);
    let perpendicular_y = cross3(
        vec4<f32>(position_dy.x, position_dy.y, position_dy.z, 0.0),
        unit,
    );
    let perpendicular_x = cross3(
        unit,
        vec4<f32>(position_dx.x, position_dx.y, position_dx.z, 0.0),
    );
    let tangent = normalized(perpendicular_y * uv_dx.x + perpendicular_x * uv_dy.x);
    let bitangent = normalized(perpendicular_y * uv_dx.y + perpendicular_x * uv_dy.y);
    let mapped = textureSample(
        textures[entry.normal_image],
        samplers[entry.normal_sampler],
        uv,
    );
    let scaled = vec4<f32>(
        (mapped.x * 2.0 - 1.0) * entry.normal_scale,
        (mapped.y * 2.0 - 1.0) * entry.normal_scale,
        mapped.z * 2.0 - 1.0,
        0.0,
    );
    let normal = normalized(tangent * scaled.x + bitangent * scaled.y + unit * scaled.z);
    if entry.alpha_mode == 1u && coverage < entry.alpha_cutoff {
        discard;
    }
    let albedo = vec4<f32>(
        base.x * (1.0 - metallic),
        base.y * (1.0 - metallic),
        base.z * (1.0 - metallic),
        1.0,
    );
    let f0 = vec4<f32>(
        0.04 + (base.x - 0.04) * metallic,
        0.04 + (base.y - 0.04) * metallic,
        0.04 + (base.z - 0.04) * metallic,
        1.0,
    );
    let travel = lamp.direction;
    let sun_direction = vec4<f32>(-travel.x, -travel.y, -travel.z, 0.0);
    let incidence = max(dot3(normal, sun_direction), 0.0);
    var cascade = 0u;
    if reach > cascades[0u].split {
        cascade = 1u;
    }
    if reach > cascades[1u].split {
        cascade = 2u;
    }
    if reach > cascades[2u].split {
        cascade = 3u;
    }
    let shadow = cascades[cascade];
    let offset = shadow.texel * 2.0;
    let offset_position = vec4<f32>(
        position.x + unit.x * offset,
        position.y + unit.y * offset,
        position.z + unit.z * offset,
        1.0,
    );
    let placed = shadow.clip0 * offset_position.x
        + shadow.clip1 * offset_position.y
        + shadow.clip2 * offset_position.z
        + shadow.clip3 * offset_position.w;
    let inverse = 1.0 / placed.w;
    let local = vec2<f32>(
        placed.x * inverse * 0.5 + 0.5,
        placed.y * inverse * lamp.vertical + 0.5,
    );
    let atlas = shadow.atlas;
    let low = vec2<f32>(
        atlas.x + shadow.atlas_texel * 0.5,
        atlas.y + shadow.atlas_texel * 0.5,
    );
    let high = vec2<f32>(
        atlas.x + atlas.z - shadow.atlas_texel * 0.5,
        atlas.y + atlas.z - shadow.atlas_texel * 0.5,
    );
    let at = vec2<f32>(atlas.x + local.x * atlas.z, atlas.y + local.y * atlas.z);
    let slope = sqrt(max(0.0, 1.0 - incidence * incidence)) / max(incidence, 0.1);
    let depth = placed.z * inverse - 0.0005 - shadow.texel / shadow.depth_span * slope;
    let sampled_shadow = shadowed(shadows, shadower, at, depth, shadow.atlas_texel, low, high);
    let outside = local.x < 0.0 || local.x > 1.0 || local.y < 0.0 || local.y > 1.0;
    let beyond = reach > cascades[3u].split;
    let visibility = select(sampled_shadow, 1.0, beyond || outside);
    let direct = pbr(
        normal,
        eye_direction,
        sun_direction,
        base,
        metallic,
        roughness,
    );
    let sun = vec4<f32>(
        direct.x * lamp.radiance.x * incidence * visibility,
        direct.y * lamp.radiance.y * incidence * visibility,
        direct.z * lamp.radiance.z * incidence * visibility,
        0.0,
    );
    let occlusion = textureSample(
        textures[entry.occlusion_image],
        samplers[entry.occlusion_sampler],
        uv,
    );
    let openness = 1.0 + entry.occlusion_strength * (occlusion.x - 1.0);
    let view_dot = dot3(normal, eye_direction);
    let reflected = normalized(vec4<f32>(
        normal.x * 2.0 * view_dot - eye_direction.x,
        normal.y * 2.0 * view_dot - eye_direction.y,
        normal.z * 2.0 * view_dot - eye_direction.z,
        0.0,
    ));
    let environment_normal = turned(
        lamp.environment0,
        lamp.environment1,
        lamp.environment2,
        normal,
    );
    let environment_reflected = turned(
        lamp.environment0,
        lamp.environment1,
        lamp.environment2,
        reflected,
    );
    let prefiltered = textureSampleLevel(
        radiance,
        radiance_sampler,
        equirect(environment_reflected),
        roughness * lamp.levels,
    );
    let split = textureSample(brdf, brdf_sampler, vec2<f32>(max(view_dot, 0.0001), roughness));
    let environment_specular = vec4<f32>(
        prefiltered.x * split_sum(f0.x, split),
        prefiltered.y * split_sum(f0.y, split),
        prefiltered.z * split_sum(f0.z, split),
        0.0,
    );
    let environment_diffuse = (irradiance_low(
        lamp.irradiance0,
        lamp.irradiance1,
        lamp.irradiance2,
        lamp.irradiance3,
        environment_normal,
    ) + irradiance_high(
        lamp.irradiance4,
        lamp.irradiance5,
        lamp.irradiance6,
        lamp.irradiance7,
        lamp.irradiance8,
        environment_normal,
    )) * albedo
        * (113.0 / 355.0);
    let global_ambient =
        (environment_diffuse + environment_specular) * (lamp.intensity * openness);
    var local_red = 0.0;
    var local_green = 0.0;
    var local_blue = 0.0;
    var local_weight = 0.0;
    var local_coverage = 0.0;
    for (var index = 0u; index < lamp.probes; index = index + 1u) {
        let held = probes[index];
        let relative = vec4<f32>(
            position.x - held.center.x,
            position.y - held.center.y,
            position.z - held.center.z,
            0.0,
        );
        let local = turned(held.inverse0, held.inverse1, held.inverse2, relative);
        let outside_x = max(abs(local.x) - held.extent.x, 0.0);
        let outside_y = max(abs(local.y) - held.extent.y, 0.0);
        let outside_z = max(abs(local.z) - held.extent.z, 0.0);
        let outside = max(outside_x, max(outside_y, outside_z));
        let fade = clamped(1.0 - outside / max(held.center.w, 0.00001), 0.0, 1.0);
        if fade > 0.0 {
            let local_normal =
                normalized(turned(held.inverse0, held.inverse1, held.inverse2, normal));
            let local_reflected = normalized(turned(
                held.inverse0,
                held.inverse1,
                held.inverse2,
                reflected,
            ));
            let corrected = box_projected(local, local_reflected, held.extent);
            let held_radiance = textureSampleLevel(
                probe_textures[held.texture],
                probe_samplers[held.sampler],
                equirect(corrected),
                roughness * held.levels,
            );
            let held_diffuse = (irradiance_low(
                held.irradiance0,
                held.irradiance1,
                held.irradiance2,
                held.irradiance3,
                local_normal,
            ) + irradiance_high(
                held.irradiance4,
                held.irradiance5,
                held.irradiance6,
                held.irradiance7,
                held.irradiance8,
                local_normal,
            )) * albedo
                * (113.0 / 355.0);
            let held_specular = vec4<f32>(
                held_radiance.x * split_sum(f0.x, split),
                held_radiance.y * split_sum(f0.y, split),
                held_radiance.z * split_sum(f0.z, split),
                0.0,
            );
            let rank = 1.0 / max(held.extent.x, max(held.extent.y, held.extent.z));
            let scale = held.extent.w * openness * fade * rank;
            local_red += (held_diffuse.x + held_specular.x) * scale;
            local_green += (held_diffuse.y + held_specular.y) * scale;
            local_blue += (held_diffuse.z + held_specular.z) * scale;
            local_weight += fade * rank;
            local_coverage = max(local_coverage, fade);
        }
    }
    let local_scale = local_coverage / max(local_weight, 0.00001);
    let global_scale = max(1.0 - local_coverage, 0.0);
    let ambient = vec4<f32>(
        global_ambient.x * global_scale + local_red * local_scale,
        global_ambient.y * global_scale + local_green * local_scale,
        global_ambient.z * global_scale + local_blue * local_scale,
        0.0,
    );
    let glow = textureSample(
        textures[entry.emissive_image],
        samplers[entry.emissive_sampler],
        uv,
    );
    let emissive = vec4<f32>(
        glow.x * entry.emissive.x,
        glow.y * entry.emissive.y,
        glow.z * entry.emissive.z,
        0.0,
    );
    var emitted_x = 0.0;
    var emitted_y = 0.0;
    var emitted_z = 0.0;
    if camera.point_lights > 0u
        && camera.cluster_x > 0u
        && camera.cluster_y > 0u
        && camera.far > camera.near
    {
        let tile_x = u32(min(
            floor(screen.x * camera.view_width / 64.0),
            f32(camera.cluster_x - 1u),
        ));
        let tile_y = u32(min(
            floor(screen.y * camera.view_height / 64.0),
            f32(camera.cluster_y - 1u),
        ));
        let ratio = log(max(reach, camera.near) / camera.near) / log(camera.far / camera.near);
        var slice = u32(floor(ratio * 24.0));
        if slice >= 24u {
            slice = 23u;
        }
        let clustered = (slice * camera.cluster_y + tile_y) * camera.cluster_x + tile_x;
        let cluster_base = camera.cluster_offset + clustered * 33u;
        var count = lights[cluster_base];
        if count > 32u {
            count = 32u;
        }
        for (var slot = 0u; slot < count; slot = slot + 1u) {
            let light = lights[cluster_base + 1u + slot];
            let at = light * 12u;
            let point_x = bitcast<f32>(point_light[at]);
            let point_y = bitcast<f32>(point_light[at + 1u]);
            let point_z = bitcast<f32>(point_light[at + 2u]);
            let point_range = bitcast<f32>(point_light[at + 3u]);
            let radiance = vec4<f32>(
                bitcast<f32>(point_light[at + 4u]),
                bitcast<f32>(point_light[at + 5u]),
                bitcast<f32>(point_light[at + 6u]),
                0.0,
            );
            let shadow = vec4<f32>(
                bitcast<f32>(point_light[at + 8u]),
                bitcast<f32>(point_light[at + 9u]),
                bitcast<f32>(point_light[at + 10u]),
                bitcast<f32>(point_light[at + 11u]),
            );
            let delta = vec4<f32>(
                point_x - position.x,
                point_y - position.y,
                point_z - position.z,
                0.0,
            );
            let distance2 = dot3(delta, delta);
            let distance = sqrt(distance2);
            let direction = delta * (1.0 / max(distance, 0.0001));
            let n_dot_l = max(dot3(normal, direction), 0.0);
            let falloff = max(0.0, 1.0 - pow(distance / point_range, 4.0));
            let attenuation = falloff * falloff / max(distance2, 0.0001);
            let slope = sqrt(max(0.0, 1.0 - n_dot_l * n_dot_l)) / max(n_dot_l, 0.2);
            let visible = point_visible(
                point_shadows,
                point_shadower,
                position,
                vec4<f32>(point_x, point_y, point_z, 0.0),
                shadow,
                slope,
            );
            let term = pbr(normal, eye_direction, direction, base, metallic, roughness);
            let shaded = n_dot_l * attenuation * visible;
            emitted_x += term.x * radiance.x * shaded;
            emitted_y += term.y * radiance.y * shaded;
            emitted_z += term.z * radiance.z * shaded;
        }
    }
    let lit = vec4<f32>(
        sun.x + emitted_x + ambient.x + emissive.x,
        sun.y + emitted_y + ambient.y + emissive.y,
        sun.z + emitted_z + ambient.z + emissive.z,
        coverage,
    );
    let fog_travel = vec4<f32>(
        position.x - camera.eye.x,
        position.y - camera.eye.y,
        position.z - camera.eye.z,
        0.0,
    );
    let span_km = length3(fog_travel) * 0.001;
    let floor_km = max(min(camera.eye.y, position.y), 0.0) * 0.001;
    let climb_km = (position.y - camera.eye.y) * 0.001;
    let thin_depth = optical(climb_km, floor_km, span_km, 8.0);
    let haze_depth = optical(climb_km, floor_km, span_km, 1.2);
    let fog_extinction = vec4<f32>(
        lamp.rayleigh.x * thin_depth + (lamp.mie.x + lamp.absorption.x) * haze_depth,
        lamp.rayleigh.y * thin_depth + (lamp.mie.y + lamp.absorption.y) * haze_depth,
        lamp.rayleigh.z * thin_depth + (lamp.mie.z + lamp.absorption.z) * haze_depth,
        0.0,
    );
    let fog_through = vec4<f32>(
        exp(0.0 - fog_extinction.x),
        exp(0.0 - fog_extinction.y),
        exp(0.0 - fog_extinction.z),
        0.0,
    );
    let fog_toward = turned(
        lamp.environment0,
        lamp.environment1,
        lamp.environment2,
        vec4<f32>(
            0.0 - eye_direction.x,
            0.0 - eye_direction.y,
            0.0 - eye_direction.z,
            0.0,
        ),
    );
    let fog_sky = textureSampleLevel(
        radiance,
        radiance_sampler,
        equirect(fog_toward),
        min(1.0, lamp.levels),
    );
    let fogged = vec4<f32>(
        lit.x * fog_through.x + fog_sky.x * (1.0 - fog_through.x),
        lit.y * fog_through.y + fog_sky.y * (1.0 - fog_through.y),
        lit.z * fog_through.z + fog_sky.z * (1.0 - fog_through.z),
        coverage,
    );
    let marker = select(0.0, 1.0, marked);
    let blended = entry.alpha_mode == 2u;
    if blended && coverage <= 0.0 {
        discard;
    }
    if blended {
        let setup = settings[0u];
        let width = f32(setup.width);
        let height = f32(setup.height);
        let pixel_x = u32(min(max(floor(screen.x * width), 0.0), width - 1.0));
        let pixel_y = u32(min(max(floor(screen.y * height), 0.0), height - 1.0));
        let pixel = pixel_y * setup.width + pixel_x;
        let held = pixel * setup.layers;
        let key = encoded_depth(reach, camera.near, camera.far)
            | encoded_coverage(coverage)
            | 0x40000000u;
        var placed = false;
        var attempts = 8u;
        while !placed && attempts > 0u {
            attempts -= 1u;
            var free = 4u;
            var farthest = 4u;
            var deepest = 0u;
            for (var layer = 0u; layer < setup.layers; layer = layer + 1u) {
                let occupant = atomicLoad(&keys[held + layer]);
                if occupant == 0u {
                    if free == 4u {
                        free = layer;
                    }
                } else if occupant > 1u && (farthest == 4u || occupant > deepest) {
                    farthest = layer;
                    deepest = occupant;
                }
            }
            if free == 4u && farthest == 4u {
                break;
            }
            if free == 4u && key >= deepest {
                break;
            }
            let chosen = select(farthest, free, free != 4u);
            let expected = select(deepest, 0u, free != 4u);
            let previous = atomicCompareExchangeWeak(&keys[held + chosen], expected, 1u).old_value;
            if previous == expected {
                if expected != 0u {
                    atomicAdd(&dropped[0u], 1u);
                }
                colors[held + chosen] = encoded_color(fogged.x, fogged.y, fogged.z);
                atomicStore(&keys[held + chosen], key);
                placed = true;
            }
        }
        if !placed {
            atomicAdd(&dropped[0u], 1u);
        }
    }
    return vec4<f32>(
        select(fogged.x, fogged.x * coverage, blended),
        select(fogged.y, fogged.y * coverage, blended),
        select(fogged.z, fogged.z * coverage, blended),
        select(marker, coverage, blended),
    );
}

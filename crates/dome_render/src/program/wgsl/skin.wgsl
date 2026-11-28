fn deformed(col0: vec4<f32>, col1: vec4<f32>, col2: vec4<f32>, col3: vec4<f32>, point: vec4<f32>) -> vec4<f32> {
    return col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w;
}

fn blended(first: Joint, second: Joint, third: Joint, fourth: Joint, weights: vec4<f32>, point: vec4<f32>) -> vec4<f32> {
    return deformed(first.col0, first.col1, first.col2, first.col3, point) * weights.x
        + deformed(second.col0, second.col1, second.col2, second.col3, point) * weights.y
        + deformed(third.col0, third.col1, third.col2, third.col3, point) * weights.z
        + deformed(fourth.col0, fourth.col1, fourth.col2, fourth.col3, point) * weights.w;
}

struct Skinned {
    position: vec4<f32>,
    normal: vec4<f32>,
}

fn skinned(held: Instance, vertex: u32, base: u32, point: vec4<f32>, normal: vec4<f32>) -> Skinned {
    if held.joints == 0u {
        return Skinned(point, normal);
    }
    let skin = skins[vertex];
    let at = held.palette + base;
    let first = palette[at + skin.joints.x];
    let second = palette[at + skin.joints.y];
    let third = palette[at + skin.joints.z];
    let fourth = palette[at + skin.joints.w];
    return Skinned(
        blended(first, second, third, fourth, skin.weights, point),
        blended(first, second, third, fourth, skin.weights, normal),
    );
}

fn skinned_position(held: Instance, vertex: u32, base: u32, point: vec4<f32>) -> vec4<f32> {
    if held.joints == 0u {
        return point;
    }
    let skin = skins[vertex];
    let at = held.palette + base;
    return blended(
        palette[at + skin.joints.x],
        palette[at + skin.joints.y],
        palette[at + skin.joints.z],
        palette[at + skin.joints.w],
        skin.weights,
        point,
    );
}

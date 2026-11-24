@group(0) @binding(0) var<storage, read> setup: array<Atmosphere>;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) surface: vec2<f32>,
};

struct FragmentIn {
    @location(0) at: vec2<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) vertex: u32) -> VertexOut {
    var x = -1.0;
    var y = -1.0;
    if (vertex == 1u) {
        x = 3.0;
    }
    if (vertex == 2u) {
        y = 3.0;
    }
    var output: VertexOut;
    output.position = vec4<f32>(x, y, 0.0, 1.0);
    output.surface = vec2<f32>(x, y);
    return output;
}

@fragment
fn fragment(input: FragmentIn) -> @location(0) vec4<f32> {
    let held = setup[0u];
    let cosine = (input.at.x * 0.5 + 0.5) * 2.0 - 1.0;
    let altitude = (input.at.y * held.vertical + 0.5) * 60.0;
    let origin = vec4<f32>(0.0, 6360.0 + altitude, 0.0, 0.0);
    let sine = sqrt(max(1.0 - cosine * cosine, 0.0));
    let direction = vec4<f32>(0.0, cosine, 0.0 - sine, 0.0);
    var transmittance = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    if (sphere(origin, direction, 6360.0).x <= 0.0) {
        let atmosphere = sphere(origin, direction, 6420.0);
        if (atmosphere.y > 0.0) {
            let step = max(atmosphere.y, 0.0) * (1.0 / 40.0);
            var optical = vec4<f32>(0.0, 0.0, 0.0, 0.0);
            var start = 0.0;
            for (var _step = 0u; _step < 40u; _step = _step + 1u) {
                let middle = origin + direction * (start + step * 0.3);
                optical = optical
                    + extinction_of(
                        held.rayleigh,
                        held.mie,
                        held.absorption,
                        held.ozone,
                        length3(middle) - 6360.0,
                    ) * step;
                start += step;
            }
            transmittance = vec4<f32>(
                exp(0.0 - optical.x),
                exp(0.0 - optical.y),
                exp(0.0 - optical.z),
                1.0,
            );
        }
    }
    return transmittance;
}

fn extinction_of(
    rayleigh: vec4<f32>,
    mie: vec4<f32>,
    absorption: vec4<f32>,
    ozone: vec4<f32>,
    altitude: f32,
) -> vec4<f32> {
    let thin = exp(0.0 - altitude * (1.0 / 8.0));
    let haze = exp(0.0 - altitude * (1.0 / 1.2));
    let layer = max(1.0 - abs(altitude - 25.0) * (1.0 / 15.0), 0.0);
    return vec4<f32>(
        rayleigh.x * thin + mie.x * haze + absorption.x * haze + ozone.x * layer,
        rayleigh.y * thin + mie.y * haze + absorption.y * haze + ozone.y * layer,
        rayleigh.z * thin + mie.z * haze + absorption.z * haze + ozone.z * layer,
        0.0,
    );
}

fn sphere(origin: vec4<f32>, direction: vec4<f32>, radius: f32) -> vec4<f32> {
    let along = dot3(origin, direction);
    let outside = dot3(origin, origin) - radius * radius;
    let discriminant = along * along - outside;
    if (discriminant < 0.0) {
        return vec4<f32>(-1.0, -1.0, 0.0, 0.0);
    }
    let root = sqrt(discriminant);
    return vec4<f32>(0.0 - along - root, 0.0 - along + root, 0.0, 0.0);
}

fn length3(value: vec4<f32>) -> f32 {
    return sqrt(dot3(value, value));
}

fn dot3(left: vec4<f32>, right: vec4<f32>) -> f32 {
    return left.x * right.x + left.y * right.y + left.z * right.z;
}

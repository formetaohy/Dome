@group(0) @binding(0) var<storage, read> setup: array<Atmosphere>;
@group(1) @binding(0) var transmittance: texture_2d<f32>;
@group(1) @binding(1) var lut_sampler: sampler;

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
    let sun = vec4<f32>(0.0, cosine, 0.0 - sine, 0.0);
    var luminance = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var transfer = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    for (var index = 0u; index < 64u; index = index + 1u) {
        let rank = 1.0 - 2.0 * (f32(index) + 0.5) * (1.0 / 64.0);
        let radius = sqrt(max(1.0 - rank * rank, 0.0));
        let angle = 2.3999632 * f32(index);
        let direction = vec4<f32>(radius * cos(angle), rank, radius * sin(angle), 0.0);
        let atmosphere = sphere(origin, direction, 6420.0);
        if (atmosphere.y > 0.0) {
            let ground = sphere(origin, direction, 6360.0);
            let beyond = ground.x > 0.0;
            var distance = atmosphere.y;
            if (beyond) {
                distance = ground.x;
            }
            let incident = dot3(direction, sun);
            let rayleigh = phase_rayleigh(incident);
            let mie = phase_mie(incident, held.anisotropy);
            let step = distance * (1.0 / 20.0);
            var view = vec4<f32>(1.0, 1.0, 1.0, 0.0);
            var absorbed = vec4<f32>(0.0, 0.0, 0.0, 0.0);
            var lost = vec4<f32>(0.0, 0.0, 0.0, 0.0);
            var start = 0.0;
            for (var _step = 0u; _step < 20u; _step = _step + 1u) {
                let middle = origin + direction * (start + step * 0.3);
                let here = length3(middle) - 6360.0;
                let scattered = thin(held.rayleigh, here);
                let suspended = haze(held.mie, here);
                let extinction =
                    extinction_of(held.rayleigh, held.mie, held.absorption, held.ozone, here);
                let sunlight = sampled(transmittance, lut_sampler, sun.y, here);
                let in_scattering = scattered * (sunlight * rayleigh) + suspended * (sunlight * mie);
                let local = attenuation(extinction, step);
                absorbed = absorbed + view * integrated(in_scattering, local, extinction);
                lost = lost + view * integrated(scattered + suspended, local, extinction);
                view = view * local;
                start += step;
            }
            luminance = luminance + absorbed;
            transfer = transfer + lost;
            if (beyond) {
                let surface = normalized(origin + direction * distance);
                let sunlight = sampled(
                    transmittance,
                    lut_sampler,
                    sun.y,
                    length3(origin + direction * distance) - 6360.0,
                );
                let incidence = dot3(surface, sun);
                if (incidence > 0.0) {
                    luminance = luminance + view * held.ground * sunlight * incidence;
                }
            }
        }
    }
    let scale = 1.0 / 64.0;
    return vec4<f32>(
        luminance.x * scale / max(1.0 - transfer.x * scale, 0.0001),
        luminance.y * scale / max(1.0 - transfer.y * scale, 0.0001),
        luminance.z * scale / max(1.0 - transfer.z * scale, 0.0001),
        1.0,
    );
}

fn sampled(
    lookup: texture_2d<f32>,
    filter_sampler: sampler,
    cosine: f32,
    altitude: f32,
) -> vec4<f32> {
    return textureSampleLevel(
        lookup,
        filter_sampler,
        vec2<f32>(
            0.5 + 0.5 * cosine,
            min(max(altitude * (1.0 / 60.0), 0.0), 1.0),
        ),
        0.0,
    );
}

fn thin(rayleigh: vec4<f32>, altitude: f32) -> vec4<f32> {
    let held = exp(0.0 - altitude * (1.0 / 8.0));
    return vec4<f32>(rayleigh.x * held, rayleigh.y * held, rayleigh.z * held, 0.0);
}

fn haze(mie: vec4<f32>, altitude: f32) -> vec4<f32> {
    let held = exp(0.0 - altitude * (1.0 / 1.2));
    return vec4<f32>(mie.x * held, mie.y * held, mie.z * held, 0.0);
}

fn extinction_of(
    rayleigh: vec4<f32>,
    mie: vec4<f32>,
    absorption: vec4<f32>,
    ozone: vec4<f32>,
    altitude: f32,
) -> vec4<f32> {
    let held_thin = thin(rayleigh, altitude);
    let held_haze = haze(mie, altitude);
    let held_ozone = exp(0.0 - altitude * (1.0 / 1.2));
    let layer = max(1.0 - abs(altitude - 25.0) * (1.0 / 15.0), 0.0);
    return vec4<f32>(
        held_thin.x + held_haze.x + absorption.x * held_ozone + ozone.x * layer,
        held_thin.y + held_haze.y + absorption.y * held_ozone + ozone.y * layer,
        held_thin.z + held_haze.z + absorption.z * held_ozone + ozone.z * layer,
        0.0,
    );
}

fn attenuation(extinction: vec4<f32>, distance: f32) -> vec4<f32> {
    return vec4<f32>(
        exp(0.0 - extinction.x * distance),
        exp(0.0 - extinction.y * distance),
        exp(0.0 - extinction.z * distance),
        1.0,
    );
}

fn integrated(in_scattering: vec4<f32>, local: vec4<f32>, extinction: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        (in_scattering.x - in_scattering.x * local.x) / max(extinction.x, 0.000000000001),
        (in_scattering.y - in_scattering.y * local.y) / max(extinction.y, 0.000000000001),
        (in_scattering.z - in_scattering.z * local.z) / max(extinction.z, 0.000000000001),
        0.0,
    );
}

fn phase_rayleigh(cosine: f32) -> f32 {
    return 0.059683104 * (1.0 + cosine * cosine);
}

fn phase_mie(cosine: f32, anisotropy: f32) -> f32 {
    let numerator = (1.0 - anisotropy * anisotropy) * (1.0 + cosine * cosine);
    let denominator = (2.0 + anisotropy * anisotropy)
        * pow(
            1.0 + anisotropy * anisotropy - 2.0 * anisotropy * cosine,
            1.5,
        );
    return 0.11936621 * numerator / denominator;
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

fn normalized(vector: vec4<f32>) -> vec4<f32> {
    let inverse = 1.0 / max(length3(vector), 0.0000001);
    return vec4<f32>(vector.x * inverse, vector.y * inverse, vector.z * inverse, 0.0);
}

fn dot3(left: vec4<f32>, right: vec4<f32>) -> f32 {
    return left.x * right.x + left.y * right.y + left.z * right.z;
}

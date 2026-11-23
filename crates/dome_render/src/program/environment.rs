use super::*;

#[dome_shader::graphics]
mod prefilter {
    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x, y);
    }

    #[fragment]
    fn fragment(
        level: Read<BakeLevelRecord>,
        panorama: Texture2D,
        sampler: Sampler,
        at: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let held = level[0u32];
        let normal = ray(fvec2(at.x * 0.5 + 0.5, at.y * held.vertical + 0.5));
        let radiance = prefiltered(panorama, sampler, normal, held.roughness);
        target = fvec4(radiance.x, radiance.y, radiance.z, 1.0);
    }

    fn prefiltered(panorama: Texture2D, sampler: Sampler, normal: Fvec4, roughness: f32) -> Fvec4 {
        if roughness <= 0.0 {
            return sample_lod(panorama, sampler, equirect(normal), 0.0);
        }
        let view = normalized(normal);
        let mut sign = -1.0;
        if view.z >= 0.0 {
            sign = 1.0;
        }
        let frame = -1.0 / (sign + view.z);
        let cross = view.x * view.y * frame;
        let tangent = fvec4(
            1.0 + sign * view.x * view.x * frame,
            sign * cross,
            -sign * view.x,
            0.0,
        );
        let bitangent = fvec4(cross, sign + view.y * view.y * frame, -view.y, 0.0);
        let alpha = roughness * roughness;
        let mut sum = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut weight = 0.0;
        for index in stride(0u32, 128u32, 1u32) {
            let rank = radical(index);
            let phi = (710.0 / 113.0) * (index as f32 * (1.0 / 128.0));
            let cosine = sqrt(max(
                (1.0 - rank) / (1.0 + (alpha * alpha - 1.0) * rank),
                0.0,
            ));
            let sine = sqrt(max(1.0 - cosine * cosine, 0.0));
            let half = tangent * (sine * cos(phi)) + bitangent * (sine * sin(phi)) + view * cosine;
            let light = normalized(half * (2.0 * dot3(view, half)) - view);
            let incidence = max(dot3(view, light), 0.0);
            if incidence > 0.0 {
                sum = sum + sample_lod(panorama, sampler, equirect(light), 0.0) * incidence;
                weight += incidence;
            }
        }
        if weight == 0.0 {
            return sample_lod(panorama, sampler, equirect(view), 0.0);
        }
        sum * (1.0 / weight)
    }

    fn radical(bits: u32) -> f32 {
        let mut weight = 0.5;
        let mut value = 0.0;
        for bit in stride(0u32, 7u32, 1u32) {
            if ((bits >> bit) & 1u32) == 1u32 {
                value += weight;
            }
            weight *= 0.5;
        }
        value
    }

    fn ray(at: Fvec2) -> Fvec4 {
        let phi = (at.x - 0.5) * (710.0 / 113.0);
        let theta = at.y * (355.0 / 113.0);
        let sine = sin(theta);
        fvec4(sine * sin(phi), cos(theta), -sine * cos(phi), 0.0)
    }

    fn equirect(direction: Fvec4) -> Fvec2 {
        let unit = normalized(direction);
        fvec2(
            atan2(unit.x, -unit.z) * (113.0 / 710.0) + 0.5,
            acos(clamped(unit.y, -1.0, 1.0)) * (113.0 / 355.0),
        )
    }

    fn dot3(left: Fvec4, right: Fvec4) -> f32 {
        left.x * right.x + left.y * right.y + left.z * right.z
    }

    fn normalized(vector: Fvec4) -> Fvec4 {
        let inverse =
            1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
        vector * inverse
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }
}
#[dome_shader::graphics]
mod integration {
    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * 0.5 + 0.5);
    }

    #[fragment]
    fn fragment(
        setup: Read<IrradianceSetupRecord>,
        partials: ReadWrite<IrradianceRecord>,
        panorama: Texture2D,
        sampler: Sampler,
        at: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let held = setup[0u32];
        let index = floor(at.x * held.partials as f32) as u32;
        let width = held.width;
        let height = held.height;
        let width_f = width as f32;
        let height_f = height as f32;
        let solid = ((710.0 / 113.0) / width_f) * ((355.0 / 113.0) / height_f);
        let mut first = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut second = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut third = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut fourth = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut fifth = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut sixth = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut seventh = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut eighth = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut ninth = fvec4(0.0, 0.0, 0.0, 0.0);
        for texel in stride(index, held.texels, held.partials) {
            let column = texel % width;
            let row = texel / width;
            let u = (column as f32 + 0.5) / width_f;
            let v = (row as f32 + 0.5) / height_f;
            let phi = (u - 0.5) * (710.0 / 113.0);
            let theta = v * (355.0 / 113.0);
            let sine = sin(theta);
            let axis_x = sine * sin(phi);
            let axis_y = cos(theta);
            let axis_z = -sine * cos(phi);
            let radiance = sample_lod(panorama, sampler, fvec2(u, v), 0.0)
                * (sin(v * (355.0 / 113.0)) * solid);
            first = first + radiance * 0.886227;
            second = second + radiance * (1.0233276 * axis_y);
            third = third + radiance * (1.0233276 * axis_z);
            fourth = fourth + radiance * (1.0233276 * axis_x);
            fifth = fifth + radiance * (0.8580851 * axis_x * axis_y);
            sixth = sixth + radiance * (0.8580851 * axis_y * axis_z);
            seventh = seventh + radiance * (0.2477083 * (3.0 * axis_z * axis_z - 1.0));
            eighth = eighth + radiance * (0.8580851 * axis_x * axis_z);
            ninth = ninth + radiance * (0.429041 * (axis_x * axis_x - axis_y * axis_y));
        }
        partials[index].coefficient0 = first;
        partials[index].coefficient1 = second;
        partials[index].coefficient2 = third;
        partials[index].coefficient3 = fourth;
        partials[index].coefficient4 = fifth;
        partials[index].coefficient5 = sixth;
        partials[index].coefficient6 = seventh;
        partials[index].coefficient7 = eighth;
        partials[index].coefficient8 = ninth;
        target = fvec4(0.0, 0.0, 0.0, 1.0);
    }
}
#[dome_shader::kernel(workgroup_size = 64)]
mod irradiance_reduction {
    #[kernel]
    fn irradiance_reduction(lid: u32, partials: Read<f32>, total: ReadWrite<f32>) {
        if lid < 27u32 {
            let at = (lid / 3u32) * 4u32 + lid % 3u32;
            let mut sum = 0.0;
            for index in stride(0u32, 1024u32, 1u32) {
                sum += partials[index * 36u32 + at];
            }
            total[lid] = sum;
        }
    }
}
pub(crate) fn prefiltering() -> GraphicsProgram {
    prefilter()
}
#[dome_shader::graphics]
mod atmosphere_transmittance {
    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x, y);
    }

    #[fragment]
    fn fragment(setup: Read<AtmosphereRecord>, at: In<Fvec2>, target: Out<Fvec4>) {
        let held = setup[0u32];
        let cosine = (at.x * 0.5 + 0.5) * 2.0 - 1.0;
        let altitude = (at.y * held.vertical + 0.5) * 60.0;
        let origin = fvec4(0.0, 6360.0 + altitude, 0.0, 0.0);
        let sine = sqrt(max(1.0 - cosine * cosine, 0.0));
        let direction = fvec4(0.0, cosine, 0.0 - sine, 0.0);
        let mut transmittance = fvec4(0.0, 0.0, 0.0, 1.0);
        if sphere(origin, direction, 6360.0).x <= 0.0 {
            let atmosphere = sphere(origin, direction, 6420.0);
            if atmosphere.y > 0.0 {
                let step = max(atmosphere.y, 0.0) * (1.0 / 40.0);
                let mut optical = fvec4(0.0, 0.0, 0.0, 0.0);
                let mut start = 0.0;
                for _step in stride(0u32, 40u32, 1u32) {
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
                transmittance = fvec4(
                    exp(0.0 - optical.x),
                    exp(0.0 - optical.y),
                    exp(0.0 - optical.z),
                    1.0,
                );
            }
        }
        target = transmittance;
    }

    fn extinction_of(
        rayleigh: Fvec4,
        mie: Fvec4,
        absorption: Fvec4,
        ozone: Fvec4,
        altitude: f32,
    ) -> Fvec4 {
        let thin = exp(0.0 - altitude * (1.0 / 8.0));
        let haze = exp(0.0 - altitude * (1.0 / 1.2));
        let layer = max(1.0 - abs(altitude - 25.0) * (1.0 / 15.0), 0.0);
        fvec4(
            rayleigh.x * thin + mie.x * haze + absorption.x * haze + ozone.x * layer,
            rayleigh.y * thin + mie.y * haze + absorption.y * haze + ozone.y * layer,
            rayleigh.z * thin + mie.z * haze + absorption.z * haze + ozone.z * layer,
            0.0,
        )
    }

    fn sphere(origin: Fvec4, direction: Fvec4, radius: f32) -> Fvec4 {
        let along = dot3(origin, direction);
        let outside = dot3(origin, origin) - radius * radius;
        let discriminant = along * along - outside;
        if discriminant < 0.0 {
            return fvec4(-1.0, -1.0, 0.0, 0.0);
        }
        let root = sqrt(discriminant);
        fvec4(0.0 - along - root, 0.0 - along + root, 0.0, 0.0)
    }

    fn length3(value: Fvec4) -> f32 {
        sqrt(dot3(value, value))
    }

    fn dot3(left: Fvec4, right: Fvec4) -> f32 {
        left.x * right.x + left.y * right.y + left.z * right.z
    }
}
#[dome_shader::graphics]
mod atmosphere_scattering {
    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x, y);
    }

    #[fragment]
    fn fragment(
        setup: Read<AtmosphereRecord>,
        transmittance: Texture2D,
        sampler: Sampler,
        at: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let held = setup[0u32];
        let cosine = (at.x * 0.5 + 0.5) * 2.0 - 1.0;
        let altitude = (at.y * held.vertical + 0.5) * 60.0;
        let origin = fvec4(0.0, 6360.0 + altitude, 0.0, 0.0);
        let sine = sqrt(max(1.0 - cosine * cosine, 0.0));
        let sun = fvec4(0.0, cosine, 0.0 - sine, 0.0);
        let mut luminance = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut transfer = fvec4(0.0, 0.0, 0.0, 0.0);
        for index in stride(0u32, 64u32, 1u32) {
            let rank = 1.0 - 2.0 * ((index as f32) + 0.5) * (1.0 / 64.0);
            let radius = sqrt(max(1.0 - rank * rank, 0.0));
            let angle = 2.3999632 * (index as f32);
            let direction = fvec4(radius * cos(angle), rank, radius * sin(angle), 0.0);
            let atmosphere = sphere(origin, direction, 6420.0);
            if atmosphere.y > 0.0 {
                let ground = sphere(origin, direction, 6360.0);
                let beyond = ground.x > 0.0;
                let mut distance = atmosphere.y;
                if beyond {
                    distance = ground.x;
                }
                let incident = dot3(direction, sun);
                let rayleigh = phase_rayleigh(incident);
                let mie = phase_mie(incident, held.anisotropy);
                let step = distance * (1.0 / 20.0);
                let mut view = fvec4(1.0, 1.0, 1.0, 0.0);
                let mut absorbed = fvec4(0.0, 0.0, 0.0, 0.0);
                let mut lost = fvec4(0.0, 0.0, 0.0, 0.0);
                let mut start = 0.0;
                for _step in stride(0u32, 20u32, 1u32) {
                    let middle = origin + direction * (start + step * 0.3);
                    let here = length3(middle) - 6360.0;
                    let scattered = thin(held.rayleigh, here);
                    let suspended = haze(held.mie, here);
                    let extinction =
                        extinction_of(held.rayleigh, held.mie, held.absorption, held.ozone, here);
                    let sunlight = sampled(transmittance, sampler, sun.y, here);
                    let in_scattering =
                        scattered * (sunlight * rayleigh) + suspended * (sunlight * mie);
                    let local = attenuation(extinction, step);
                    absorbed = absorbed + view * integrated(in_scattering, local, extinction);
                    lost = lost + view * integrated(scattered + suspended, local, extinction);
                    view = view * local;
                    start += step;
                }
                luminance = luminance + absorbed;
                transfer = transfer + lost;
                if beyond {
                    let surface = normalized(origin + direction * distance);
                    let sunlight = sampled(
                        transmittance,
                        sampler,
                        sun.y,
                        length3(origin + direction * distance) - 6360.0,
                    );
                    let incidence = dot3(surface, sun);
                    if incidence > 0.0 {
                        luminance = luminance + view * held.ground * sunlight * incidence;
                    }
                }
            }
        }
        let scale = 1.0 / 64.0;
        target = fvec4(
            luminance.x * scale / max(1.0 - transfer.x * scale, 0.0001),
            luminance.y * scale / max(1.0 - transfer.y * scale, 0.0001),
            luminance.z * scale / max(1.0 - transfer.z * scale, 0.0001),
            1.0,
        );
    }

    fn sampled(lookup: Texture2D, filter: Sampler, cosine: f32, altitude: f32) -> Fvec4 {
        sample_lod(
            lookup,
            filter,
            fvec2(
                0.5 + 0.5 * cosine,
                min(max(altitude * (1.0 / 60.0), 0.0), 1.0),
            ),
            0.0,
        )
    }

    fn thin(rayleigh: Fvec4, altitude: f32) -> Fvec4 {
        let held = exp(0.0 - altitude * (1.0 / 8.0));
        fvec4(rayleigh.x * held, rayleigh.y * held, rayleigh.z * held, 0.0)
    }

    fn haze(mie: Fvec4, altitude: f32) -> Fvec4 {
        let held = exp(0.0 - altitude * (1.0 / 1.2));
        fvec4(mie.x * held, mie.y * held, mie.z * held, 0.0)
    }

    fn extinction_of(
        rayleigh: Fvec4,
        mie: Fvec4,
        absorption: Fvec4,
        ozone: Fvec4,
        altitude: f32,
    ) -> Fvec4 {
        let held_thin = thin(rayleigh, altitude);
        let held_haze = haze(mie, altitude);
        let held_ozone = exp(0.0 - altitude * (1.0 / 1.2));
        let layer = max(1.0 - abs(altitude - 25.0) * (1.0 / 15.0), 0.0);
        fvec4(
            held_thin.x + held_haze.x + absorption.x * held_ozone + ozone.x * layer,
            held_thin.y + held_haze.y + absorption.y * held_ozone + ozone.y * layer,
            held_thin.z + held_haze.z + absorption.z * held_ozone + ozone.z * layer,
            0.0,
        )
    }

    fn attenuation(extinction: Fvec4, distance: f32) -> Fvec4 {
        fvec4(
            exp(0.0 - extinction.x * distance),
            exp(0.0 - extinction.y * distance),
            exp(0.0 - extinction.z * distance),
            1.0,
        )
    }

    fn integrated(in_scattering: Fvec4, local: Fvec4, extinction: Fvec4) -> Fvec4 {
        fvec4(
            (in_scattering.x - in_scattering.x * local.x) / max(extinction.x, 0.000000000001),
            (in_scattering.y - in_scattering.y * local.y) / max(extinction.y, 0.000000000001),
            (in_scattering.z - in_scattering.z * local.z) / max(extinction.z, 0.000000000001),
            0.0,
        )
    }

    fn phase_rayleigh(cosine: f32) -> f32 {
        0.059683104 * (1.0 + cosine * cosine)
    }

    fn phase_mie(cosine: f32, anisotropy: f32) -> f32 {
        let numerator = (1.0 - anisotropy * anisotropy) * (1.0 + cosine * cosine);
        let denominator = (2.0 + anisotropy * anisotropy)
            * pow(
                1.0 + anisotropy * anisotropy - 2.0 * anisotropy * cosine,
                1.5,
            );
        0.11936621 * numerator / denominator
    }

    fn sphere(origin: Fvec4, direction: Fvec4, radius: f32) -> Fvec4 {
        let along = dot3(origin, direction);
        let outside = dot3(origin, origin) - radius * radius;
        let discriminant = along * along - outside;
        if discriminant < 0.0 {
            return fvec4(-1.0, -1.0, 0.0, 0.0);
        }
        let root = sqrt(discriminant);
        fvec4(0.0 - along - root, 0.0 - along + root, 0.0, 0.0)
    }

    fn length3(value: Fvec4) -> f32 {
        sqrt(dot3(value, value))
    }

    fn normalized(vector: Fvec4) -> Fvec4 {
        let inverse = 1.0 / max(length3(vector), 0.0000001);
        fvec4(
            vector.x * inverse,
            vector.y * inverse,
            vector.z * inverse,
            0.0,
        )
    }

    fn dot3(left: Fvec4, right: Fvec4) -> f32 {
        left.x * right.x + left.y * right.y + left.z * right.z
    }
}
#[dome_shader::graphics]
mod atmosphere_sky {
    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x, y);
    }

    #[fragment]
    fn fragment(
        setup: Read<AtmosphereRecord>,
        transmittance: Texture2D,
        scattering: Texture2D,
        sampler: Sampler,
        at: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let held = setup[0u32];
        let u = at.x * 0.5 + 0.5;
        let v = at.y * held.vertical + 0.5;
        let phi = (u - 0.5) * (710.0 / 113.0);
        let theta = v * (355.0 / 113.0);
        let sine = sin(theta);
        let direction = fvec4(sine * sin(phi), cos(theta), 0.0 - sine * cos(phi), 0.0);
        let origin = fvec4(0.0, 6360.0 + held.altitude, 0.0, 0.0);
        let mut radiance = fvec4(0.0, 0.0, 0.0, 0.0);
        let atmosphere = sphere(origin, direction, 6420.0);
        if atmosphere.y > 0.0 {
            let ground = sphere(origin, direction, 6360.0);
            let beyond = ground.x > 0.0;
            let mut distance = atmosphere.y;
            if beyond {
                distance = ground.x;
            }
            if distance > 0.0 {
                let incident = dot3(direction, held.sun);
                let rayleigh = phase_rayleigh(incident);
                let mie = phase_mie(incident, held.anisotropy);
                let step = distance * (1.0 / 24.0);
                let mut view = fvec4(1.0, 1.0, 1.0, 0.0);
                let mut start = 0.0;
                for _step in stride(0u32, 24u32, 1u32) {
                    let middle = origin + direction * (start + step * 0.3);
                    let here = length3(middle) - 6360.0;
                    let scattered = thin(held.rayleigh, here);
                    let suspended = haze(held.mie, here);
                    let extinction =
                        extinction_of(held.rayleigh, held.mie, held.absorption, held.ozone, here);
                    let sunlight = sampled(transmittance, sampler, held.sun.y, here);
                    let rebound = sampled(scattering, sampler, held.sun.y, here);
                    let in_scattering = scattered * (sunlight * rayleigh + rebound)
                        + suspended * (sunlight * mie + rebound);
                    let local = attenuation(extinction, step);
                    radiance = radiance + view * integrated(in_scattering, local, extinction);
                    view = view * local;
                    start += step;
                }
                if beyond {
                    let surface = normalized(origin + direction * distance);
                    let sunlight = sampled(
                        transmittance,
                        sampler,
                        held.sun.y,
                        length3(origin + direction * distance) - 6360.0,
                    );
                    radiance = radiance
                        + view * held.ground * sunlight * max(dot3(surface, held.sun), 0.0);
                }
            }
        }
        if held.disk == 1u32 {
            let angle = acos(min(max(dot3(direction, held.sun), -1.0), 1.0));
            let half = 0.5
                * max(
                    (710.0 / 113.0) * (1.0 / held.width) * sin(v * (355.0 / 113.0)),
                    (355.0 / 113.0) * (1.0 / held.height),
                );
            let coverage = min(max((0.004675 + half - angle) / (2.0 * half), 0.0), 1.0);
            if coverage > 0.0 {
                let disk = sampled(transmittance, sampler, held.sun.y, held.altitude)
                    * (1.0 / ((355.0 / 113.0) * 0.0000218556));
                radiance = radiance + disk * coverage;
            }
        }
        target = fvec4(radiance.x, radiance.y, radiance.z, 1.0);
    }

    fn sampled(lookup: Texture2D, filter: Sampler, cosine: f32, altitude: f32) -> Fvec4 {
        sample_lod(
            lookup,
            filter,
            fvec2(
                0.5 + 0.5 * cosine,
                min(max(altitude * (1.0 / 60.0), 0.0), 1.0),
            ),
            0.0,
        )
    }

    fn thin(rayleigh: Fvec4, altitude: f32) -> Fvec4 {
        let held = exp(0.0 - altitude * (1.0 / 8.0));
        fvec4(rayleigh.x * held, rayleigh.y * held, rayleigh.z * held, 0.0)
    }

    fn haze(mie: Fvec4, altitude: f32) -> Fvec4 {
        let held = exp(0.0 - altitude * (1.0 / 1.2));
        fvec4(mie.x * held, mie.y * held, mie.z * held, 0.0)
    }

    fn extinction_of(
        rayleigh: Fvec4,
        mie: Fvec4,
        absorption: Fvec4,
        ozone: Fvec4,
        altitude: f32,
    ) -> Fvec4 {
        let held_thin = thin(rayleigh, altitude);
        let held_haze = haze(mie, altitude);
        let held_ozone = exp(0.0 - altitude * (1.0 / 1.2));
        let layer = max(1.0 - abs(altitude - 25.0) * (1.0 / 15.0), 0.0);
        fvec4(
            held_thin.x + held_haze.x + absorption.x * held_ozone + ozone.x * layer,
            held_thin.y + held_haze.y + absorption.y * held_ozone + ozone.y * layer,
            held_thin.z + held_haze.z + absorption.z * held_ozone + ozone.z * layer,
            0.0,
        )
    }

    fn attenuation(extinction: Fvec4, distance: f32) -> Fvec4 {
        fvec4(
            exp(0.0 - extinction.x * distance),
            exp(0.0 - extinction.y * distance),
            exp(0.0 - extinction.z * distance),
            1.0,
        )
    }

    fn integrated(in_scattering: Fvec4, local: Fvec4, extinction: Fvec4) -> Fvec4 {
        fvec4(
            (in_scattering.x - in_scattering.x * local.x) / max(extinction.x, 0.000000000001),
            (in_scattering.y - in_scattering.y * local.y) / max(extinction.y, 0.000000000001),
            (in_scattering.z - in_scattering.z * local.z) / max(extinction.z, 0.000000000001),
            0.0,
        )
    }

    fn phase_rayleigh(cosine: f32) -> f32 {
        0.059683104 * (1.0 + cosine * cosine)
    }

    fn phase_mie(cosine: f32, anisotropy: f32) -> f32 {
        let numerator = (1.0 - anisotropy * anisotropy) * (1.0 + cosine * cosine);
        let denominator = (2.0 + anisotropy * anisotropy)
            * pow(
                1.0 + anisotropy * anisotropy - 2.0 * anisotropy * cosine,
                1.5,
            );
        0.11936621 * numerator / denominator
    }

    fn sphere(origin: Fvec4, direction: Fvec4, radius: f32) -> Fvec4 {
        let along = dot3(origin, direction);
        let outside = dot3(origin, origin) - radius * radius;
        let discriminant = along * along - outside;
        if discriminant < 0.0 {
            return fvec4(-1.0, -1.0, 0.0, 0.0);
        }
        let root = sqrt(discriminant);
        fvec4(0.0 - along - root, 0.0 - along + root, 0.0, 0.0)
    }

    fn length3(value: Fvec4) -> f32 {
        sqrt(dot3(value, value))
    }

    fn normalized(vector: Fvec4) -> Fvec4 {
        let inverse = 1.0 / max(length3(vector), 0.0000001);
        fvec4(
            vector.x * inverse,
            vector.y * inverse,
            vector.z * inverse,
            0.0,
        )
    }

    fn dot3(left: Fvec4, right: Fvec4) -> f32 {
        left.x * right.x + left.y * right.y + left.z * right.z
    }
}
pub(crate) fn atmosphere_transmittance_program() -> GraphicsProgram {
    atmosphere_transmittance()
}
pub(crate) fn atmosphere_scattering_program() -> GraphicsProgram {
    atmosphere_scattering()
}
pub(crate) fn atmosphere_sky_program() -> GraphicsProgram {
    atmosphere_sky()
}
pub(crate) fn integrating() -> GraphicsProgram {
    integration()
}
pub(crate) fn irradiance_reduction_program() -> ComputeProgram {
    irradiance_reduction()
}

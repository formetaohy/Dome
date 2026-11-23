use super::*;

#[dome_shader::graphics]
mod instanced {
    #[binding]
    fn light(light: Read<LightRecord>) {}

    #[binding]
    fn cascade(cascades: Read<CascadeRecord>) {}

    #[binding]
    fn drawn(drawn: Read<DrawRecord>) {}

    #[binding]
    fn materials(materials: Read<MaterialRecord>) {}

    #[binding]
    fn view(view: Read<ViewRecord>) {}

    #[binding]
    fn shade(
        shadows: Texture2D,
        shadower: Sampler,
        radiance: Texture2D,
        radiance_sampler: Sampler,
        brdf: Texture2D,
        brdf_sampler: Sampler,
        point_shadows: Texture2D,
        point_shadower: Sampler,
    ) {
    }

    #[binding]
    fn palette(palette: Read<JointRecord>) {}

    #[binding]
    fn skins(skins: Read<SkinVertexRecord>) {}

    #[binding]
    fn transparent(
        keys: ReadWrite<AtomicU32>,
        colors: ReadWrite<u32>,
        dropped: ReadWrite<AtomicU32>,
        settings: Read<TransparentSetupRecord>,
    ) {
    }

    #[binding]
    fn probe(
        probes: Read<ProbeRecord>,
        probe_textures: TextureTable,
        probe_samplers: SamplerTable,
    ) {
    }

    fn deformed(col0: Fvec4, col1: Fvec4, col2: Fvec4, col3: Fvec4, point: Fvec4) -> Fvec4 {
        col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w
    }

    #[vertex]
    fn vertex(
        vertex: u32,
        instance: u32,
        vertices: Read<f32>,
        position: Out<Position>,
        placed: Out<Fvec4>,
        oriented: Out<Fvec4>,
        tinted: Out<Fvec4>,
    ) {
        let held = drawn[instance];
        let camera = view[0u32];
        let base = vertex * 8u32;
        let rest = fvec4(
            vertices[base],
            vertices[base + 1u32],
            vertices[base + 2u32],
            1.0,
        );
        let resting = held.normal0 * vertices[base + 3u32]
            + held.normal1 * vertices[base + 4u32]
            + held.normal2 * vertices[base + 5u32];
        let material = held.material as f32;
        tinted = fvec4(
            held.tint.x,
            held.tint.y,
            held.tint.z,
            select(material, -(material + 1.0), held.pick == camera.highlight),
        );
        if held.joints > 0u32 {
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
            let local = fvec4(place.x, place.y, place.z, 1.0);
            let moved = held.model0 * local.x
                + held.model1 * local.y
                + held.model2 * local.z
                + held.model3 * local.w;
            let projected = held.clip0 * local.x
                + held.clip1 * local.y
                + held.clip2 * local.z
                + held.clip3 * local.w;
            position = projected;
            placed = fvec4(moved.x, moved.y, moved.z, vertices[base + 6u32]);
            oriented = fvec4(normal.x, normal.y, normal.z, vertices[base + 7u32]);
        } else {
            let moved = held.model0 * rest.x
                + held.model1 * rest.y
                + held.model2 * rest.z
                + held.model3 * rest.w;
            let projected = held.clip0 * rest.x
                + held.clip1 * rest.y
                + held.clip2 * rest.z
                + held.clip3 * rest.w;
            position = projected;
            placed = fvec4(moved.x, moved.y, moved.z, vertices[base + 6u32]);
            oriented = fvec4(resting.x, resting.y, resting.z, vertices[base + 7u32]);
        }
    }

    #[fragment]
    fn fragment(
        textures: TextureTable,
        samplers: SamplerTable,
        lights: Read<u32>,
        world: In<Fvec4>,
        surface: In<Fvec4>,
        shaded: In<Fvec4>,
        target: Out<Fvec4>,
    ) {
        let position = world;
        let direction = surface;
        let tint = shaded;
        let stored = tint.w;
        let marked = stored < 0.0;
        let material = (abs(stored) + 0.5) as u32 - select(0u32, 1u32, marked);
        let camera = view[0u32];
        let projected = camera.clip0 * position.x
            + camera.clip1 * position.y
            + camera.clip2 * position.z
            + camera.clip3;
        let screen = fvec2(
            projected.x / projected.w * 0.5 + 0.5,
            projected.y / projected.w * camera.vertical + 0.5,
        );
        let reach = (position.x - camera.eye.x) * camera.forward.x
            + (position.y - camera.eye.y) * camera.forward.y
            + (position.z - camera.eye.z) * camera.forward.z;
        let entry = materials[material];
        let lamp = light[0u32];
        let unit = normalized(direction);
        let eye_direction = normalized(fvec4(
            camera.eye.x - position.x,
            camera.eye.y - position.y,
            camera.eye.z - position.z,
            0.0,
        ));
        let uv = fvec2(position.w, direction.w);
        let sampled = sample(
            textures[entry.base_color_image],
            samplers[entry.base_color_sampler],
            uv,
        );
        let base = fvec4(
            sampled.x * tint.x * entry.base_color.x,
            sampled.y * tint.y * entry.base_color.y,
            sampled.z * tint.z * entry.base_color.z,
            sampled.w * entry.base_color.w,
        );
        let coverage = clamped(base.w, 0.0, 1.0);
        let properties = sample(
            textures[entry.metallic_roughness_image],
            samplers[entry.metallic_roughness_sampler],
            uv,
        );
        let metallic = clamped(entry.metallic * properties.z, 0.0, 1.0);
        let roughness = clamped(entry.roughness * properties.y, 0.045, 1.0);
        let position_dx = dpdx(position);
        let position_dy = dpdy(position);
        let surface_dx = dpdx(surface);
        let surface_dy = dpdy(surface);
        let uv_dx = fvec2(position_dx.w, surface_dx.w);
        let uv_dy = fvec2(position_dy.w, surface_dy.w);
        let perpendicular_y = cross3(
            fvec4(position_dy.x, position_dy.y, position_dy.z, 0.0),
            unit,
        );
        let perpendicular_x = cross3(
            unit,
            fvec4(position_dx.x, position_dx.y, position_dx.z, 0.0),
        );
        let tangent = normalized(perpendicular_y * uv_dx.x + perpendicular_x * uv_dy.x);
        let bitangent = normalized(perpendicular_y * uv_dx.y + perpendicular_x * uv_dy.y);
        let mapped = sample(
            textures[entry.normal_image],
            samplers[entry.normal_sampler],
            uv,
        );
        let scaled = fvec4(
            (mapped.x * 2.0 - 1.0) * entry.normal_scale,
            (mapped.y * 2.0 - 1.0) * entry.normal_scale,
            mapped.z * 2.0 - 1.0,
            0.0,
        );
        let normal = normalized(tangent * scaled.x + bitangent * scaled.y + unit * scaled.z);
        if entry.alpha_mode == 1u32 && coverage < entry.alpha_cutoff {
            discard();
        }
        let albedo = fvec4(
            base.x * (1.0 - metallic),
            base.y * (1.0 - metallic),
            base.z * (1.0 - metallic),
            1.0,
        );
        let f0 = fvec4(
            0.04 + (base.x - 0.04) * metallic,
            0.04 + (base.y - 0.04) * metallic,
            0.04 + (base.z - 0.04) * metallic,
            1.0,
        );
        let travel = lamp.direction;
        let sun_direction = fvec4(-travel.x, -travel.y, -travel.z, 0.0);
        let incidence = max(dot3(normal, sun_direction), 0.0);
        let mut cascade = 0u32;
        if reach > cascades[0u32].split {
            cascade = 1u32;
        }
        if reach > cascades[1u32].split {
            cascade = 2u32;
        }
        if reach > cascades[2u32].split {
            cascade = 3u32;
        }
        let shadow = cascades[cascade];
        let offset = shadow.texel * 2.0;
        let offset_position = fvec4(
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
        let local = fvec2(
            placed.x * inverse * 0.5 + 0.5,
            placed.y * inverse * lamp.vertical + 0.5,
        );
        let atlas = shadow.atlas;
        let low = fvec2(
            atlas.x + shadow.atlas_texel * 0.5,
            atlas.y + shadow.atlas_texel * 0.5,
        );
        let high = fvec2(
            atlas.x + atlas.z - shadow.atlas_texel * 0.5,
            atlas.y + atlas.z - shadow.atlas_texel * 0.5,
        );
        let at = fvec2(atlas.x + local.x * atlas.z, atlas.y + local.y * atlas.z);
        let slope = sqrt(max(0.0, 1.0 - incidence * incidence)) / max(incidence, 0.1);
        let depth = placed.z * inverse - 0.0005 - shadow.texel / shadow.depth_span * slope;
        let sampled_shadow = shadowed(shadows, shadower, at, depth, shadow.atlas_texel, low, high);
        let outside = local.x < 0.0 || local.x > 1.0 || local.y < 0.0 || local.y > 1.0;
        let beyond = reach > cascades[3u32].split;
        let visibility = select(sampled_shadow, 1.0, beyond || outside);
        let direct = pbr(
            normal,
            eye_direction,
            sun_direction,
            base,
            metallic,
            roughness,
        );
        let sun = fvec4(
            direct.x * lamp.radiance.x * incidence * visibility,
            direct.y * lamp.radiance.y * incidence * visibility,
            direct.z * lamp.radiance.z * incidence * visibility,
            0.0,
        );
        let occlusion = sample(
            textures[entry.occlusion_image],
            samplers[entry.occlusion_sampler],
            uv,
        );
        let openness = 1.0 + entry.occlusion_strength * (occlusion.x - 1.0);
        let view_dot = dot3(normal, eye_direction);
        let reflected = normalized(fvec4(
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
        let prefiltered = sample_lod(
            radiance,
            radiance_sampler,
            equirect(environment_reflected),
            roughness * lamp.levels,
        );
        let split = sample(brdf, brdf_sampler, fvec2(max(view_dot, 0.0001), roughness));
        let environment_specular = fvec4(
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
        let mut local_red = 0.0;
        let mut local_green = 0.0;
        let mut local_blue = 0.0;
        let mut local_weight = 0.0;
        let mut local_coverage = 0.0;
        for index in stride(0u32, lamp.probes, 1u32) {
            let held = probes[index];
            let relative = fvec4(
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
                let held_radiance = sample_lod(
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
                let held_specular = fvec4(
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
        let ambient = fvec4(
            global_ambient.x * global_scale + local_red * local_scale,
            global_ambient.y * global_scale + local_green * local_scale,
            global_ambient.z * global_scale + local_blue * local_scale,
            0.0,
        );
        let glow = sample(
            textures[entry.emissive_image],
            samplers[entry.emissive_sampler],
            uv,
        );
        let emissive = fvec4(
            glow.x * entry.emissive.x,
            glow.y * entry.emissive.y,
            glow.z * entry.emissive.z,
            0.0,
        );
        let mut emitted_x = 0.0;
        let mut emitted_y = 0.0;
        let mut emitted_z = 0.0;
        if camera.point_lights > 0u32
            && camera.cluster_x > 0u32
            && camera.cluster_y > 0u32
            && camera.far > camera.near
        {
            let tile_x = min(
                floor(screen.x * camera.view_width / 64.0),
                (camera.cluster_x - 1u32) as f32,
            ) as u32;
            let tile_y = min(
                floor(screen.y * camera.view_height / 64.0),
                (camera.cluster_y - 1u32) as f32,
            ) as u32;
            let ratio = log(max(reach, camera.near) / camera.near) / log(camera.far / camera.near);
            let mut slice = floor(ratio * 24.0) as u32;
            if slice >= 24u32 {
                slice = 23u32;
            }
            let clustered = (slice * camera.cluster_y + tile_y) * camera.cluster_x + tile_x;
            let cluster_base = camera.cluster_offset + clustered * 33u32;
            let mut count = lights[cluster_base];
            if count > 32u32 {
                count = 32u32;
            }
            for slot in stride(0u32, count, 1u32) {
                let light = lights[cluster_base + 1u32 + slot];
                let at = light * 12u32;
                let point_x = bitcast_f32(lights[at]);
                let point_y = bitcast_f32(lights[at + 1u32]);
                let point_z = bitcast_f32(lights[at + 2u32]);
                let point_range = bitcast_f32(lights[at + 3u32]);
                let radiance = fvec4(
                    bitcast_f32(lights[at + 4u32]),
                    bitcast_f32(lights[at + 5u32]),
                    bitcast_f32(lights[at + 6u32]),
                    0.0,
                );
                let shadow = fvec4(
                    bitcast_f32(lights[at + 8u32]),
                    bitcast_f32(lights[at + 9u32]),
                    bitcast_f32(lights[at + 10u32]),
                    bitcast_f32(lights[at + 11u32]),
                );
                let delta = fvec4(
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
                    fvec4(point_x, point_y, point_z, 0.0),
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
        let lit = fvec4(
            sun.x + emitted_x + ambient.x + emissive.x,
            sun.y + emitted_y + ambient.y + emissive.y,
            sun.z + emitted_z + ambient.z + emissive.z,
            coverage,
        );
        let fog_travel = fvec4(
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
        let fog_extinction = fvec4(
            lamp.rayleigh.x * thin_depth + (lamp.mie.x + lamp.absorption.x) * haze_depth,
            lamp.rayleigh.y * thin_depth + (lamp.mie.y + lamp.absorption.y) * haze_depth,
            lamp.rayleigh.z * thin_depth + (lamp.mie.z + lamp.absorption.z) * haze_depth,
            0.0,
        );
        let fog_through = fvec4(
            exp(0.0 - fog_extinction.x),
            exp(0.0 - fog_extinction.y),
            exp(0.0 - fog_extinction.z),
            0.0,
        );
        let fog_toward = turned(
            lamp.environment0,
            lamp.environment1,
            lamp.environment2,
            fvec4(
                0.0 - eye_direction.x,
                0.0 - eye_direction.y,
                0.0 - eye_direction.z,
                0.0,
            ),
        );
        let fog_sky = sample_lod(
            radiance,
            radiance_sampler,
            equirect(fog_toward),
            min(1.0, lamp.levels),
        );
        let fogged = fvec4(
            lit.x * fog_through.x + fog_sky.x * (1.0 - fog_through.x),
            lit.y * fog_through.y + fog_sky.y * (1.0 - fog_through.y),
            lit.z * fog_through.z + fog_sky.z * (1.0 - fog_through.z),
            coverage,
        );
        let marker = select(0.0, 1.0, marked);
        let blended = entry.alpha_mode == 2u32;
        if blended && coverage <= 0.0 {
            discard();
        }
        if blended {
            let setup = settings[0u32];
            let width = setup.width as f32;
            let height = setup.height as f32;
            let pixel_x = min(max(floor(screen.x * width), 0.0), width - 1.0) as u32;
            let pixel_y = min(max(floor(screen.y * height), 0.0), height - 1.0) as u32;
            let pixel = pixel_y * setup.width + pixel_x;
            let held = pixel * setup.layers;
            let key = encoded_depth(reach, camera.near, camera.far)
                | encoded_coverage(coverage)
                | 0x40000000u32;
            let mut placed = false;
            let mut attempts = 8u32;
            while !placed && attempts > 0u32 {
                attempts -= 1u32;
                let mut free = 4u32;
                let mut farthest = 4u32;
                let mut deepest = 0u32;
                for layer in stride(0u32, setup.layers, 1u32) {
                    let occupant = atomic_load(&keys[held + layer]);
                    if occupant == 0u32 {
                        if free == 4u32 {
                            free = layer;
                        }
                    } else if occupant > 1u32 && (farthest == 4u32 || occupant > deepest) {
                        farthest = layer;
                        deepest = occupant;
                    }
                }
                if free == 4u32 && farthest == 4u32 {
                    break;
                }
                if free == 4u32 && key >= deepest {
                    break;
                }
                let chosen = select(farthest, free, free != 4u32);
                let expected = select(deepest, 0u32, free != 4u32);
                let previous = atomic_compare_exchange(&keys[held + chosen], expected, 1u32);
                if previous == expected {
                    if expected != 0u32 {
                        atomic_add(&dropped[0u32], 1u32);
                    }
                    colors[held + chosen] = encoded_color(fogged.x, fogged.y, fogged.z);
                    atomic_store(&keys[held + chosen], key);
                    placed = true;
                }
            }
            if !placed {
                atomic_add(&dropped[0u32], 1u32);
            }
        }
        target = fvec4(
            select(fogged.x, fogged.x * coverage, blended),
            select(fogged.y, fogged.y * coverage, blended),
            select(fogged.z, fogged.z * coverage, blended),
            select(marker, coverage, blended),
        );
    }

    fn encoded_color(red: f32, green: f32, blue: f32) -> u32 {
        let peak = max(max(red, green), blue);
        let exponent = min(max((bitcast_u32(peak) >> 23u32) & 255u32, 8u32), 253u32) + 2u32;
        let unit = 255.0 / bitcast_f32((exponent - 1u32) << 23u32);
        (exponent << 24u32)
            | (encoded_byte(red * unit) << 16u32)
            | (encoded_byte(green * unit) << 8u32)
            | encoded_byte(blue * unit)
    }

    fn encoded_depth(reach: f32, near: f32, far: f32) -> u32 {
        let normalized = min(max((reach - near) / (far - near), 0.0), 1.0);
        encoded_fixed(normalized * 4194303.0, 4194303.0) << 8u32
    }

    fn encoded_coverage(coverage: f32) -> u32 {
        encoded_byte(coverage * 255.0)
    }

    fn encoded_byte(value: f32) -> u32 {
        encoded_fixed(value, 255.0)
    }

    fn encoded_fixed(value: f32, ceiling: f32) -> u32 {
        min(max(floor(value + 0.5), 0.0), ceiling) as u32
    }

    fn optical(climb: f32, low: f32, span: f32, scale: f32) -> f32 {
        let flat = span * exp(0.0 - low / scale);
        let slanted =
            (span * scale / climb) * (exp(0.0 - low / scale) - exp(0.0 - (low + climb) / scale));
        select(slanted, flat, abs(climb) < 0.001)
    }

    fn length3(value: Fvec4) -> f32 {
        sqrt(dot3(value, value))
    }

    fn irradiance_low(c0: Fvec4, c1: Fvec4, c2: Fvec4, c3: Fvec4, normal: Fvec4) -> Fvec4 {
        c0 * 0.282095
            + c1 * (0.488603 * normal.y)
            + c2 * (0.488603 * normal.z)
            + c3 * (0.488603 * normal.x)
    }

    fn turned(first: Fvec4, second: Fvec4, third: Fvec4, vector: Fvec4) -> Fvec4 {
        fvec4(
            first.x * vector.x + second.x * vector.y + third.x * vector.z,
            first.y * vector.x + second.y * vector.y + third.y * vector.z,
            first.z * vector.x + second.z * vector.y + third.z * vector.z,
            0.0,
        )
    }

    fn irradiance_high(
        c4: Fvec4,
        c5: Fvec4,
        c6: Fvec4,
        c7: Fvec4,
        c8: Fvec4,
        normal: Fvec4,
    ) -> Fvec4 {
        let x = normal.x;
        let y = normal.y;
        let z = normal.z;
        c4 * (1.092548 * x * y)
            + c5 * (1.092548 * y * z)
            + c6 * (0.315392 * (3.0 * z * z - 1.0))
            + c7 * (1.092548 * x * z)
            + c8 * (0.546274 * (x * x - y * y))
    }

    fn split_sum(f0: f32, split: Fvec4) -> f32 {
        let first = f0 * split.x + split.y;
        let energy = 1.0 - (split.x + split.y);
        let average = f0 + (1.0 - f0) * 0.047619;
        first + first * energy * average / (1.0 - energy * average)
    }

    fn box_projected(position: Fvec4, direction: Fvec4, extent: Fvec4) -> Fvec4 {
        let mut nearest = 1.0e30;
        let mut hit = true;
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
            return normalized(fvec4(
                position.x + direction.x * nearest,
                position.y + direction.y * nearest,
                position.z + direction.z * nearest,
                0.0,
            ));
        }
        direction
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

    fn cross3(left: Fvec4, right: Fvec4) -> Fvec4 {
        fvec4(
            left.y * right.z - left.z * right.y,
            left.z * right.x - left.x * right.z,
            left.x * right.y - left.y * right.x,
            0.0,
        )
    }

    fn pbr(
        normal: Fvec4,
        view: Fvec4,
        light: Fvec4,
        base: Fvec4,
        metallic: f32,
        roughness: f32,
    ) -> Fvec4 {
        let pi = 355.0 / 113.0;
        let n_dot_v = max(dot3(normal, view), 0.0001);
        let n_dot_l = max(dot3(normal, light), 0.0);
        let albedo = fvec4(
            base.x * (1.0 - metallic),
            base.y * (1.0 - metallic),
            base.z * (1.0 - metallic),
            1.0,
        );
        let f0 = fvec4(
            0.04 + (base.x - 0.04) * metallic,
            0.04 + (base.y - 0.04) * metallic,
            0.04 + (base.z - 0.04) * metallic,
            1.0,
        );
        let half = normalized(fvec4(
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
        let fresnel = fvec4(
            f0.x + (1.0 - f0.x) * fresnel_power,
            f0.y + (1.0 - f0.y) * fresnel_power,
            f0.z + (1.0 - f0.z) * fresnel_power,
            0.0,
        );
        let specular = fvec4(
            distribution * geometry * fresnel.x / (4.0 * n_dot_v * n_dot_l + 0.0001),
            distribution * geometry * fresnel.y / (4.0 * n_dot_v * n_dot_l + 0.0001),
            distribution * geometry * fresnel.z / (4.0 * n_dot_v * n_dot_l + 0.0001),
            0.0,
        );
        let diffuse = fvec4(
            (1.0 - fresnel.x) * albedo.x / pi,
            (1.0 - fresnel.y) * albedo.y / pi,
            (1.0 - fresnel.z) * albedo.z / pi,
            0.0,
        );
        fvec4(
            diffuse.x + specular.x,
            diffuse.y + specular.y,
            diffuse.z + specular.z,
            0.0,
        )
    }

    fn normalized(vector: Fvec4) -> Fvec4 {
        let inverse =
            1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
        vector * inverse
    }

    fn shadowed(
        shadows: Texture2D,
        shadower: Sampler,
        at: Fvec2,
        depth: f32,
        texel: f32,
        low: Fvec2,
        high: Fvec2,
    ) -> f32 {
        let left = clamped(at.x - texel, low.x, high.x);
        let right = clamped(at.x + texel, low.x, high.x);
        let down = clamped(at.y - texel, low.y, high.y);
        let up = clamped(at.y + texel, low.y, high.y);
        let x = clamped(at.x, low.x, high.x);
        let y = clamped(at.y, low.y, high.y);
        let total = compared(shadows, shadower, fvec2(left, down), depth)
            + compared(shadows, shadower, fvec2(x, down), depth)
            + compared(shadows, shadower, fvec2(right, down), depth)
            + compared(shadows, shadower, fvec2(left, y), depth)
            + compared(shadows, shadower, fvec2(x, y), depth)
            + compared(shadows, shadower, fvec2(right, y), depth)
            + compared(shadows, shadower, fvec2(left, up), depth)
            + compared(shadows, shadower, fvec2(x, up), depth)
            + compared(shadows, shadower, fvec2(right, up), depth);
        total * (1.0 / 9.0)
    }

    fn point_visible(
        atlas: Texture2D,
        atlas_sampler: Sampler,
        fragment: Fvec4,
        center: Fvec4,
        shadow: Fvec4,
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
        let mut index = 0.0;
        let mut horizontal = -dz;
        let mut sideways = -dy;
        let mut major = across;
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
        let at = fvec2((index + left) * (1.0 / 6.0), origin + top * scale);
        let texel = fvec2((1.0 / 6.0) / 256.0, scale / 256.0);
        let low = fvec2(index * (1.0 / 6.0) + texel.x * 0.5, origin + texel.y * 0.5);
        let high = fvec2(
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
        let total = point_compared(atlas, atlas_sampler, fvec2(left_x, down_y), depth)
            + point_compared(atlas, atlas_sampler, fvec2(at.x, down_y), depth)
            + point_compared(atlas, atlas_sampler, fvec2(right_x, down_y), depth)
            + point_compared(atlas, atlas_sampler, fvec2(left_x, at.y), depth)
            + point_compared(atlas, atlas_sampler, fvec2(at.x, at.y), depth)
            + point_compared(atlas, atlas_sampler, fvec2(right_x, at.y), depth)
            + point_compared(atlas, atlas_sampler, fvec2(left_x, up_y), depth)
            + point_compared(atlas, atlas_sampler, fvec2(at.x, up_y), depth)
            + point_compared(atlas, atlas_sampler, fvec2(right_x, up_y), depth);
        total * (1.0 / 9.0)
    }

    fn point_compared(atlas: Texture2D, atlas_sampler: Sampler, at: Fvec2, depth: f32) -> f32 {
        let stored = sample(atlas, atlas_sampler, at);
        if stored.x < depth { 0.0 } else { 1.0 }
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }

    fn compared(shadows: Texture2D, shadower: Sampler, at: Fvec2, depth: f32) -> f32 {
        let stored = sample(shadows, shadower, at);
        if stored.x < depth { 0.0 } else { 1.0 }
    }
}
#[dome_shader::graphics]
mod casting {
    #[binding]
    fn palette(palette: Read<JointRecord>) {}

    #[binding]
    fn skins(skins: Read<SkinVertexRecord>) {}

    fn deformed(col0: Fvec4, col1: Fvec4, col2: Fvec4, col3: Fvec4, point: Fvec4) -> Fvec4 {
        col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w
    }

    #[vertex]
    fn vertex(
        vertex: u32,
        instance: u32,
        drawn: Read<DrawRecord>,
        vertices: Read<f32>,
        position: Out<Position>,
        clip: Out<Fvec4>,
        surface: Out<Fvec4>,
    ) {
        let held = drawn[instance];
        let base = vertex * 8u32;
        let rest = fvec4(
            vertices[base],
            vertices[base + 1u32],
            vertices[base + 2u32],
            1.0,
        );
        let shaped = fvec4(
            vertices[base + 6u32],
            vertices[base + 7u32],
            held.material as f32,
            0.0,
        );
        if held.joints > 0u32 {
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
            let local = fvec4(place.x, place.y, place.z, 1.0);
            let moved = held.clip0 * local.x
                + held.clip1 * local.y
                + held.clip2 * local.z
                + held.clip3 * local.w;
            position = moved;
            clip = moved;
        } else {
            let moved = held.clip0 * rest.x
                + held.clip1 * rest.y
                + held.clip2 * rest.z
                + held.clip3 * rest.w;
            position = moved;
            clip = moved;
        }
        surface = shaped;
    }

    #[fragment]
    fn fragment(
        space: In<Fvec4>,
        shaped: In<Fvec4>,
        materials: Read<MaterialRecord>,
        textures: TextureTable,
        samplers: SamplerTable,
        target: Out<Fvec4>,
    ) {
        let placed = space;
        let shape = shaped;
        let material = shape.z as u32;
        let entry = materials[material];
        if entry.alpha_mode == 1u32 {
            let sampled = sample(
                textures[entry.base_color_image],
                samplers[entry.base_color_sampler],
                fvec2(shape.x, shape.y),
            );
            if sampled.w * entry.base_color.w < entry.alpha_cutoff {
                discard();
            }
        }
        target = fvec4(placed.z / placed.w, 0.0, 0.0, 1.0);
    }
}
#[dome_shader::graphics]
mod selection {
    #[binding]
    fn view(view: Read<ViewRecord>) {}

    #[binding]
    fn palette(palette: Read<JointRecord>) {}

    #[binding]
    fn skins(skins: Read<SkinVertexRecord>) {}

    fn deformed(col0: Fvec4, col1: Fvec4, col2: Fvec4, col3: Fvec4, point: Fvec4) -> Fvec4 {
        col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w
    }

    #[vertex]
    fn vertex(
        vertex: u32,
        instance: u32,
        drawn: Read<DrawRecord>,
        vertices: Read<f32>,
        position: Out<Position>,
        picked: Out<Fvec4>,
    ) {
        let held = drawn[instance];
        let camera = view[0u32];
        let base = vertex * 8u32;
        let rest = fvec4(
            vertices[base],
            vertices[base + 1u32],
            vertices[base + 2u32],
            1.0,
        );
        let id = held.pick;
        picked = fvec4(
            (id % 256u32) as f32 / 255.0,
            ((id / 256u32) % 256u32) as f32 / 255.0,
            ((id / 65536u32) % 256u32) as f32 / 255.0,
            1.0,
        );
        if held.joints > 0u32 {
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
            let local = fvec4(place.x, place.y, place.z, 1.0);
            let moved = held.model0 * local.x
                + held.model1 * local.y
                + held.model2 * local.z
                + held.model3 * local.w;
            position = camera.clip0 * moved.x
                + camera.clip1 * moved.y
                + camera.clip2 * moved.z
                + camera.clip3;
        } else {
            let moved = held.model0 * rest.x
                + held.model1 * rest.y
                + held.model2 * rest.z
                + held.model3 * rest.w;
            position = camera.clip0 * moved.x
                + camera.clip1 * moved.y
                + camera.clip2 * moved.z
                + camera.clip3;
        }
    }

    #[fragment]
    fn fragment(space: In<Fvec4>, target: Out<Fvec4>) {
        target = space;
    }
}
#[dome_shader::graphics]
mod depth {
    #[binding]
    fn view(view: Read<ViewRecord>) {}

    #[binding]
    fn palette(palette: Read<JointRecord>) {}

    #[binding]
    fn skins(skins: Read<SkinVertexRecord>) {}

    fn deformed(col0: Fvec4, col1: Fvec4, col2: Fvec4, col3: Fvec4, point: Fvec4) -> Fvec4 {
        col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w
    }

    #[vertex]
    fn vertex(
        vertex: u32,
        instance: u32,
        drawn: Read<DrawRecord>,
        vertices: Read<f32>,
        position: Out<Position>,
        world: Out<Fvec4>,
        surface: Out<Fvec4>,
    ) {
        let held = drawn[instance];
        let base = vertex * 8u32;
        let rest = fvec4(
            vertices[base],
            vertices[base + 1u32],
            vertices[base + 2u32],
            1.0,
        );
        let shaped = fvec4(
            vertices[base + 6u32],
            vertices[base + 7u32],
            held.material as f32,
            0.0,
        );
        if held.joints > 0u32 {
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
            let local = fvec4(place.x, place.y, place.z, 1.0);
            let moved = held.model0 * local.x
                + held.model1 * local.y
                + held.model2 * local.z
                + held.model3 * local.w;
            let projected = held.clip0 * local.x
                + held.clip1 * local.y
                + held.clip2 * local.z
                + held.clip3 * local.w;
            position = projected;
            world = moved;
        } else {
            let moved = held.model0 * rest.x
                + held.model1 * rest.y
                + held.model2 * rest.z
                + held.model3 * rest.w;
            let projected = held.clip0 * rest.x
                + held.clip1 * rest.y
                + held.clip2 * rest.z
                + held.clip3 * rest.w;
            position = projected;
            world = moved;
        }
        surface = shaped;
    }

    #[fragment]
    fn fragment(
        space: In<Fvec4>,
        shaped: In<Fvec4>,
        materials: Read<MaterialRecord>,
        textures: TextureTable,
        samplers: SamplerTable,
        target: Out<Fvec4>,
    ) {
        let placed = space;
        let shape = shaped;
        let material = shape.z as u32;
        let entry = materials[material];
        if entry.alpha_mode == 1u32 {
            let sampled = sample(
                textures[entry.base_color_image],
                samplers[entry.base_color_sampler],
                fvec2(shape.x, shape.y),
            );
            if sampled.w * entry.base_color.w < entry.alpha_cutoff {
                discard();
            }
        }
        let camera = view[0u32];
        let reach = (placed.x - camera.eye.x) * camera.forward.x
            + (placed.y - camera.eye.y) * camera.forward.y
            + (placed.z - camera.eye.z) * camera.forward.z;
        target = fvec4(
            (reach - camera.near) / (camera.far - camera.near),
            0.0,
            0.0,
            1.0,
        );
    }
}
pub(crate) fn instancing() -> GraphicsProgram {
    instanced()
}
pub(crate) fn shadowing() -> GraphicsProgram {
    casting()
}
pub(crate) fn depth_prepass() -> GraphicsProgram {
    depth()
}
pub(crate) fn picking() -> GraphicsProgram {
    selection()
}
pub(crate) fn debug_lines() -> GraphicsProgram {
    lines()
}
#[dome_shader::graphics]
mod lines {
    #[binding]
    fn view(view: Read<ViewRecord>) {}

    #[binding]
    fn segments(segments: Read<DebugLineRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, tint: Out<Fvec4>) {
        let camera = view[0u32];
        let segment = segments[vertex / 6u32];
        let corner = vertex % 6u32;
        let head = column(
            camera.clip0,
            camera.clip1,
            camera.clip2,
            camera.clip3,
            segment.start,
        );
        let tail = column(
            camera.clip0,
            camera.clip1,
            camera.clip2,
            camera.clip3,
            segment.end,
        );
        let mut x = 2.0;
        let mut y = 2.0;
        let mut z = 1.0;
        let mut alpha = 0.0;
        if head.w > 0.0001 && tail.w > 0.0001 {
            let head_x = head.x / head.w;
            let head_y = head.y / head.w;
            let tail_x = tail.x / tail.w;
            let tail_y = tail.y / tail.w;
            let span_x = (tail_x - head_x) * camera.view_width;
            let span_y = (tail_y - head_y) * camera.view_height;
            let reach = max(sqrt(span_x * span_x + span_y * span_y), 0.0001);
            let normal_x = -span_y / reach * 2.0 / camera.view_width;
            let normal_y = span_x / reach * 2.0 / camera.view_height;
            let mut end = 0.0;
            let mut side = -1.0;
            if corner == 1u32 {
                side = 1.0;
            }
            if corner == 2u32 {
                end = 1.0;
                side = 1.0;
            }
            if corner == 4u32 {
                end = 1.0;
                side = 1.0;
            }
            if corner == 5u32 {
                end = 1.0;
                side = -1.0;
            }
            x = head_x + (tail_x - head_x) * end + normal_x * side;
            y = head_y + (tail_y - head_y) * end + normal_y * side;
            z = head.z / head.w + (tail.z / tail.w - head.z / head.w) * end;
            alpha = segment.color.w;
        }
        position = fvec4(x, y, z, 1.0);
        tint = fvec4(segment.color.x, segment.color.y, segment.color.z, alpha);
    }

    #[fragment]
    fn fragment(input: In<Fvec4>, target: Out<Fvec4>) {
        let color = input;
        target = fvec4(
            color.x * color.w,
            color.y * color.w,
            color.z * color.w,
            color.w,
        );
    }

    fn column(a: Fvec4, b: Fvec4, c: Fvec4, d: Fvec4, local: Fvec4) -> Fvec4 {
        a * local.x + b * local.y + c * local.z + d * local.w
    }
}
#[dome_shader::graphics]
mod motion {
    #[binding]
    fn view(view: Read<MotionRecord>) {}

    #[binding]
    fn drawn(drawn: Read<DrawRecord>) {}

    #[binding]
    fn palette(palette: Read<JointRecord>) {}

    #[binding]
    fn skins(skins: Read<SkinVertexRecord>) {}

    #[binding]
    fn materials(materials: Read<MaterialRecord>) {}

    fn deformed(col0: Fvec4, col1: Fvec4, col2: Fvec4, col3: Fvec4, point: Fvec4) -> Fvec4 {
        col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w
    }

    #[vertex]
    fn vertex(
        vertex: u32,
        instance: u32,
        vertices: Read<f32>,
        position: Out<Position>,
        surface: Out<Fvec4>,
        travel: Out<Fvec4>,
    ) {
        let held = drawn[instance];
        let camera = view[0u32];
        let base = vertex * 8u32;
        let rest = fvec4(
            vertices[base],
            vertices[base + 1u32],
            vertices[base + 2u32],
            1.0,
        );
        let mut local = rest;
        let mut carried = rest;
        if held.joints > 0u32 {
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
            local = fvec4(place.x, place.y, place.z, 1.0);
            let before = held.palette - camera.base + camera.previous;
            let was_first = palette[before + skin.joints.x];
            let was_second = palette[before + skin.joints.y];
            let was_third = palette[before + skin.joints.z];
            let was_fourth = palette[before + skin.joints.w];
            let was = deformed(
                was_first.col0,
                was_first.col1,
                was_first.col2,
                was_first.col3,
                rest,
            ) * skin.weights.x
                + deformed(
                    was_second.col0,
                    was_second.col1,
                    was_second.col2,
                    was_second.col3,
                    rest,
                ) * skin.weights.y
                + deformed(
                    was_third.col0,
                    was_third.col1,
                    was_third.col2,
                    was_third.col3,
                    rest,
                ) * skin.weights.z
                + deformed(
                    was_fourth.col0,
                    was_fourth.col1,
                    was_fourth.col2,
                    was_fourth.col3,
                    rest,
                ) * skin.weights.w;
            carried = fvec4(was.x, was.y, was.z, 1.0);
        }
        let current_world = held.model0 * local.x
            + held.model1 * local.y
            + held.model2 * local.z
            + held.model3 * local.w;
        let current_clip = held.clip0 * local.x
            + held.clip1 * local.y
            + held.clip2 * local.z
            + held.clip3 * local.w;
        let previous_world = held.previous0 * carried.x
            + held.previous1 * carried.y
            + held.previous2 * carried.z
            + held.previous3 * carried.w;
        let previous_clip = camera.previous0 * previous_world.x
            + camera.previous1 * previous_world.y
            + camera.previous2 * previous_world.z
            + camera.previous3 * previous_world.w;
        let current_inverse = 1.0 / max(current_clip.w, 0.0001);
        let previous_inverse = 1.0 / max(previous_clip.w, 0.0001);
        let reach = (current_world.x - camera.eye.x) * camera.forward.x
            + (current_world.y - camera.eye.y) * camera.forward.y
            + (current_world.z - camera.eye.z) * camera.forward.z;
        position = current_clip;
        surface = fvec4(
            vertices[base + 6u32],
            vertices[base + 7u32],
            held.material as f32,
            reach,
        );
        travel = fvec4(
            current_clip.x * current_inverse * 0.5 + 0.5,
            current_clip.y * current_inverse * camera.vertical + 0.5,
            previous_clip.x * previous_inverse * 0.5 + 0.5,
            previous_clip.y * previous_inverse * camera.vertical + 0.5,
        );
    }

    #[fragment]
    fn fragment(
        textures: TextureTable,
        samplers: SamplerTable,
        shaped: In<Fvec4>,
        moved: In<Fvec4>,
        target: Out<Fvec4>,
    ) {
        let entry = materials[shaped.z as u32];
        if entry.alpha_mode == 1u32 {
            let sampled = sample(
                textures[entry.base_color_image],
                samplers[entry.base_color_sampler],
                fvec2(shaped.x, shaped.y),
            );
            if sampled.w * entry.base_color.w < entry.alpha_cutoff {
                discard();
            }
        }
        target = fvec4(moved.z - moved.x, moved.w - moved.y, shaped.w, 1.0);
    }
}
pub(crate) fn motion_vectors() -> GraphicsProgram {
    motion()
}

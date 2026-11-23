use super::*;

#[dome_gpu::graphics]
mod transparency {
    #[binding]
    fn settings(settings: Read<TransparentSetupRecord>) {}

    #[binding]
    fn keys(keys: Read<u32>) {}

    #[binding]
    fn colors(colors: Read<u32>) {}
    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let setup = settings[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * setup.vertical + 0.5);
    }

    #[fragment]
    fn fragment(source: Texture2D, filter: Sampler, coverage: In<Fvec2>, target: Out<Fvec4>) {
        let setup = settings[0u32];
        let at = coverage;
        let width = setup.width as f32;
        let height = setup.height as f32;
        let pixel_x = min(max(floor(at.x * width), 0.0), width - 1.0) as u32;
        let pixel_y = min(max(floor(at.y * height), 0.0), height - 1.0) as u32;
        let pixel = pixel_y * setup.width + pixel_x;
        let held = pixel * setup.layers;
        let slot0 = keys[held];
        let slot1 = keys[held + 1u32];
        let slot2 = keys[held + 2u32];
        let slot3 = keys[held + 3u32];
        let filled0 = slot0 > 1u32;
        let filled1 = slot1 > 1u32;
        let filled2 = slot2 > 1u32;
        let filled3 = slot3 > 1u32;
        let mut color0 = select(0u32, colors[held], filled0);
        let mut depth0 = select(0u32, slot0, filled0);
        let mut color1 = select(0u32, colors[held + 1u32], filled1);
        let mut depth1 = select(0u32, slot1, filled1);
        let mut color2 = select(0u32, colors[held + 2u32], filled2);
        let mut depth2 = select(0u32, slot2, filled2);
        let mut color3 = select(0u32, colors[held + 3u32], filled3);
        let mut depth3 = select(0u32, slot3, filled3);
        let ordering01 = depth1 < depth0;
        let low_color01 = select(color0, color1, ordering01);
        let low_depth01 = select(depth0, depth1, ordering01);
        color1 = select(color1, color0, ordering01);
        depth1 = select(depth1, depth0, ordering01);
        color0 = low_color01;
        depth0 = low_depth01;
        let ordering23 = depth3 < depth2;
        let low_color23 = select(color2, color3, ordering23);
        let low_depth23 = select(depth2, depth3, ordering23);
        color3 = select(color3, color2, ordering23);
        depth3 = select(depth3, depth2, ordering23);
        color2 = low_color23;
        depth2 = low_depth23;
        let ordering02 = depth2 < depth0;
        let low_color02 = select(color0, color2, ordering02);
        let low_depth02 = select(depth0, depth2, ordering02);
        color2 = select(color2, color0, ordering02);
        depth2 = select(depth2, depth0, ordering02);
        color0 = low_color02;
        depth0 = low_depth02;
        let ordering13 = depth3 < depth1;
        let low_color13 = select(color1, color3, ordering13);
        let low_depth13 = select(depth1, depth3, ordering13);
        color3 = select(color3, color1, ordering13);
        depth3 = select(depth3, depth1, ordering13);
        color1 = low_color13;
        depth1 = low_depth13;
        let ordering12 = depth2 < depth1;
        let low_color12 = select(color1, color2, ordering12);
        let low_depth12 = select(depth1, depth2, ordering12);
        color2 = select(color2, color1, ordering12);
        depth2 = select(depth2, depth1, ordering12);
        color1 = low_color12;
        depth1 = low_depth12;
        let first = (depth0 & 255u32) as f32 * (1.0 / 255.0);
        let second = (depth1 & 255u32) as f32 * (1.0 / 255.0);
        let third = (depth2 & 255u32) as f32 * (1.0 / 255.0);
        let fourth = (depth3 & 255u32) as f32 * (1.0 / 255.0);
        let through_first = 1.0 - first;
        let through_second = through_first * (1.0 - second);
        let through_third = through_second * (1.0 - third);
        let transmit = through_third * (1.0 - fourth);
        let weight0 = first;
        let weight1 = second * through_first;
        let weight2 = third * through_second;
        let weight3 = fourth * through_third;
        let base = sample(source, filter, at);
        target = fvec4(
            decoded(color0, 16u32) * weight0
                + decoded(color1, 16u32) * weight1
                + decoded(color2, 16u32) * weight2
                + decoded(color3, 16u32) * weight3
                + base.x * transmit,
            decoded(color0, 8u32) * weight0
                + decoded(color1, 8u32) * weight1
                + decoded(color2, 8u32) * weight2
                + decoded(color3, 8u32) * weight3
                + base.y * transmit,
            decoded(color0, 0u32) * weight0
                + decoded(color1, 0u32) * weight1
                + decoded(color2, 0u32) * weight2
                + decoded(color3, 0u32) * weight3
                + base.z * transmit,
            base.w * transmit,
        );
    }

    fn decoded(color: u32, shift: u32) -> f32 {
        let exponent = color >> 24u32;
        let scale = select(
            0.0,
            bitcast_f32((exponent - 1u32) << 23u32),
            exponent > 1u32,
        ) * (1.0 / 255.0);
        ((color >> shift) & 255u32) as f32 * scale
    }
}
pub(crate) fn transparency_resolve() -> GraphicsProgram {
    transparency()
}
#[dome_gpu::graphics]
mod bloom_extraction {
    #[binding]
    fn settings(settings: Read<BloomRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let setup = settings[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * setup.vertical + 0.5);
    }

    #[fragment]
    fn fragment(source: Texture2D, filter: Sampler, coverage: In<Fvec2>, target: Out<Fvec4>) {
        let setup = settings[0u32];
        let step = fvec2(1.0 / setup.source_width, 1.0 / setup.source_height);
        let at = coverage;
        let a = tap(source, filter, at, step, -2.0, 2.0);
        let b = tap(source, filter, at, step, 0.0, 2.0);
        let c = tap(source, filter, at, step, 2.0, 2.0);
        let d = tap(source, filter, at, step, -2.0, 0.0);
        let e = tap(source, filter, at, step, 0.0, 0.0);
        let f = tap(source, filter, at, step, 2.0, 0.0);
        let g = tap(source, filter, at, step, -2.0, -2.0);
        let h = tap(source, filter, at, step, 0.0, -2.0);
        let i = tap(source, filter, at, step, 2.0, -2.0);
        let j = tap(source, filter, at, step, -1.0, 1.0);
        let k = tap(source, filter, at, step, 1.0, 1.0);
        let l = tap(source, filter, at, step, -1.0, -1.0);
        let m = tap(source, filter, at, step, 1.0, -1.0);
        let gathered = karis(d, e, i, j) * 0.5
            + karis(a, b, f, g) * 0.125
            + karis(b, c, g, h) * 0.125
            + karis(f, g, k, l) * 0.125
            + karis(g, h, l, m) * 0.125;
        target = thresholded(gathered, setup.threshold, setup.knee);
    }

    fn tap(source: Texture2D, filter: Sampler, at: Fvec2, step: Fvec2, x: f32, y: f32) -> Fvec4 {
        sample(source, filter, fvec2(at.x + step.x * x, at.y + step.y * y))
    }

    fn karis(a: Fvec4, b: Fvec4, c: Fvec4, d: Fvec4) -> Fvec4 {
        let wa = 1.0 / (1.0 + luma(a));
        let wb = 1.0 / (1.0 + luma(b));
        let wc = 1.0 / (1.0 + luma(c));
        let wd = 1.0 / (1.0 + luma(d));
        let total = wa + wb + wc + wd;
        fvec4(
            (a.x * wa + b.x * wb + c.x * wc + d.x * wd) / total,
            (a.y * wa + b.y * wb + c.y * wc + d.y * wd) / total,
            (a.z * wa + b.z * wb + c.z * wc + d.z * wd) / total,
            1.0,
        )
    }

    fn luma(value: Fvec4) -> f32 {
        0.2126 * value.x + 0.7152 * value.y + 0.0722 * value.z
    }

    fn thresholded(value: Fvec4, threshold: f32, knee: f32) -> Fvec4 {
        let brightness = max(value.x, max(value.y, value.z));
        let soft = clamped(brightness - threshold + knee, 0.0, 2.0 * knee);
        let softened = soft * soft / (4.0 * knee + 0.00001);
        let contribution = max(softened, brightness - threshold) / max(brightness, 0.00001);
        value * contribution
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }
}
#[dome_gpu::graphics]
mod bloom_downsampling {
    #[binding]
    fn settings(settings: Read<BloomRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let setup = settings[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * setup.vertical + 0.5);
    }

    #[fragment]
    fn fragment(source: Texture2D, filter: Sampler, coverage: In<Fvec2>, target: Out<Fvec4>) {
        let setup = settings[0u32];
        let step = fvec2(1.0 / setup.source_width, 1.0 / setup.source_height);
        let at = coverage;
        let a = tap(source, filter, at, step, -2.0, 2.0);
        let b = tap(source, filter, at, step, 0.0, 2.0);
        let c = tap(source, filter, at, step, 2.0, 2.0);
        let d = tap(source, filter, at, step, -2.0, 0.0);
        let e = tap(source, filter, at, step, 0.0, 0.0);
        let f = tap(source, filter, at, step, 2.0, 0.0);
        let g = tap(source, filter, at, step, -2.0, -2.0);
        let h = tap(source, filter, at, step, 0.0, -2.0);
        let i = tap(source, filter, at, step, 2.0, -2.0);
        let j = tap(source, filter, at, step, -1.0, 1.0);
        let k = tap(source, filter, at, step, 1.0, 1.0);
        let l = tap(source, filter, at, step, -1.0, -1.0);
        let m = tap(source, filter, at, step, 1.0, -1.0);
        target = e * 0.125
            + (a + c + g + i) * 0.03125
            + (b + d + f + h) * 0.0625
            + (j + k + l + m) * 0.125;
    }

    fn tap(source: Texture2D, filter: Sampler, at: Fvec2, step: Fvec2, x: f32, y: f32) -> Fvec4 {
        sample(source, filter, fvec2(at.x + step.x * x, at.y + step.y * y))
    }
}
#[dome_gpu::graphics]
mod bloom_upsampling {
    #[binding]
    fn settings(settings: Read<BloomUpRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let setup = settings[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * setup.vertical + 0.5);
    }

    #[fragment]
    fn fragment(
        low: Texture2D,
        high: Texture2D,
        filter: Sampler,
        coverage: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let setup = settings[0u32];
        let step = fvec2(1.0 / setup.source_width, 1.0 / setup.source_height);
        let at = coverage;
        let a = tap(low, filter, at, step, -1.0, -1.0);
        let b = tap(low, filter, at, step, 0.0, -1.0);
        let c = tap(low, filter, at, step, 1.0, -1.0);
        let d = tap(low, filter, at, step, -1.0, 0.0);
        let e = tap(low, filter, at, step, 0.0, 0.0);
        let f = tap(low, filter, at, step, 1.0, 0.0);
        let g = tap(low, filter, at, step, -1.0, 1.0);
        let h = tap(low, filter, at, step, 0.0, 1.0);
        let i = tap(low, filter, at, step, 1.0, 1.0);
        let tent = (a + c + g + i) * 0.0625 + (b + d + f + h) * 0.125 + e * 0.25;
        target = tent + sample(high, filter, at);
    }

    fn tap(source: Texture2D, filter: Sampler, at: Fvec2, step: Fvec2, x: f32, y: f32) -> Fvec4 {
        sample(source, filter, fvec2(at.x + step.x * x, at.y + step.y * y))
    }
}
#[dome_gpu::graphics]
mod tone_mapping {
    #[binding]
    fn settings(settings: Read<TonemapRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let setup = settings[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * setup.vertical + 0.5);
    }

    #[fragment]
    fn fragment(
        lit: Texture2D,
        bloom: Texture2D,
        filter: Sampler,
        coverage: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let setup = settings[0u32];
        let at = coverage;
        let scene = sample(lit, filter, at);
        let glow = sample(bloom, filter, at);
        let held = fvec4(
            scene.x + glow.x * setup.intensity,
            scene.y + glow.y * setup.intensity,
            scene.z + glow.z * setup.intensity,
            1.0,
        );
        let neutral = pbr_neutral(held);
        let step = fvec2(1.0 / setup.source_width, 1.0 / setup.source_height);
        let left = sample(lit, filter, fvec2(at.x - 2.0 * step.x, at.y));
        let right = sample(lit, filter, fvec2(at.x + 2.0 * step.x, at.y));
        let down = sample(lit, filter, fvec2(at.x, at.y - 2.0 * step.y));
        let up = sample(lit, filter, fvec2(at.x, at.y + 2.0 * step.y));
        let near = max(max(left.w, right.w), max(down.w, up.w));
        let outlined = select(0.0, 1.0, near > 0.5 && scene.w < 0.5);
        let shaded = fvec4(
            encoded(neutral.x),
            encoded(neutral.y),
            encoded(neutral.z),
            1.0,
        );
        let outline = fvec4(0.30, 0.62, 1.0, 1.0);
        target = fvec4(
            shaded.x + (outline.x - shaded.x) * outlined,
            shaded.y + (outline.y - shaded.y) * outlined,
            shaded.z + (outline.z - shaded.z) * outlined,
            1.0,
        );
    }

    fn pbr_neutral(colour: Fvec4) -> Fvec4 {
        let start = 0.76;
        let desaturation = 0.15;
        let lowest = min(colour.x, min(colour.y, colour.z));
        let offset = select(0.04, lowest - 6.25 * lowest * lowest, lowest < 0.08);
        let shifted = fvec4(colour.x - offset, colour.y - offset, colour.z - offset, 1.0);
        let peak = max(shifted.x, max(shifted.y, shifted.z));
        if peak < start {
            return shifted;
        }
        let headroom = 1.0 - start;
        let compressed = 1.0 - headroom * headroom / (peak + headroom - start);
        let scale = compressed / peak;
        let grey = 1.0 - 1.0 / (desaturation * (peak - compressed) + 1.0);
        fvec4(
            shifted.x * scale + (compressed - shifted.x * scale) * grey,
            shifted.y * scale + (compressed - shifted.y * scale) * grey,
            shifted.z * scale + (compressed - shifted.z * scale) * grey,
            1.0,
        )
    }

    fn encoded(value: f32) -> f32 {
        let held = max(value, 0.0);
        select(
            1.055 * pow(held, 0.41666666) - 0.055,
            held * 12.92,
            held <= 0.0031308,
        )
    }
}
#[dome_gpu::graphics]
mod sky {
    #[binding]
    fn view(view: Read<SkyRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, screen: Out<Fvec2>) {
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 1.0, 1.0);
        screen = fvec2(x, y);
    }

    #[fragment]
    fn fragment(radiance: Texture2D, sampler: Sampler, at: In<Fvec2>, target: Out<Fvec4>) {
        let held = view[0u32];
        let ray = at;
        let far = held.inverse0 * ray.x + held.inverse1 * ray.y + held.inverse2 + held.inverse3;
        let at = fvec4(far.x / far.w, far.y / far.w, far.z / far.w, 1.0) - held.eye;
        let unit = turned(
            held.environment0,
            held.environment1,
            held.environment2,
            normalized(at),
        );
        let held_radiance = sample_lod(
            radiance,
            sampler,
            fvec2(
                atan2(unit.x, -unit.z) * (113.0 / 710.0) + 0.5,
                acos(clamped(unit.y, -1.0, 1.0)) * (113.0 / 355.0),
            ),
            0.0,
        );
        target = fvec4(
            held_radiance.x * held.intensity,
            held_radiance.y * held.intensity,
            held_radiance.z * held.intensity,
            0.0,
        );
    }

    fn normalized(vector: Fvec4) -> Fvec4 {
        let inverse =
            1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
        vector * inverse
    }

    fn turned(first: Fvec4, second: Fvec4, third: Fvec4, vector: Fvec4) -> Fvec4 {
        fvec4(
            first.x * vector.x + second.x * vector.y + third.x * vector.z,
            first.y * vector.x + second.y * vector.y + third.y * vector.z,
            first.z * vector.x + second.z * vector.y + third.z * vector.z,
            0.0,
        )
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }
}
pub(crate) fn sky_program() -> GraphicsProgram {
    sky()
}
pub(crate) fn bloom_extract() -> GraphicsProgram {
    bloom_extraction()
}
pub(crate) fn bloom_down() -> GraphicsProgram {
    bloom_downsampling()
}
pub(crate) fn bloom_up() -> GraphicsProgram {
    bloom_upsampling()
}
pub(crate) fn tonemap() -> GraphicsProgram {
    tone_mapping()
}
#[dome_gpu::graphics]
mod temporal {
    #[binding]
    fn frame(frame: Read<TemporalRecord>) {}

    #[binding]
    fn motion(motion: Texture2D, previous: Texture2D, nearest: Sampler) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let held = frame[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * held.vertical + 0.5);
    }

    #[fragment]
    fn fragment(
        source: Texture2D,
        history: Texture2D,
        filter: Sampler,
        coverage: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let setup = frame[0u32];
        let at = coverage;
        let recorded = sample(motion, nearest, at);
        let sky = recorded.w < 0.5;
        let mut reach = setup.far;
        if !sky {
            reach = recorded.z;
        }
        let mut history_at = fvec2(at.x + recorded.x, at.y + recorded.y);
        let ndc_x = at.x * 2.0 - 1.0;
        let ndc_y = (at.y - 0.5) / setup.vertical;
        let head = column(
            setup.inverse0,
            setup.inverse1,
            setup.inverse2,
            setup.inverse3,
            fvec4(ndc_x, ndc_y, 0.0, 1.0),
        );
        let tail = column(
            setup.inverse0,
            setup.inverse1,
            setup.inverse2,
            setup.inverse3,
            fvec4(ndc_x, ndc_y, 1.0, 1.0),
        );
        let head_point = fvec4(head.x / head.w, head.y / head.w, head.z / head.w, 1.0);
        let tail_point = fvec4(tail.x / tail.w, tail.y / tail.w, tail.z / tail.w, 1.0);
        let ray = normalized(tail_point - head_point);
        let mut travel = setup.far;
        if !sky {
            travel = reach / max(dot3(ray, setup.forward), 0.0001);
        }
        let world = fvec4(
            setup.eye.x + ray.x * travel,
            setup.eye.y + ray.y * travel,
            setup.eye.z + ray.z * travel,
            1.0,
        );
        let previous_clip = column(
            setup.previous0,
            setup.previous1,
            setup.previous2,
            setup.previous3,
            world,
        );
        let previous_inverse = 1.0 / max(previous_clip.w, 0.0001);
        let previous_at = fvec2(
            previous_clip.x * previous_inverse * 0.5 + 0.5,
            previous_clip.y * previous_inverse * setup.vertical + 0.5,
        );
        if sky {
            history_at = previous_at;
        }
        let previous_reach = (world.x - setup.previous_eye.x) * setup.previous_forward.x
            + (world.y - setup.previous_eye.y) * setup.previous_forward.y
            + (world.z - setup.previous_eye.z) * setup.previous_forward.z;
        let carried = sample(previous, nearest, history_at);
        let mut valid = history_at.x > 0.0
            && history_at.x < 1.0
            && history_at.y > 0.0
            && history_at.y < 1.0
            && previous_clip.w > 0.0001;
        if carried.w > 0.5 {
            valid = valid
                && abs(carried.z - previous_reach)
                    <= setup.tolerance * max(previous_reach, setup.near);
        } else {
            valid = valid && sky;
        }
        let current = sample(source, filter, at);
        let held = sample(history, filter, history_at);
        let step = fvec2(1.0 / setup.width, 1.0 / setup.height);
        let left = sample(source, filter, fvec2(at.x - step.x, at.y));
        let right = sample(source, filter, fvec2(at.x + step.x, at.y));
        let below = sample(source, filter, fvec2(at.x, at.y - step.y));
        let above = sample(source, filter, fvec2(at.x, at.y + step.y));
        let shade_low = lowest(lowest(lowest(left, right), lowest(below, above)), current);
        let shade_high = highest(
            highest(highest(left, right), highest(below, above)),
            current,
        );
        let bounded = fvec4(
            clamped(held.x, shade_low.x, shade_high.x),
            clamped(held.y, shade_low.y, shade_high.y),
            clamped(held.z, shade_low.z, shade_high.z),
            1.0,
        );
        let weight = select(0.0, setup.blend, valid);
        target = fvec4(
            bounded.x * weight + current.x * (1.0 - weight),
            bounded.y * weight + current.y * (1.0 - weight),
            bounded.z * weight + current.z * (1.0 - weight),
            current.w,
        );
    }

    fn column(a: Fvec4, b: Fvec4, c: Fvec4, d: Fvec4, local: Fvec4) -> Fvec4 {
        a * local.x + b * local.y + c * local.z + d * local.w
    }

    fn lowest(left: Fvec4, right: Fvec4) -> Fvec4 {
        fvec4(
            min(left.x, right.x),
            min(left.y, right.y),
            min(left.z, right.z),
            0.0,
        )
    }

    fn highest(left: Fvec4, right: Fvec4) -> Fvec4 {
        fvec4(
            max(left.x, right.x),
            max(left.y, right.y),
            max(left.z, right.z),
            0.0,
        )
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }

    fn dot3(left: Fvec4, right: Fvec4) -> f32 {
        left.x * right.x + left.y * right.y + left.z * right.z
    }

    fn normalized(vector: Fvec4) -> Fvec4 {
        let inverse =
            1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
        vector * inverse
    }
}
pub(crate) fn temporal_resolve() -> GraphicsProgram {
    temporal()
}

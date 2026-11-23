use super::*;

#[dome_shader::graphics]
mod basis {
    #[binding]
    fn step(step: Read<HzbStepRecord>) {}

    #[binding]
    fn depths(depths: ReadWrite<f32>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let held = step[0u32];
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
    fn fragment(source: Texture2D, filter: Sampler, coverage: In<Fvec2>, target: Out<Fvec4>) {
        let at = coverage;
        let held = step[0u32];
        let column = floor(at.x * held.target_width);
        let row = floor(at.y * held.target_height);
        let mut deepest = 0.0;
        for y in stride(0u32, 4u32, 1u32) {
            for x in stride(0u32, 4u32, 1u32) {
                let u = clamped(
                    (column * 4.0 + x as f32 + 0.5) / held.source_width,
                    0.5 / held.source_width,
                    1.0 - 0.5 / held.source_width,
                );
                let v = clamped(
                    (row * 4.0 + y as f32 + 0.5) / held.source_height,
                    0.5 / held.source_height,
                    1.0 - 0.5 / held.source_height,
                );
                deepest = max(deepest, sample(source, filter, fvec2(u, v)).x);
            }
        }
        depths[held.target_offset + row as u32 * held.target_stride + column as u32] = deepest;
        target = fvec4(deepest, 0.0, 0.0, 1.0);
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }
}
#[dome_shader::graphics]
mod reduction {
    #[binding]
    fn step(step: Read<HzbStepRecord>) {}

    #[binding]
    fn depths(depths: ReadWrite<f32>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let held = step[0u32];
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
    fn fragment(coverage: In<Fvec2>, target: Out<Fvec4>) {
        let at = coverage;
        let held = step[0u32];
        let column = floor(at.x * held.target_width) as u32;
        let row = floor(at.y * held.target_height) as u32;
        let last_column = held.source_width as u32 - 1u32;
        let last_row = held.source_height as u32 - 1u32;
        let mut deepest = 0.0;
        for y in stride(0u32, 2u32, 1u32) {
            for x in stride(0u32, 2u32, 1u32) {
                let source_column = min(column * 2u32 + x, last_column);
                let source_row = min(row * 2u32 + y, last_row);
                deepest = max(
                    deepest,
                    depths[held.source_offset + source_row * held.source_stride + source_column],
                );
            }
        }
        depths[held.target_offset + row * held.target_stride + column] = deepest;
        target = fvec4(deepest, 0.0, 0.0, 1.0);
    }
}
pub(crate) fn hzb_base() -> GraphicsProgram {
    basis()
}
pub(crate) fn hzb_reduce() -> GraphicsProgram {
    reduction()
}

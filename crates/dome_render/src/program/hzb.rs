use super::*;

#[dome_shader::graphics]
mod base {
    #[binding]
    fn orientation(orientation: Read<f32>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec4>) {
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec4(x * 0.5 + 0.5, y * orientation[0u32] + 0.5, 0.0, 0.0);
    }

    #[fragment]
    fn fragment(depth: SampledDepth, filter: Sampler, coverage: In<Fvec4>, target: Out<Fvec4>) {
        let at = fvec2(coverage.x, coverage.y);
        let step = dpdx(coverage);
        let mut deepest = 0.0;
        for y in stride(0u32, 4u32, 1u32) {
            for x in stride(0u32, 4u32, 1u32) {
                let offset = fvec2(
                    ((x as f32 + 0.5) / 4.0 - 0.5) * step.x,
                    ((y as f32 + 0.5) / 4.0 - 0.5) * step.y,
                );
                deepest = max(
                    deepest,
                    sample_depth(depth, filter, fvec2(at.x + offset.x, at.y + offset.y)),
                );
            }
        }
        target = fvec4(deepest, 0.0, 0.0, 1.0);
    }
}

#[dome_shader::graphics]
mod reduction {
    #[binding]
    fn orientation(orientation: Read<f32>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec4>) {
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec4(x * 0.5 + 0.5, y * orientation[0u32] + 0.5, 0.0, 0.0);
    }

    #[fragment]
    fn fragment(source: Texture2D, filter: Sampler, coverage: In<Fvec4>, target: Out<Fvec4>) {
        let at = fvec2(coverage.x, coverage.y);
        let step = dpdx(coverage);
        let mut deepest = 0.0;
        for y in stride(0u32, 2u32, 1u32) {
            for x in stride(0u32, 2u32, 1u32) {
                let offset = fvec2(
                    ((x as f32 + 0.5) / 2.0 - 0.5) * step.x,
                    ((y as f32 + 0.5) / 2.0 - 0.5) * step.y,
                );
                deepest = max(
                    deepest,
                    sample(source, filter, fvec2(at.x + offset.x, at.y + offset.y)).x,
                );
            }
        }
        target = fvec4(deepest, 0.0, 0.0, 1.0);
    }
}

pub(crate) fn hzb_base() -> GraphicsProgram {
    base()
}
pub(crate) fn hzb_reduce() -> GraphicsProgram {
    reduction()
}

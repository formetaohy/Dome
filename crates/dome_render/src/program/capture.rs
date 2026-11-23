use super::*;

#[dome_shader::graphics]
mod capture_atlas {
    #[binding]
    fn setup(setup: Read<AtlasRecord>) {}

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
    fn fragment(atlas: Texture2D, sampler: Sampler, at: In<Fvec2>, target: Out<Fvec4>) {
        let held = setup[0u32];
        let direction = ray(fvec2(at.x * 0.5 + 0.5, at.y * held.vertical + 0.5));
        let lit = sample_lod(
            atlas,
            sampler,
            face_of(direction, held.face, held.texel),
            0.0,
        );
        target = fvec4(lit.x, lit.y, lit.z, 1.0);
    }

    fn face_of(direction: Fvec4, face: f32, texel: f32) -> Fvec2 {
        let unit = normalized(direction);
        let across = abs(unit.x);
        let up = abs(unit.y);
        let forward = abs(unit.z);
        let mut index = 0.0;
        let mut horizontal = -unit.z;
        let mut vertical = -unit.y;
        let mut major = across;
        if across >= up && across >= forward {
            if unit.x < 0.0 {
                index = 1.0;
                horizontal = unit.z;
            }
        } else if up >= forward {
            major = up;
            index = 2.0;
            horizontal = unit.x;
            vertical = unit.z;
            if unit.y < 0.0 {
                index = 3.0;
                vertical = -unit.z;
            }
        } else {
            major = forward;
            index = 4.0;
            horizontal = unit.x;
            vertical = -unit.y;
            if unit.z < 0.0 {
                index = 5.0;
                horizontal = -unit.x;
            }
        }
        let span = max(major, 0.000001);
        let left = 0.5 + 0.5 * horizontal / span;
        let top = 0.5 - 0.5 * vertical / span;
        fvec2(
            (index + clamped(left, texel, 1.0 - texel)) * face,
            clamped(top, texel, 1.0 - texel),
        )
    }

    fn ray(at: Fvec2) -> Fvec4 {
        let phi = (at.x - 0.5) * (710.0 / 113.0);
        let theta = at.y * (355.0 / 113.0);
        let sine = sin(theta);
        fvec4(sine * sin(phi), cos(theta), -sine * cos(phi), 0.0)
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
pub(crate) fn capture_conversion() -> GraphicsProgram {
    capture_atlas()
}

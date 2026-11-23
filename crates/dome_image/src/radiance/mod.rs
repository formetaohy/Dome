mod header;
mod scanline;

pub struct Radiance {
    width: u32,
    height: u32,
    rgb: Vec<f32>,
}

impl Radiance {
    pub fn decode(bytes: &[u8], what: &str) -> Self {
        let (exposure, width, height, body) = header::header(bytes, what);
        let texels = scanline::decode(body, width, height, what);
        let scale = 1.0 / exposure as f32;
        let mut rgb = Vec::with_capacity(texels.len() * 3);
        for texel in texels {
            rgb.extend_from_slice(&[texel[0] * scale, texel[1] * scale, texel[2] * scale]);
        }
        Self { width, height, rgb }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgb(&self) -> &[f32] {
        &self.rgb
    }
}

pub(crate) fn value(texel: [u8; 4]) -> [f32; 3] {
    if texel[3] == 0 {
        return [0.0, 0.0, 0.0];
    }
    let scale = (texel[3] as f32 - 136.0).exp2();
    [
        texel[0] as f32 * scale,
        texel[1] as f32 * scale,
        texel[2] as f32 * scale,
    ]
}

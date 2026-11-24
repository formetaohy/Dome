use dome_gpu::{Texture, TextureFormat, TextureUsages};
use dome_math::Vec3;
use half::f16;

pub struct Panorama {
    width: u32,
    height: u32,
    texels: Vec<Vec3>,
}

impl Panorama {
    pub fn new(width: u32, height: u32, rgb: &[f32]) -> Self {
        assert!(
            width > 0 && height > 0,
            "a panorama spans no texel of its {width} by {height} picture",
        );
        assert_eq!(
            rgb.len() as u64,
            u64::from(width) * u64::from(height) * 3,
            "a {width} by {height} panorama holds {} channels where its picture holds {}",
            rgb.len(),
            u64::from(width) * u64::from(height) * 3,
        );
        Self {
            width,
            height,
            texels: rgb
                .as_chunks::<3>()
                .0
                .iter()
                .map(|texel| Vec3::new(texel[0], texel[1], texel[2]))
                .collect(),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn texel(&self, x: u32, y: u32) -> Vec3 {
        self.texels[(y * self.width + x) as usize]
    }

    pub fn texture(&self, renderer: &crate::Renderer, label: &str) -> Texture {
        let texture = renderer.texture(
            label,
            self.width,
            self.height,
            1,
            TextureFormat::Rgba16Float,
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        );
        let one = f16::from_f32(1.0).to_bits();
        let mut halves = Vec::with_capacity(self.texels.len() * 4);
        for texel in &self.texels {
            halves.push(half(texel.x));
            halves.push(half(texel.y));
            halves.push(half(texel.z));
            halves.push(one);
        }
        renderer.write_texture(&texture, &[bytemuck::cast_slice(&halves)]);
        texture
    }
}

fn half(value: f32) -> u16 {
    assert!(
        value.is_finite(),
        "a panorama holds the texel channel {value}"
    );
    let maximum = f16::MAX.to_f32();
    f16::from_f32(value.clamp(-maximum, maximum)).to_bits()
}

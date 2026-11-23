use std::sync::LazyLock;

const ENCODE_STEPS: usize = 1 << 16;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ColorSpace {
    Srgb,
    Linear,
}

pub fn mip_chain(width: u32, height: u32, color: ColorSpace, pixels: &[u8]) -> Vec<Vec<u8>> {
    assert!(
        width > 0 && height > 0,
        "a {width} by {height} image spans no texel",
    );
    assert_eq!(
        pixels.len() as u64,
        u64::from(width) * u64::from(height) * 4,
        "an image of {width} by {height} texels spans {} bytes",
        u64::from(width) * u64::from(height) * 4,
    );
    let mut levels = vec![pixels.to_vec()];
    let mut held_width = width;
    let mut held_height = height;
    while held_width > 1 || held_height > 1 {
        let next_width = (held_width / 2).max(1);
        let next_height = (held_height / 2).max(1);
        let previous = levels
            .last()
            .expect("a mip chain holds the level it downsamples");
        levels.push(downsample(
            color,
            held_width,
            held_height,
            previous,
            next_width,
            next_height,
        ));
        held_width = next_width;
        held_height = next_height;
    }
    levels
}

fn downsample(
    color: ColorSpace,
    width: u32,
    height: u32,
    pixels: &[u8],
    next_width: u32,
    next_height: u32,
) -> Vec<u8> {
    let mut next = vec![0u8; next_width as usize * next_height as usize * 4];
    for y in 0..next_height {
        let first_y = (y * 2).min(height - 1) as usize;
        let second_y = (y * 2 + 1).min(height - 1) as usize;
        for x in 0..next_width {
            let first_x = (x * 2).min(width - 1) as usize;
            let second_x = (x * 2 + 1).min(width - 1) as usize;
            let taps = [
                tap(pixels, width, first_x, first_y),
                tap(pixels, width, second_x, first_y),
                tap(pixels, width, first_x, second_y),
                tap(pixels, width, second_x, second_y),
            ];
            let at = (y as usize * next_width as usize + x as usize) * 4;
            let texel = match color {
                ColorSpace::Linear => linear_texel(&taps),
                ColorSpace::Srgb => srgb_texel(&taps),
            };
            next[at..at + 4].copy_from_slice(&texel);
        }
    }
    next
}

fn tap(pixels: &[u8], width: u32, x: usize, y: usize) -> [u8; 4] {
    let at = (y * width as usize + x) * 4;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

fn linear_texel(taps: &[[u8; 4]]) -> [u8; 4] {
    let mut texel = [0u8; 4];
    for channel in 0..4 {
        let sum = taps.iter().map(|tap| u32::from(tap[channel])).sum::<u32>();
        texel[channel] = ((sum + 2) / 4) as u8;
    }
    texel
}

fn srgb_texel(taps: &[[u8; 4]]) -> [u8; 4] {
    let alpha = taps[0][3];
    if taps.iter().all(|tap| tap[3] == alpha) {
        let mut texel = [0u8, 0u8, 0u8, alpha];
        for channel in 0..3 {
            let sum = taps.iter().map(|tap| decode(tap[channel])).sum::<f32>();
            texel[channel] = encode(sum / 4.0);
        }
        return texel;
    }
    let coverage = taps
        .iter()
        .map(|tap| f32::from(tap[3]) / 255.0)
        .sum::<f32>();
    let mut texel = [0u8, 0u8, 0u8, (coverage / 4.0 * 255.0).round() as u8];
    for channel in 0..3 {
        let weighted = taps
            .iter()
            .map(|tap| decode(tap[channel]) * f32::from(tap[3]) / 255.0)
            .sum::<f32>();
        texel[channel] = encode(weighted / coverage);
    }
    texel
}

fn decode(value: u8) -> f32 {
    static TABLE: LazyLock<[f32; 256]> = LazyLock::new(|| {
        let mut table = [0.0f32; 256];
        for (index, entry) in table.iter_mut().enumerate() {
            let value = index as f32 / 255.0;
            *entry = if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            };
        }
        table
    });
    TABLE[value as usize]
}

fn encode(value: f32) -> u8 {
    static TABLE: LazyLock<Vec<u8>> = LazyLock::new(|| {
        (0..ENCODE_STEPS)
            .map(|index| {
                let value = index as f32 / (ENCODE_STEPS - 1) as f32;
                let encoded = if value <= 0.003_130_8 {
                    value * 12.92
                } else {
                    1.055 * value.powf(1.0 / 2.4) - 0.055
                };
                (encoded * 255.0).round().clamp(0.0, 255.0) as u8
            })
            .collect()
    });
    let index = (value.clamp(0.0, 1.0) * (ENCODE_STEPS - 1) as f32).round() as usize;
    TABLE[index]
}

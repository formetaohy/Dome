use crate::png::header::{Color, Header};
use crate::png::pass::{self, Layout, Pass};

struct Key {
    gray: Option<u16>,
    rgb: Option<[u16; 3]>,
    alpha: Vec<u8>,
    colors: Vec<[u8; 3]>,
}

pub(crate) fn expand(
    header: &Header,
    palette: Option<Vec<[u8; 3]>>,
    transparency: Option<Vec<u8>>,
    raw: &mut [u8],
    layout: &Layout,
    texels: usize,
    what: &str,
) -> Result<Vec<u8>, String> {
    let key = key(header, palette, transparency, what)?;
    let pixels = pass::unfilter(raw, layout, what)?;
    let channels = header.color.channels();
    let mut out = vec![0u8; texels * 4];
    for (pass, data) in layout.passes.iter().zip(&pixels) {
        paint(header, &key, pass, data, channels, &mut out, what)?;
    }
    Ok(out)
}

fn key(
    header: &Header,
    palette: Option<Vec<[u8; 3]>>,
    transparency: Option<Vec<u8>>,
    what: &str,
) -> Result<Key, String> {
    let colors = match header.color {
        Color::Palette => {
            palette.ok_or_else(|| format!("{what} holds a palette image without a palette"))?
        }
        _ => palette.unwrap_or_default(),
    };
    match header.color {
        Color::Gray => {
            let gray = match transparency {
                Some(bytes) => {
                    if bytes.len() != 2 {
                        return Err(format!(
                            "{what} holds {} transparency bytes for grey",
                            bytes.len(),
                        ));
                    }
                    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
                }
                None => None,
            };
            Ok(Key {
                gray,
                rgb: None,
                alpha: Vec::new(),
                colors,
            })
        }
        Color::Rgb => {
            let rgb = match transparency {
                Some(bytes) => {
                    if bytes.len() != 6 {
                        return Err(format!(
                            "{what} holds {} transparency bytes for colour",
                            bytes.len(),
                        ));
                    }
                    Some([
                        u16::from_be_bytes([bytes[0], bytes[1]]),
                        u16::from_be_bytes([bytes[2], bytes[3]]),
                        u16::from_be_bytes([bytes[4], bytes[5]]),
                    ])
                }
                None => None,
            };
            Ok(Key {
                gray: None,
                rgb,
                alpha: Vec::new(),
                colors,
            })
        }
        Color::Palette => {
            let alpha = match &transparency {
                Some(bytes) if bytes.len() > colors.len() => {
                    return Err(format!(
                        "{what} holds {} transparency bytes for a palette of {}",
                        bytes.len(),
                        colors.len(),
                    ));
                }
                Some(bytes) => bytes.clone(),
                None => Vec::new(),
            };
            Ok(Key {
                gray: None,
                rgb: None,
                alpha,
                colors,
            })
        }
        Color::GrayAlpha | Color::Rgba => {
            if transparency.is_some() {
                return Err(format!("{what} holds transparency beside an alpha channel"));
            }
            Ok(Key {
                gray: None,
                rgb: None,
                alpha: Vec::new(),
                colors,
            })
        }
    }
}

fn paint(
    header: &Header,
    key: &Key,
    pass: &Pass,
    data: &[u8],
    channels: usize,
    out: &mut [u8],
    what: &str,
) -> Result<(), String> {
    for row in 0..pass.height {
        let start = row * (pass.row_bytes + 1) + 1;
        let line = &data[start..start + pass.row_bytes];
        let y = pass.y0 as usize + row * pass.dy as usize;
        for column in 0..pass.width {
            let x = pass.x0 as usize + column * pass.dx as usize;
            let rgba = match header.color {
                Color::Gray => {
                    let value = sample(line, channels, column, 0, header.depth);
                    let level = level(value, header.depth);
                    let alpha = u8::from(key.gray != Some(value)) * 255;
                    [level, level, level, alpha]
                }
                Color::Rgb => {
                    let red = sample(line, channels, column, 0, header.depth);
                    let green = sample(line, channels, column, 1, header.depth);
                    let blue = sample(line, channels, column, 2, header.depth);
                    let alpha = match key.rgb {
                        Some(rgb) if rgb == [red, green, blue] => 0,
                        _ => 255,
                    };
                    [
                        level(red, header.depth),
                        level(green, header.depth),
                        level(blue, header.depth),
                        alpha,
                    ]
                }
                Color::Palette => {
                    let index = usize::from(sample(line, channels, column, 0, header.depth));
                    if index >= key.colors.len() {
                        return Err(format!(
                            "{what} holds palette index {index} of {} entries",
                            key.colors.len(),
                        ));
                    }
                    let color = key.colors[index];
                    let alpha = key.alpha.get(index).copied().unwrap_or(255);
                    [color[0], color[1], color[2], alpha]
                }
                Color::GrayAlpha => {
                    let value = sample(line, channels, column, 0, header.depth);
                    let alpha = level(
                        sample(line, channels, column, 1, header.depth),
                        header.depth,
                    );
                    let shade = level(value, header.depth);
                    [shade, shade, shade, alpha]
                }
                Color::Rgba => [
                    level(
                        sample(line, channels, column, 0, header.depth),
                        header.depth,
                    ),
                    level(
                        sample(line, channels, column, 1, header.depth),
                        header.depth,
                    ),
                    level(
                        sample(line, channels, column, 2, header.depth),
                        header.depth,
                    ),
                    level(
                        sample(line, channels, column, 3, header.depth),
                        header.depth,
                    ),
                ],
            };
            let at = (y * header.width as usize + x) * 4;
            out[at..at + 4].copy_from_slice(&rgba);
        }
    }
    Ok(())
}

fn sample(line: &[u8], channels: usize, pixel: usize, channel: usize, depth: u8) -> u16 {
    if depth < 8 {
        let bit = pixel * usize::from(depth);
        let shift = 8 - usize::from(depth) - bit % 8;
        let mask = (1u8 << depth) - 1;
        return u16::from((line[bit / 8] >> shift) & mask);
    }
    let index = pixel * channels + channel;
    if depth == 16 {
        return u16::from_be_bytes([line[index * 2], line[index * 2 + 1]]);
    }
    u16::from(line[index])
}

fn level(value: u16, depth: u8) -> u8 {
    match depth {
        16 => (value >> 8) as u8,
        8 => value as u8,
        _ => (u32::from(value) * 255 / ((1u32 << depth) - 1)) as u8,
    }
}

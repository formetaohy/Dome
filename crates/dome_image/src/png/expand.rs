use crate::png::header::{Color, Header};
use crate::png::pass::Pass;
use crate::png::pass::{self, Layout};

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
    raw: &[u8],
    layout: &Layout,
    what: &str,
) -> Vec<u8> {
    let colors = match header.color {
        Color::Palette => {
            palette.unwrap_or_else(|| panic!("{what} holds a palette image without a palette"))
        }
        _ => palette.unwrap_or_default(),
    };
    let key = match header.color {
        Color::Gray => Key {
            gray: transparency.map(|bytes| {
                assert_eq!(
                    bytes.len(),
                    2,
                    "{what} holds {} transparency bytes for grey",
                    bytes.len()
                );
                u16::from_be_bytes([bytes[0], bytes[1]])
            }),
            rgb: None,
            alpha: Vec::new(),
            colors,
        },
        Color::Rgb => Key {
            gray: None,
            rgb: transparency.map(|bytes| {
                assert_eq!(
                    bytes.len(),
                    6,
                    "{what} holds {} transparency bytes for colour",
                    bytes.len()
                );
                [
                    u16::from_be_bytes([bytes[0], bytes[1]]),
                    u16::from_be_bytes([bytes[2], bytes[3]]),
                    u16::from_be_bytes([bytes[4], bytes[5]]),
                ]
            }),
            alpha: Vec::new(),
            colors,
        },
        Color::Palette => {
            if let Some(bytes) = &transparency {
                assert!(
                    bytes.len() <= colors.len(),
                    "{what} holds {} transparency bytes for a palette of {}",
                    bytes.len(),
                    colors.len(),
                );
            }
            Key {
                gray: None,
                rgb: None,
                alpha: transparency.unwrap_or_default(),
                colors,
            }
        }
        Color::GrayAlpha | Color::Rgba => {
            assert!(
                transparency.is_none(),
                "{what} holds transparency beside an alpha channel",
            );
            Key {
                gray: None,
                rgb: None,
                alpha: Vec::new(),
                colors,
            }
        }
    };
    let pixels = pass::unfilter(raw, layout, what);
    let channels = header.color.channels();
    let mut out = vec![0u8; header.width as usize * header.height as usize * 4];
    for (pass, data) in layout.passes.iter().zip(&pixels) {
        paint(header, &key, pass, data, channels, &mut out, what);
    }
    out
}

fn paint(
    header: &Header,
    key: &Key,
    pass: &Pass,
    data: &[u8],
    channels: usize,
    out: &mut [u8],
    what: &str,
) {
    for row in 0..pass.height {
        let line = &data[row * pass.row_bytes..(row + 1) * pass.row_bytes];
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
                    assert!(
                        index < key.colors.len(),
                        "{what} holds palette index {index} of {} entries",
                        key.colors.len(),
                    );
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

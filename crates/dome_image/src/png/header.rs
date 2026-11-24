pub(crate) struct Header {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) depth: u8,
    pub(crate) color: Color,
    pub(crate) interlaced: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Color {
    Gray,
    Rgb,
    Palette,
    GrayAlpha,
    Rgba,
}

impl Color {
    pub(crate) fn channels(self) -> usize {
        match self {
            Self::Gray | Self::Palette => 1,
            Self::GrayAlpha => 2,
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }
}

pub(crate) fn parse(data: &[u8], what: &str) -> Result<Header, String> {
    if data.len() != 13 {
        return Err(format!("{what} holds a header of {} bytes", data.len()));
    }
    let width = u32::from_be_bytes(data[0..4].try_into().unwrap());
    let height = u32::from_be_bytes(data[4..8].try_into().unwrap());
    let depth = data[8];
    let color = match data[9] {
        0 => Color::Gray,
        2 => Color::Rgb,
        3 => Color::Palette,
        4 => Color::GrayAlpha,
        6 => Color::Rgba,
        other => return Err(format!("{what} holds colour type {other}")),
    };
    if !matches!(
        (color, depth),
        (Color::Gray, 1 | 2 | 4 | 8 | 16)
            | (Color::Rgb, 8 | 16)
            | (Color::Palette, 1 | 2 | 4 | 8)
            | (Color::GrayAlpha, 8 | 16)
            | (Color::Rgba, 8 | 16)
    ) {
        return Err(format!(
            "{what} holds colour type {color:?} of {depth} bits"
        ));
    }
    if data[10] != 0 {
        return Err(format!("{what} holds compression {}", data[10]));
    }
    if data[11] != 0 {
        return Err(format!("{what} holds filter {}", data[11]));
    }
    if data[12] > 1 {
        return Err(format!("{what} holds interlace method {}", data[12]));
    }
    Ok(Header {
        width,
        height,
        depth,
        color,
        interlaced: data[12] == 1,
    })
}

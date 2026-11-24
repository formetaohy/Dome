mod chunk;
mod encode;
mod expand;
mod header;
mod inflate;
mod pass;

pub use encode::encode;

pub struct Image {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

impl Image {
    pub fn decode(bytes: &[u8], what: &str) -> Result<Self, String> {
        const PNG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
        const JPEG: [u8; 2] = [0xFF, 0xD8];
        if bytes.starts_with(&PNG) {
            return Self::read(bytes, what);
        }
        if bytes.starts_with(&JPEG) {
            return crate::jpeg::decode(bytes, what);
        }
        Err(format!("{what} holds neither a PNG nor a JPEG image"))
    }

    pub(crate) fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba,
        }
    }

    pub(crate) fn read(bytes: &[u8], what: &str) -> Result<Self, String> {
        const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
        if !bytes.starts_with(&SIGNATURE) {
            return Err(format!("{what} holds no portably networked graphic"));
        }
        let mut header = None;
        let mut palette = None;
        let mut transparency = None;
        let mut data = Vec::new();
        for chunk in chunk::read(&bytes[SIGNATURE.len()..], what)? {
            match &chunk.kind {
                b"IHDR" => {
                    if header.is_some() || !data.is_empty() {
                        return Err(format!("{what} holds a second header"));
                    }
                    header = Some(header::parse(chunk.data, what)?);
                }
                b"PLTE" => {
                    if palette.is_some() || !data.is_empty() {
                        return Err(format!("{what} holds a second palette"));
                    }
                    palette = Some(chunk::palette(chunk.data, what)?);
                }
                b"tRNS" => {
                    if transparency.is_some() || !data.is_empty() {
                        return Err(format!("{what} holds a second transparency chunk"));
                    }
                    transparency = Some(chunk.data.to_vec());
                }
                b"IDAT" => data.extend_from_slice(chunk.data),
                b"IEND" => {
                    if !chunk.data.is_empty() {
                        return Err(format!("{what} holds a sized end chunk"));
                    }
                }
                kind if kind[0].is_ascii_uppercase() => {
                    return Err(format!(
                        "{what} holds the unknown critical chunk {}",
                        chunk::name(*kind),
                    ));
                }
                _ => {}
            }
        }
        let header = header.ok_or_else(|| format!("{what} holds no header"))?;
        let texels = crate::limit::DecodeLimit::DEFAULT.guard(header.width, header.height, what)?;
        let layout = pass::layout(&header);
        let raw_bytes = layout.raw_bytes();
        inflate::reachable(&data, raw_bytes, what)?;
        let mut raw = inflate::inflate(&data, raw_bytes, what)?;
        let rgba = expand::expand(
            &header,
            palette,
            transparency,
            &mut raw,
            &layout,
            texels,
            what,
        )?;
        Ok(Self {
            width: header.width,
            height: header.height,
            rgba,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

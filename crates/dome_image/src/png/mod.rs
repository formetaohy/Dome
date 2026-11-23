mod chunk;
mod encode;
mod expand;
mod header;
mod inflate;
mod pass;

pub use encode::encode;

pub struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Image {
    pub fn decode(bytes: &[u8], what: &str) -> Self {
        const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
        assert!(
            bytes.starts_with(&SIGNATURE),
            "{what} holds no portably networked graphic",
        );
        let mut header = None;
        let mut palette = None;
        let mut transparency = None;
        let mut data = Vec::new();
        let mut ended = false;
        for chunk in chunk::read(&bytes[SIGNATURE.len()..], what) {
            assert!(
                !ended,
                "{what} holds chunk {} past its end",
                chunk::name(chunk.kind),
            );
            match &chunk.kind {
                b"IHDR" => {
                    assert!(
                        header.is_none() && data.is_empty(),
                        "{what} holds a second header",
                    );
                    header = Some(header::parse(chunk.data, what));
                }
                b"PLTE" => {
                    assert!(
                        palette.is_none() && data.is_empty(),
                        "{what} holds a second palette",
                    );
                    palette = Some(chunk::palette(chunk.data, what));
                }
                b"tRNS" => {
                    assert!(
                        transparency.is_none() && data.is_empty(),
                        "{what} holds a second transparency chunk",
                    );
                    transparency = Some(chunk.data.to_vec());
                }
                b"IDAT" => data.extend_from_slice(chunk.data),
                b"IEND" => {
                    assert!(chunk.data.is_empty(), "{what} holds a sized end chunk");
                    ended = true;
                }
                kind if kind[0].is_ascii_uppercase() => panic!(
                    "{what} holds the unknown critical chunk {}",
                    chunk::name(*kind)
                ),
                _ => {}
            }
        }
        assert!(ended, "{what} ends before its end chunk");
        let header = header.unwrap_or_else(|| panic!("{what} holds no header"));
        let layout = pass::layout(&header);
        let raw = inflate::inflate(
            &data,
            layout.raw_bytes(),
            &format!("{what} holds image data"),
        );
        let rgba = expand::expand(&header, palette, transparency, &raw, &layout, what);
        Self {
            width: header.width,
            height: header.height,
            rgba,
        }
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

use dome_render::SamplerDesc;
use jpeg_decoder::PixelFormat;
use std::io::Cursor;

const PNG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
const JPEG: [u8; 2] = [0xFF, 0xD8];

pub struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    sampler: SamplerDesc,
}

impl Image {
    pub(crate) fn decode(bytes: &[u8], what: &str, sampler: SamplerDesc) -> Self {
        if bytes.starts_with(&PNG) {
            let held = dome_image::Image::decode(bytes, what);
            return Self {
                width: held.width(),
                height: held.height(),
                rgba: held.rgba().to_vec(),
                sampler,
            };
        }
        if bytes.starts_with(&JPEG) {
            return Self::jpeg(bytes, what, sampler);
        }
        panic!("{what} holds neither a PNG nor a JPEG image");
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

    pub fn sampler(&self) -> SamplerDesc {
        self.sampler
    }

    fn jpeg(bytes: &[u8], what: &str, sampler: SamplerDesc) -> Self {
        let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
        let pixels = decoder
            .decode()
            .unwrap_or_else(|error| panic!("{what} decodes no JPEG image: {error}"));
        let info = decoder
            .info()
            .unwrap_or_else(|| panic!("{what} holds no JPEG header"));
        let width = u32::from(info.width);
        let height = u32::from(info.height);
        let count = width as usize * height as usize;
        let rgba = match info.pixel_format {
            PixelFormat::L8 => {
                assert_eq!(pixels.len(), count, "{what} holds a JPEG of another size");
                let mut rgba = Vec::with_capacity(count * 4);
                for value in pixels {
                    rgba.extend([value, value, value, 255]);
                }
                rgba
            }
            PixelFormat::RGB24 => {
                assert_eq!(
                    pixels.len(),
                    count * 3,
                    "{what} holds a JPEG of another size"
                );
                let mut rgba = Vec::with_capacity(count * 4);
                for held in pixels.as_chunks::<3>().0 {
                    rgba.extend([held[0], held[1], held[2], 255]);
                }
                rgba
            }
            other => panic!("{what} holds a JPEG of the {other:?} layout"),
        };
        Self {
            width,
            height,
            rgba,
            sampler,
        }
    }
}

use crate::png::Image;
use jpeg_decoder::PixelFormat;
use std::io::Cursor;

pub(crate) fn decode(bytes: &[u8], what: &str) -> Image {
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
    Image::from_rgba(width, height, rgba)
}

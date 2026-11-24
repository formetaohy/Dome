use crate::limit::DecodeLimit;
use crate::png::Image;
use jpeg_decoder::PixelFormat;
use std::io::Cursor;

pub(crate) fn decode(bytes: &[u8], what: &str) -> Result<Image, String> {
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    decoder
        .read_info()
        .map_err(|error| format!("{what} decodes no JPEG image: {error}"))?;
    let info = decoder
        .info()
        .ok_or_else(|| format!("{what} holds no JPEG header"))?;
    let width = u32::from(info.width);
    let height = u32::from(info.height);
    let count = DecodeLimit::DEFAULT.guard(width, height, what)?;
    let pixels = decoder
        .decode()
        .map_err(|error| format!("{what} decodes no JPEG image: {error}"))?;
    let rgba = match info.pixel_format {
        PixelFormat::L8 => {
            if pixels.len() != count {
                return Err(format!("{what} holds a JPEG of another size"));
            }
            let mut rgba = Vec::with_capacity(count * 4);
            for value in pixels {
                rgba.extend([value, value, value, 255]);
            }
            rgba
        }
        PixelFormat::RGB24 => {
            if pixels.len() != count * 3 {
                return Err(format!("{what} holds a JPEG of another size"));
            }
            let mut rgba = Vec::with_capacity(count * 4);
            for held in pixels.as_chunks::<3>().0 {
                rgba.extend([held[0], held[1], held[2], 255]);
            }
            rgba
        }
        other => return Err(format!("{what} holds a JPEG of the {other:?} layout")),
    };
    Ok(Image::from_rgba(width, height, rgba))
}

use flate2::read::ZlibDecoder;
use std::io::Read;

const ZLIB_HEADER: u64 = 2;
const ZLIB_TRAILER: u64 = 4;
const DEFLATE_RATIO: u64 = 1032;

pub(crate) fn reachable(data: &[u8], expected: usize, what: &str) -> Result<(), String> {
    let least = (expected as u64).div_ceil(DEFLATE_RATIO) + ZLIB_HEADER + ZLIB_TRAILER;
    if (data.len() as u64) < least {
        return Err(format!(
            "{what} holds {} image data bytes where its scanlines need at least {least}",
            data.len(),
        ));
    }
    Ok(())
}

pub(crate) fn inflate(data: &[u8], expected: usize, what: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(expected);
    let limit = expected as u64 + 1;
    ZlibDecoder::new(data)
        .take(limit)
        .read_to_end(&mut out)
        .map_err(|error| format!("{what} inflates to no bytes: {error}"))?;
    if out.len() != expected {
        return Err(format!(
            "{what} spans {} bytes where {expected} bytes stand",
            out.len(),
        ));
    }
    Ok(out)
}

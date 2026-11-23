use flate2::read::ZlibDecoder;
use std::io::Read;

pub(crate) fn inflate(data: &[u8], expected: usize, what: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(expected);
    let limit = expected as u64 + 1;
    ZlibDecoder::new(data)
        .take(limit)
        .read_to_end(&mut out)
        .unwrap_or_else(|error| panic!("{what} inflate to no bytes: {error}"));
    assert_eq!(
        out.len(),
        expected,
        "{what} span {} bytes where {expected} bytes stand",
        out.len(),
    );
    out
}

pub(crate) fn header<'a>(bytes: &'a [u8], what: &str) -> (f64, u32, u32, &'a [u8]) {
    let mut offset = 0;
    let signature = line(bytes, &mut offset, what);
    assert!(
        signature == "#?RADIANCE" || signature == "#?RGBE",
        "{what} holds the signature {signature}, not a radiance picture",
    );
    let mut exposure = 1.0;
    let mut format = false;
    loop {
        let held = line(bytes, &mut offset, what);
        if held.is_empty() {
            break;
        }
        if let Some(value) = held.strip_prefix("FORMAT=") {
            format = value == "32-bit_rle_rgbe";
        }
        if let Some(value) = held.strip_prefix("EXPOSURE=") {
            let held: f64 = value
                .parse()
                .unwrap_or_else(|error| panic!("{what} holds the exposure {value}: {error}"));
            assert!(
                held.is_finite() && held != 0.0,
                "{what} holds the exposure {value}",
            );
            exposure *= held;
        }
    }
    assert!(format, "{what} holds no thirty two bit rgbe format line");
    let held = line(bytes, &mut offset, what);
    let parts = held.split_whitespace().collect::<Vec<_>>();
    assert_eq!(
        parts.len(),
        4,
        "{what} holds the resolution {held}, not -Y rows +X columns",
    );
    assert!(
        parts[0] == "-Y" && parts[2] == "+X",
        "{what} holds the resolution {held}, not -Y rows +X columns",
    );
    let height = dimension(parts[1], what);
    let width = dimension(parts[3], what);
    assert!(
        width > 0 && height > 0,
        "{what} spans no texel of its {width} by {height} picture",
    );
    (exposure, width, height, &bytes[offset..])
}

fn dimension(text: &str, what: &str) -> u32 {
    text.parse()
        .unwrap_or_else(|error| panic!("{what} holds the dimension {text}: {error}"))
}

fn line<'a>(bytes: &'a [u8], offset: &mut usize, what: &str) -> &'a str {
    assert!(*offset < bytes.len(), "{what} ends before its header does",);
    let rest = &bytes[*offset..];
    let end = rest
        .iter()
        .position(|held| *held == b'\n')
        .unwrap_or_else(|| panic!("{what} holds a header line without an end"));
    *offset += end + 1;
    let text = &rest[..end];
    let text = text.strip_suffix(b"\r").unwrap_or(text);
    std::str::from_utf8(text)
        .unwrap_or_else(|error| panic!("{what} holds a header line without text: {error}"))
}

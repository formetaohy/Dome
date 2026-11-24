pub(crate) fn header<'a>(bytes: &'a [u8], what: &str) -> Result<(f64, u32, u32, &'a [u8]), String> {
    let mut offset = 0;
    let signature = line(bytes, &mut offset, what)?;
    if signature != "#?RADIANCE" && signature != "#?RGBE" {
        return Err(format!(
            "{what} holds the signature {signature}, not a radiance picture",
        ));
    }
    let mut exposure = 1.0;
    let mut format = false;
    loop {
        let held = line(bytes, &mut offset, what)?;
        if held.is_empty() {
            break;
        }
        if let Some(value) = held.strip_prefix("FORMAT=") {
            format = value == "32-bit_rle_rgbe";
        }
        if let Some(value) = held.strip_prefix("EXPOSURE=") {
            let held: f64 = value
                .parse()
                .map_err(|error| format!("{what} holds the exposure {value}: {error}"))?;
            if !held.is_finite() || held == 0.0 {
                return Err(format!("{what} holds the exposure {value}"));
            }
            exposure *= held;
        }
    }
    if !format {
        return Err(format!("{what} holds no thirty two bit rgbe format line"));
    }
    let held = line(bytes, &mut offset, what)?;
    let parts = held.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 4 {
        return Err(format!(
            "{what} holds the resolution {held}, not -Y rows +X columns",
        ));
    }
    if parts[0] != "-Y" || parts[2] != "+X" {
        return Err(format!(
            "{what} holds the resolution {held}, not -Y rows +X columns",
        ));
    }
    let height = dimension(parts[1], what)?;
    let width = dimension(parts[3], what)?;
    Ok((exposure, width, height, &bytes[offset..]))
}

fn dimension(text: &str, what: &str) -> Result<u32, String> {
    text.parse()
        .map_err(|error| format!("{what} holds the dimension {text}: {error}"))
}

fn line<'a>(bytes: &'a [u8], offset: &mut usize, what: &str) -> Result<&'a str, String> {
    if *offset >= bytes.len() {
        return Err(format!("{what} ends before its header does"));
    }
    let rest = &bytes[*offset..];
    let end = rest
        .iter()
        .position(|held| *held == b'\n')
        .ok_or_else(|| format!("{what} holds a header line without an end"))?;
    *offset += end + 1;
    let text = &rest[..end];
    let text = text.strip_suffix(b"\r").unwrap_or(text);
    std::str::from_utf8(text)
        .map_err(|error| format!("{what} holds a header line without text: {error}"))
}

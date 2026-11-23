use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn read(uri: &str, base: &Path, what: &str) -> Vec<u8> {
    if uri.starts_with("data:") {
        return data(uri, what);
    }
    let path = path(uri, base)
        .unwrap_or_else(|error| panic!("{what} {error}"))
        .expect("a file URI holds a path");
    fs::read(&path).unwrap_or_else(|error| panic!("{what} reads {}: {error}", path.display()))
}

pub(crate) fn path(uri: &str, base: &Path) -> Result<Option<PathBuf>, String> {
    if uri.starts_with("data:") {
        return Ok(None);
    }
    let held = percent(uri)?;
    let path = Path::new(&held);
    if !path.is_relative() {
        return Err(format!("names the absolute resource {uri}"));
    }
    Ok(Some(base.join(path)))
}

fn data(uri: &str, what: &str) -> Vec<u8> {
    let (header, body) = uri
        .split_once(',')
        .unwrap_or_else(|| panic!("{what} holds a data URI without a body"));
    assert!(
        header.starts_with("data:") && header.to_ascii_lowercase().ends_with(";base64"),
        "{what} holds a data URI that is not base64",
    );
    base64(body, what)
}

fn base64(text: &str, what: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
    let mut held = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => panic!("{what} holds the base64 byte {byte}"),
        };
        held = (held << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((held >> bits) as u8);
        }
    }
    bytes
}

fn percent(uri: &str) -> Result<String, String> {
    let bytes = uri.as_bytes();
    let mut text = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'%' {
            text.push(bytes[at]);
            at += 1;
            continue;
        }
        let held = bytes
            .get(at + 1..at + 3)
            .ok_or_else(|| format!("ends inside the escape of {uri}"))?;
        let high = digit(held[0])?;
        let low = digit(held[1])?;
        text.push(high << 4 | low);
        at += 3;
    }
    String::from_utf8(text).map_err(|error| format!("names no text path: {error}"))
}

fn digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("holds the escape digit {byte}")),
    }
}

#[cfg(test)]
mod tests {
    use super::read;
    use std::path::Path;

    #[test]
    fn a_data_uri_holds_the_bytes_its_base64_names() {
        assert_eq!(
            read(
                "data:application/octet-stream;base64,AAECAwQ=",
                Path::new("."),
                "a test URI"
            ),
            [0, 1, 2, 3, 4],
        );
        assert_eq!(
            read(
                "data:application/octet-stream;base64,//8=",
                Path::new("."),
                "a test URI"
            ),
            [255, 255],
        );
    }
}

pub(crate) struct Chunk<'a> {
    pub(crate) kind: [u8; 4],
    pub(crate) data: &'a [u8],
}

pub(crate) fn read<'a>(bytes: &'a [u8], what: &str) -> impl Iterator<Item = Chunk<'a>> {
    let mut at = 0;
    let mut ended = false;
    std::iter::from_fn(move || {
        if ended {
            return None;
        }
        assert!(bytes.len() - at >= 12, "{what} ends inside a chunk header",);
        let length = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = bytes[at + 4..at + 8].try_into().unwrap();
        assert!(
            kind.iter().all(u8::is_ascii_alphabetic),
            "{what} holds chunk {} of no letters",
            name(kind),
        );
        let end = at + 8 + length;
        assert!(
            end + 4 <= bytes.len(),
            "{what} ends inside chunk {} of {length} bytes",
            name(kind),
        );
        let data = &bytes[at + 8..end];
        let held = u32::from_be_bytes(bytes[end..end + 4].try_into().unwrap());
        assert!(
            crc32(&bytes[at + 4..end]) == held,
            "{what} holds chunk {} of mismatched checksum",
            name(kind),
        );
        ended = kind == *b"IEND";
        at = end + 4;
        if ended {
            assert!(
                at == bytes.len(),
                "{what} holds {} bytes past its end chunk",
                bytes.len() - at,
            );
        }
        Some(Chunk { kind, data })
    })
}

pub(crate) fn name(kind: [u8; 4]) -> String {
    String::from_utf8_lossy(&kind).into_owned()
}

pub(crate) fn palette(data: &[u8], what: &str) -> Vec<[u8; 3]> {
    assert!(
        !data.is_empty() && data.len().is_multiple_of(3) && data.len() <= 256 * 3,
        "{what} holds a palette of {} bytes",
        data.len(),
    );
    data.as_chunks::<3>().0.to_vec()
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        let index = ((crc ^ u32::from(*byte)) & 0xff) as usize;
        crc = CRC[index] ^ (crc >> 8);
    }
    crc ^ u32::MAX
}

const CRC: [u32; 256] = table();

const fn table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 == 1 {
                EDGE ^ (crc >> 1)
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
}

const EDGE: u32 = 0xedb8_8320;

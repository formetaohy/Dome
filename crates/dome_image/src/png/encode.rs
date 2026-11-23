use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::io::Write;

const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

pub fn encode(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    assert!(
        width > 0 && height > 0,
        "a graphic of {width} by {height} spans no texels"
    );
    let texels = u64::from(width) * u64::from(height);
    assert!(
        texels * 4 == rgba.len() as u64,
        "a graphic of {width} by {height} holds {} bytes where its texels span {}",
        rgba.len(),
        texels * 4,
    );
    let row = usize::try_from(u64::from(width) * 4).expect("a graphic row fits this machine");
    let rows = usize::try_from(height).expect("a graphic spans rows this machine counts");
    let mut scanlines = Vec::with_capacity((row + 1) * rows);
    for line in rgba.chunks_exact(row) {
        scanlines.push(0);
        scanlines.extend_from_slice(line);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(&scanlines)
        .expect("a graphic compresses into its image data");
    let data = encoder.finish().expect("a graphic closes its image data");

    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);

    let mut png = Vec::with_capacity(SIGNATURE.len() + 25 + data.len() + 12);
    png.extend_from_slice(&SIGNATURE);
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &data);
    chunk(&mut png, b"IEND", &[]);
    png
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    let length = u32::try_from(data.len()).expect("a graphic chunk fits its length");
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

struct Crc(u32);

impl Crc {
    fn new() -> Self {
        Self(0xffff_ffff)
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u32::from(*byte);
            for _ in 0..8 {
                self.0 = if self.0 & 1 == 0 {
                    self.0 >> 1
                } else {
                    0xedb8_8320 ^ (self.0 >> 1)
                };
            }
        }
    }

    fn finish(self) -> u32 {
        !self.0
    }
}

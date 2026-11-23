use crate::radiance::value;

const RUN: u8 = 128;
const CHANNELS: usize = 4;
const REPEAT: [u8; 3] = [1, 1, 1];

pub(crate) fn decode(bytes: &[u8], width: u32, height: u32, what: &str) -> Vec<[f32; 3]> {
    let mut cursor = Cursor {
        bytes,
        offset: 0,
        what,
    };
    let mut texels = Vec::with_capacity(width as usize * height as usize);
    let mut row = vec![[0u8; 4]; width as usize];
    for _ in 0..height {
        if (8..=0x7fff).contains(&width) {
            let start = cursor.offset;
            let head = cursor.take(4);
            if head[0] == 2 && head[1] == 2 && head[2] < RUN {
                let declared = u32::from(head[2]) << 8 | u32::from(head[3]);
                assert_eq!(
                    declared, width,
                    "{what} holds a scanline of {declared} texels where its picture holds {width}",
                );
                cursor.scanline(&mut row);
            } else {
                cursor.offset = start;
                old(&mut cursor, &mut row);
            }
        } else {
            old(&mut cursor, &mut row);
        }
        texels.extend(row.iter().copied().map(value));
    }
    texels
}

fn old(cursor: &mut Cursor<'_>, row: &mut [[u8; 4]]) {
    let mut at = 0;
    let mut shift = 0;
    while at < row.len() {
        let texel = cursor.texel();
        if texel[..3] == REPEAT {
            assert!(
                at > 0,
                "{} repeats a texel before it holds one",
                cursor.what,
            );
            assert!(
                shift < usize::BITS as usize,
                "{} repeats a texel beyond its scanline",
                cursor.what,
            );
            let count = usize::from(texel[3]) << shift;
            assert!(
                count <= row.len() - at,
                "{} overruns a scanline of {} texels",
                cursor.what,
                row.len(),
            );
            let repeated = row[at - 1];
            for held in &mut row[at..at + count] {
                *held = repeated;
            }
            at += count;
            shift += 8;
        } else {
            row[at] = texel;
            at += 1;
            shift = 0;
        }
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
    what: &'a str,
}

impl Cursor<'_> {
    fn texel(&mut self) -> [u8; 4] {
        let held = self.take(4);
        [held[0], held[1], held[2], held[3]]
    }

    fn take(&mut self, count: usize) -> &[u8] {
        let end = self.offset + count;
        assert!(
            end <= self.bytes.len(),
            "{} ends inside a scanline",
            self.what,
        );
        let held = &self.bytes[self.offset..end];
        self.offset = end;
        held
    }

    fn byte(&mut self) -> u8 {
        self.take(1)[0]
    }

    fn scanline(&mut self, row: &mut [[u8; 4]]) {
        for channel in 0..CHANNELS {
            let mut at = 0;
            while at < row.len() {
                let count = self.byte();
                if count > RUN {
                    let value = self.byte();
                    let run = usize::from(count - RUN);
                    assert!(
                        at + run <= row.len(),
                        "{} overruns a scanline of {} texels",
                        self.what,
                        row.len(),
                    );
                    for texel in &mut row[at..at + run] {
                        texel[channel] = value;
                    }
                    at += run;
                } else {
                    let run = usize::from(count);
                    assert!(
                        run > 0 && at + run <= row.len(),
                        "{} overruns a scanline of {} texels",
                        self.what,
                        row.len(),
                    );
                    let values = self.take(run);
                    for (texel, value) in row[at..at + run].iter_mut().zip(values) {
                        texel[channel] = *value;
                    }
                    at += run;
                }
            }
        }
    }
}

use crate::radiance::value;

const RUN: u8 = 128;
const CHANNELS: usize = 4;
const REPEAT: [u8; 3] = [1, 1, 1];

pub(crate) fn decode(
    bytes: &[u8],
    width: u32,
    height: u32,
    scale: f32,
    texels: usize,
    what: &str,
) -> Result<Vec<f32>, String> {
    let least = u64::from(height) * least_scanline(width);
    if (bytes.len() as u64) < least {
        return Err(format!(
            "{what} holds {} scanline bytes where {height} rows of {width} texels need at least {least}",
            bytes.len(),
        ));
    }
    let mut cursor = Cursor {
        bytes,
        offset: 0,
        what,
    };
    let mut rgb = Vec::with_capacity(texels * 3);
    let mut row = vec![[0u8; 4]; width as usize];
    for _ in 0..height {
        if (8..=0x7fff).contains(&width) {
            let start = cursor.offset;
            let head = cursor.take(4)?;
            if head[0] == 2 && head[1] == 2 && head[2] < RUN {
                let declared = u32::from(head[2]) << 8 | u32::from(head[3]);
                if declared != width {
                    return Err(format!(
                        "{what} holds a scanline of {declared} texels where its picture holds {width}",
                    ));
                }
                cursor.scanline(&mut row)?;
            } else {
                cursor.offset = start;
                old(&mut cursor, &mut row)?;
            }
        } else {
            old(&mut cursor, &mut row)?;
        }
        for texel in &row {
            let held = value(*texel);
            rgb.extend_from_slice(&[held[0] * scale, held[1] * scale, held[2] * scale]);
        }
    }
    Ok(rgb)
}

fn least_scanline(width: u32) -> u64 {
    if width == 1 { 4 } else { 8 }
}

fn old(cursor: &mut Cursor<'_>, row: &mut [[u8; 4]]) -> Result<(), String> {
    let mut at = 0;
    let mut shift = 0;
    while at < row.len() {
        let texel = cursor.texel()?;
        if texel[..3] == REPEAT {
            if at == 0 {
                return Err(format!(
                    "{} repeats a texel before it holds one",
                    cursor.what,
                ));
            }
            if shift >= usize::BITS as usize {
                return Err(format!(
                    "{} repeats a texel beyond its scanline",
                    cursor.what,
                ));
            }
            let count = usize::from(texel[3]) << shift;
            if count > row.len() - at {
                return Err(format!(
                    "{} overruns a scanline of {} texels",
                    cursor.what,
                    row.len(),
                ));
            }
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
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
    what: &'a str,
}

impl Cursor<'_> {
    fn texel(&mut self) -> Result<[u8; 4], String> {
        let held = self.take(4)?;
        Ok([held[0], held[1], held[2], held[3]])
    }

    fn take(&mut self, count: usize) -> Result<&[u8], String> {
        let end = self
            .offset
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| format!("{} ends inside a scanline", self.what))?;
        let held = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(held)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn scanline(&mut self, row: &mut [[u8; 4]]) -> Result<(), String> {
        for channel in 0..CHANNELS {
            let mut at = 0;
            while at < row.len() {
                let count = self.byte()?;
                if count > RUN {
                    let value = self.byte()?;
                    let run = usize::from(count - RUN);
                    if at + run > row.len() {
                        return Err(format!(
                            "{} overruns a scanline of {} texels",
                            self.what,
                            row.len(),
                        ));
                    }
                    for texel in &mut row[at..at + run] {
                        texel[channel] = value;
                    }
                    at += run;
                } else {
                    let run = usize::from(count);
                    if run == 0 || at + run > row.len() {
                        return Err(format!(
                            "{} overruns a scanline of {} texels",
                            self.what,
                            row.len(),
                        ));
                    }
                    let values = self.take(run)?;
                    for (texel, value) in row[at..at + run].iter_mut().zip(values) {
                        texel[channel] = *value;
                    }
                    at += run;
                }
            }
        }
        Ok(())
    }
}

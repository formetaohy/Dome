use crate::sound::{Channels, Sound};
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};

const RIFF: &[u8; 4] = b"RIFF";
const WAVE: &[u8; 4] = b"WAVE";
const FORMAT: &[u8; 4] = b"fmt ";
const DATA: &[u8; 4] = b"data";
const PCM: u16 = 1;
const IEEE_FLOAT: u16 = 3;
const EXTENSIBLE: u16 = 0xFFFE;
const EXTENSIBLE_BYTES: usize = 40;
const FORMAT_BYTES: u64 = 64;

#[derive(Clone, Debug)]
pub(crate) struct Pcm {
    channels: Channels,
    sample_rate: u32,
    bits: u16,
    tag: u16,
    block: usize,
}

impl Pcm {
    pub(crate) fn channels(&self) -> Channels {
        self.channels
    }

    pub(crate) fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Header {
    pcm: Pcm,
    offset: u64,
    bytes: u64,
}

impl Header {
    pub(crate) fn pcm(&self) -> &Pcm {
        &self.pcm
    }

    pub(crate) fn frames(&self) -> u64 {
        self.bytes / self.pcm.block as u64
    }
}

pub(crate) struct Reader {
    file: File,
    header: Header,
    frame: u64,
}

impl Reader {
    pub(crate) fn open(file: File, header: Header) -> Self {
        Self {
            file,
            header,
            frame: 0,
        }
    }

    pub(crate) fn frames(&self) -> u64 {
        self.header.frames()
    }

    pub(crate) fn seek(&mut self, frame: u64) -> Result<(), String> {
        let at = self.header.offset + frame * self.header.pcm.block as u64;
        self.file
            .seek(SeekFrom::Start(at))
            .map_err(|error| format!("the waveform seeks no frame {frame}: {error}"))?;
        self.frame = frame;
        Ok(())
    }

    pub(crate) fn read(&mut self, frames: usize) -> Result<Vec<f32>, String> {
        let held = self.frames().saturating_sub(self.frame);
        let count = held.min(frames as u64) as usize;
        let mut bytes = vec![0u8; count * self.header.pcm.block];
        self.file
            .read_exact(&mut bytes)
            .map_err(|error| format!("the waveform reads no {count} frames: {error}"))?;
        self.frame += count as u64;
        Ok(samples(&self.header.pcm, &bytes))
    }
}

pub(crate) fn header(reader: &mut (impl Read + Seek), what: &str) -> Result<Header, String> {
    let mut intro = [0u8; 12];
    reader
        .read_exact(&mut intro)
        .map_err(|_| format!("{what} holds no waveform audio"))?;
    if &intro[..4] != RIFF || &intro[8..12] != WAVE {
        return Err(format!("{what} holds no waveform audio"));
    }
    let mut pcm: Option<Pcm> = None;
    let mut data: Option<(u64, u64)> = None;
    loop {
        if let (Some(pcm), Some((offset, bytes))) = (&pcm, data) {
            return Ok(Header {
                pcm: pcm.clone(),
                offset,
                bytes,
            });
        }
        let mut chunk = [0u8; 8];
        if reader.read_exact(&mut chunk).is_err() {
            break;
        }
        let id = &chunk[..4];
        let size = u64::from(u32::from_le_bytes(chunk[4..8].try_into().unwrap()));
        let start = reader
            .stream_position()
            .map_err(|error| format!("{what} reads no position: {error}"))?;
        if id == FORMAT {
            let held = size.min(FORMAT_BYTES) as usize;
            let mut bytes = vec![0u8; held];
            reader
                .read_exact(&mut bytes)
                .map_err(|error| format!("{what} reads no format chunk: {error}"))?;
            pcm = Some(read_layout(&bytes, what)?);
        } else if id == DATA {
            let end = reader
                .seek(SeekFrom::End(0))
                .map_err(|error| format!("{what} reads no length: {error}"))?;
            let available = end.saturating_sub(start);
            let bytes = if size == 0 || size > available {
                available
            } else {
                size
            };
            data = Some((start, bytes));
            if size == 0 && pcm.is_none() {
                return Err(format!("{what} holds its data before its format"));
            }
        }
        reader
            .seek(SeekFrom::Start(start + size + (size & 1)))
            .map_err(|error| format!("{what} skips no chunk: {error}"))?;
    }
    match (pcm, data) {
        (None, Some(_)) => Err(format!("{what} holds no format chunk")),
        (_, None) => Err(format!("{what} holds no data chunk")),
        (Some(_), Some(_)) => unreachable!("a held format and data chunk returns from its scan"),
    }
}

pub(crate) fn decode(bytes: &[u8], what: &str) -> Sound {
    let mut reader = Cursor::new(bytes);
    let header = header(&mut reader, what).unwrap_or_else(|error| panic!("{error}"));
    let start = header.offset as usize;
    let end = start + header.bytes as usize;
    let samples = samples(&header.pcm, &bytes[start..end]);
    Sound::new(header.pcm.sample_rate, header.pcm.channels, samples)
}

pub(crate) fn samples(pcm: &Pcm, data: &[u8]) -> Vec<f32> {
    let frames = data.len() / pcm.block;
    let data = &data[..frames * pcm.block];
    match (pcm.tag, pcm.bits) {
        (PCM, 8) => pcm8(data),
        (PCM, 16) => pcm16(data),
        (PCM, 24) => pcm24(data),
        (PCM, 32) => pcm32(data),
        (IEEE_FLOAT, 32) => floats32(data),
        (IEEE_FLOAT, 64) => floats64(data),
        (tag, bits) => panic!("a waveform holds {bits} bit samples of format {tag}"),
    }
}

fn read_layout(bytes: &[u8], what: &str) -> Result<Pcm, String> {
    if bytes.len() < 16 {
        return Err(format!(
            "{what} holds a format chunk of {} bytes",
            bytes.len(),
        ));
    }
    let mut tag = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
    let channels = u16::from_le_bytes(bytes[2..4].try_into().unwrap());
    let sample_rate = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let block = usize::from(u16::from_le_bytes(bytes[12..14].try_into().unwrap()));
    let bits = u16::from_le_bytes(bytes[14..16].try_into().unwrap());
    if tag == EXTENSIBLE {
        if bytes.len() < EXTENSIBLE_BYTES {
            return Err(format!(
                "{what} holds an extensible format chunk of {} bytes",
                bytes.len(),
            ));
        }
        tag = u16::from_le_bytes(bytes[24..26].try_into().unwrap());
        if !matches!(tag, PCM | IEEE_FLOAT) {
            return Err(format!("{what} holds an extensible format {tag}"));
        }
    }
    let channels = match channels {
        1 => Channels::Mono,
        2 => Channels::Stereo,
        channels => {
            return Err(format!(
                "{what} holds {channels} channels where one or two stand"
            ));
        }
    };
    let bytes_per_sample = usize::from(bits).div_ceil(8);
    if block != channels.count() * bytes_per_sample {
        return Err(format!(
            "{what} holds {block} byte frames of {channels:?} samples of {bits} bits",
        ));
    }
    Ok(Pcm {
        channels,
        sample_rate,
        bits,
        tag,
        block,
    })
}

fn pcm8(data: &[u8]) -> Vec<f32> {
    data.iter()
        .map(|held| (f32::from(*held) - 128.0) / 128.0)
        .collect()
}

fn pcm16(data: &[u8]) -> Vec<f32> {
    data.as_chunks::<2>()
        .0
        .iter()
        .map(|held| f32::from(i16::from_le_bytes(*held)) / 32_768.0)
        .collect()
}

fn pcm24(data: &[u8]) -> Vec<f32> {
    data.as_chunks::<3>()
        .0
        .iter()
        .map(|held| {
            let sign = if held[2] & 0x80 == 0 { 0 } else { 0xFF };
            let held = i32::from_le_bytes([held[0], held[1], held[2], sign]);
            held as f32 / 8_388_608.0
        })
        .collect()
}

fn pcm32(data: &[u8]) -> Vec<f32> {
    data.as_chunks::<4>()
        .0
        .iter()
        .map(|held| i32::from_le_bytes(*held) as f32 / 2_147_483_648.0)
        .collect()
}

fn floats32(data: &[u8]) -> Vec<f32> {
    data.as_chunks::<4>()
        .0
        .iter()
        .map(|held| f32::from_le_bytes(*held))
        .collect()
}

fn floats64(data: &[u8]) -> Vec<f32> {
    data.as_chunks::<8>()
        .0
        .iter()
        .map(|held| f64::from_le_bytes(*held) as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(frames: &[[i16; 2]]) -> Vec<u8> {
        let data = frames
            .iter()
            .flat_map(|frame| frame.iter().flat_map(|sample| sample.to_le_bytes()))
            .collect::<Vec<_>>();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(RIFF);
        bytes.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(WAVE);
        bytes.extend_from_slice(FORMAT);
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&PCM.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&8_000u32.to_le_bytes());
        bytes.extend_from_slice(&32_000u32.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(DATA);
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&data);
        bytes
    }

    #[test]
    fn a_pcm_wave_decodes_interleaved_frames() {
        let bytes = wave(&[[0i16, i16::MAX], [i16::MIN, 0]]);
        let sound = Sound::decode(&bytes, "the test wave");
        assert_eq!(sound.sample_rate(), 8_000);
        assert_eq!(sound.channels(), Channels::Stereo);
        assert_eq!(sound.frame_count(), 2);
        assert_eq!(sound.samples()[0], 0.0);
        assert!((sound.samples()[1] - 1.0).abs() < 1e-4);
        assert_eq!(sound.samples()[2], -1.0);
        assert_eq!(sound.samples()[3], 0.0);
    }

    #[test]
    fn a_float_wave_decodes_extensible_samples() {
        let frames = [0.25f32, -0.5, 0.75, 0.125];
        let mut data = Vec::new();
        for sample in frames {
            data.extend_from_slice(&sample.to_le_bytes());
        }
        let mut bytes = wave(&[]);
        bytes.truncate(12);
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&EXTENSIBLE.to_le_bytes());
        fmt.extend_from_slice(&2u16.to_le_bytes());
        fmt.extend_from_slice(&48_000u32.to_le_bytes());
        fmt.extend_from_slice(&384_000u32.to_le_bytes());
        fmt.extend_from_slice(&8u16.to_le_bytes());
        fmt.extend_from_slice(&32u16.to_le_bytes());
        fmt.extend_from_slice(&22u16.to_le_bytes());
        fmt.extend_from_slice(&32u16.to_le_bytes());
        fmt.extend_from_slice(&3u32.to_le_bytes());
        fmt.extend_from_slice(&[
            3, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
        ]);
        bytes.extend_from_slice(FORMAT);
        bytes.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&fmt);
        bytes.extend_from_slice(DATA);
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&data);
        let sound = Sound::decode(&bytes, "the float wave");
        assert_eq!(sound.channels(), Channels::Stereo);
        assert_eq!(sound.frame_count(), 2);
        assert_eq!(sound.samples(), frames);
    }
}

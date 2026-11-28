use crate::frames;
use crate::sound::Channels;
use crate::wav;
use lewton::inside_ogg::OggStreamReader;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

const RIFF: &[u8; 4] = b"RIFF";
const OGG: &[u8; 4] = b"OggS";
const LOOKAHEAD: usize = 1 << 15;
const BLOCK: usize = 4096;
const TAIL: u64 = 1 << 17;
const UNKNOWN: u64 = u64::MAX;

#[derive(Clone, Debug)]
enum Container {
    Wav(wav::Header),
    Vorbis,
}

pub struct Stream {
    path: PathBuf,
    sample_rate: u32,
    channels: Channels,
    frames: Option<u64>,
    container: Container,
}

impl std::fmt::Debug for Stream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stream")
            .field("path", &self.path)
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .field("frames", &self.frames)
            .finish()
    }
}

impl Stream {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let mut file = File::open(&path)
            .map_err(|error| format!("the sound {} opens no stream: {error}", path.display()))?;
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic).map_err(|error| {
            format!(
                "the sound {} holds no stream header: {error}",
                path.display()
            )
        })?;
        let what = format!("the sound {}", path.display());
        let (sample_rate, channels, frames, container) = if &magic == RIFF {
            file.seek(SeekFrom::Start(0))
                .map_err(|error| format!("{what} seeks no header: {error}"))?;
            let header = wav::header(&mut file, &what)?;
            let pcm = header.pcm();
            (
                pcm.sample_rate(),
                pcm.channels(),
                Some(header.frames()),
                Container::Wav(header),
            )
        } else if &magic == OGG {
            file.seek(SeekFrom::Start(0))
                .map_err(|error| format!("{what} seeks no header: {error}"))?;
            let reader = OggStreamReader::new(BufReader::new(file))
                .map_err(|error| format!("{what} holds no vorbis stream: {error}"))?;
            let channels = match reader.ident_hdr.audio_channels {
                1 => Channels::Mono,
                2 => Channels::Stereo,
                channels => {
                    return Err(format!(
                        "{what} holds {channels} channels where one or two stand"
                    ));
                }
            };
            (
                reader.ident_hdr.audio_sample_rate,
                channels,
                vorbis_frames(&path),
                Container::Vorbis,
            )
        } else {
            return Err(format!("{what} holds no waveform or vorbis stream"));
        };
        if sample_rate == 0 {
            return Err(format!("{what} plays at no sample rate"));
        }
        Ok(Self {
            path,
            sample_rate,
            channels,
            frames,
            container,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> Channels {
        self.channels
    }

    pub fn frame_count(&self) -> Option<u64> {
        self.frames
    }

    pub(crate) fn track(&self, looping: bool, start: u64) -> Result<Track, String> {
        assert!(
            self.frames.is_none_or(|frames| start <= frames),
            "the stream {} starts at frame {start} past its {} frames",
            self.path.display(),
            self.frames.unwrap_or(0),
        );
        let (producer, consumer) = frames::channel(LOOKAHEAD);
        let total = Arc::new(AtomicU64::new(UNKNOWN));
        let held = Arc::clone(&total);
        let period = if looping { self.frames } else { None };
        let path = self.path.clone();
        let container = self.container.clone();
        let channels = self.channels;
        thread::Builder::new()
            .name(String::from("dome audio stream"))
            .spawn(move || {
                let mut producer = producer;
                decode(
                    &path,
                    &container,
                    channels,
                    looping,
                    start,
                    &mut producer,
                    &held,
                );
            })
            .map_err(|error| {
                format!(
                    "the sound {} starts no stream: {error}",
                    self.path.display(),
                )
            })?;
        Ok(Track {
            frames: consumer,
            total,
            period,
            sample_rate: self.sample_rate,
            channels: self.channels,
        })
    }
}

pub(crate) struct Track {
    frames: frames::Consumer,
    total: Arc<AtomicU64>,
    period: Option<u64>,
    sample_rate: u32,
    channels: Channels,
}

impl std::fmt::Debug for Track {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Track")
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .field("total", &self.total())
            .field("period", &self.period)
            .finish()
    }
}

impl Track {
    pub(crate) fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub(crate) fn channels(&self) -> Channels {
        self.channels
    }

    pub(crate) fn take(&mut self) -> Option<[f32; 2]> {
        self.frames.take()
    }

    pub(crate) fn total(&self) -> Option<u64> {
        match self.total.load(Ordering::Acquire) {
            UNKNOWN => None,
            total => Some(total),
        }
    }

    pub(crate) fn period(&self) -> Option<u64> {
        self.period
    }
}

fn decode(
    path: &Path,
    container: &Container,
    channels: Channels,
    looping: bool,
    start: u64,
    producer: &mut frames::Producer,
    total: &AtomicU64,
) {
    let served = match container {
        Container::Wav(header) => {
            decode_wav(path, header.clone(), looping, start, channels, producer)
        }
        Container::Vorbis => decode_vorbis(path, looping, start, producer),
    };
    total.store(start + served, Ordering::Release);
}

fn decode_wav(
    path: &Path,
    header: wav::Header,
    looping: bool,
    start: u64,
    channels: Channels,
    producer: &mut frames::Producer,
) -> u64 {
    let Ok(file) = File::open(path) else {
        return 0;
    };
    let mut reader = wav::Reader::open(file, header);
    if reader.seek(start).is_err() {
        return 0;
    }
    let mut served = 0u64;
    let mut from_zero = start == 0;
    let mut progress = 0u64;
    loop {
        if producer.closed() {
            return served;
        }
        let samples = match reader.read(BLOCK) {
            Ok(samples) => samples,
            Err(_) => return served,
        };
        if samples.is_empty() {
            if !looping || (from_zero && progress == 0) {
                return served;
            }
            if reader.seek(0).is_err() {
                return served;
            }
            from_zero = true;
            progress = 0;
            continue;
        }
        let frames = stereo(&samples, channels);
        served += frames.len() as u64;
        progress += frames.len() as u64;
        if !write(producer, &frames) {
            return served;
        }
    }
}

fn decode_vorbis(path: &Path, looping: bool, start: u64, producer: &mut frames::Producer) -> u64 {
    let mut served = 0u64;
    let mut first = true;
    let mut from_zero = start == 0;
    loop {
        let Ok(file) = File::open(path) else {
            return served;
        };
        let Ok(mut reader) = OggStreamReader::new(BufReader::new(file)) else {
            return served;
        };
        let mut skip = if first { start } else { 0 };
        let channels = reader.ident_hdr.audio_channels;
        let mut pass = 0u64;
        let mut pending: Vec<[f32; 2]> = Vec::new();
        let mut broken = false;
        let mut round = 0u64;
        loop {
            if producer.closed() {
                return served;
            }
            let packet = match reader.read_dec_packet_itl() {
                Ok(Some(packet)) => packet,
                Ok(None) => {
                    if let Some(total) = reader.get_last_absgp() {
                        let ending = if first {
                            total.saturating_sub(start)
                        } else {
                            total
                        };
                        pending.truncate(ending.saturating_sub(pass) as usize);
                    }
                    if !pending.is_empty() {
                        served += pending.len() as u64;
                        round += pending.len() as u64;
                        if !write(producer, &pending) {
                            return served;
                        }
                    }
                    break;
                }
                Err(_) => {
                    broken = true;
                    break;
                }
            };
            if !pending.is_empty() {
                let frames = std::mem::take(&mut pending);
                served += frames.len() as u64;
                pass += frames.len() as u64;
                round += frames.len() as u64;
                if !write(producer, &frames) {
                    return served;
                }
            }
            let frames = match channels {
                1 => packet
                    .iter()
                    .map(|sample| {
                        let held = f32::from(*sample) / 32_768.0;
                        [held, held]
                    })
                    .collect(),
                _ => packet
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| [f32::from(pair[0]) / 32_768.0, f32::from(pair[1]) / 32_768.0])
                    .collect(),
            };
            pending = skip_frames(frames, &mut skip);
        }
        if broken || !looping || (from_zero && round == 0) {
            return served;
        }
        first = false;
        from_zero = true;
    }
}

fn vorbis_frames(path: &Path) -> Option<u64> {
    let mut file = File::open(path).ok()?;
    let length = file.seek(SeekFrom::End(0)).ok()?;
    let start = length.saturating_sub(TAIL);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let at = bytes.windows(4).rposition(|held| held == OGG)?;
    let granule = u64::from_le_bytes(bytes.get(at + 6..at + 14)?.try_into().ok()?);
    (granule != u64::MAX).then_some(granule)
}

fn skip_frames(mut frames: Vec<[f32; 2]>, skip: &mut u64) -> Vec<[f32; 2]> {
    if *skip == 0 {
        return frames;
    }
    let held = (*skip).min(frames.len() as u64) as usize;
    frames.drain(..held);
    *skip -= held as u64;
    frames
}

fn stereo(samples: &[f32], channels: Channels) -> Vec<[f32; 2]> {
    match channels {
        Channels::Mono => samples.iter().map(|sample| [*sample, *sample]).collect(),
        Channels::Stereo => samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| [pair[0], pair[1]])
            .collect(),
    }
}

fn write(producer: &mut frames::Producer, frames: &[[f32; 2]]) -> bool {
    let mut at = 0;
    while at < frames.len() {
        if producer.closed() {
            return false;
        }
        let written = producer.write(&frames[at..]);
        at += written;
        if written == 0 {
            thread::sleep(Duration::from_millis(1));
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound::Sound;
    use std::fs;
    use std::time::Instant;

    const TONE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/tone.ogg");

    fn directory(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("dome_audio_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a test opens its directory");
        root
    }

    fn wave(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let block = channels * 2;
        let data = samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<_>>();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * u32::from(block)).to_le_bytes());
        bytes.extend_from_slice(&block.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&data);
        bytes
    }

    fn drain(stream: &Stream, looping: bool, start: u64) -> Vec<[f32; 2]> {
        let mut track = stream
            .track(looping, start)
            .expect("a test starts its track");
        let mut frames = Vec::new();
        let begin = Instant::now();
        loop {
            while let Some(frame) = track.take() {
                frames.push(frame);
            }
            if let Some(total) = track.total() {
                assert!(
                    track.take().is_none(),
                    "a track of {total} served frames holds none past its end",
                );
                break;
            }
            assert!(
                begin.elapsed() < Duration::from_secs(20),
                "a stream decodes within its test",
            );
            thread::sleep(Duration::from_millis(1));
        }
        frames
    }

    #[test]
    fn a_wave_stream_serves_every_frame_of_its_file() {
        let root = directory("wave");
        let path = root.join("ramp.wav");
        let samples = (0..256)
            .flat_map(|at| [(at * 128) as i16, -((at * 128) as i16)])
            .collect::<Vec<_>>();
        fs::write(&path, wave(8_000, 2, &samples)).expect("a test writes its wave");
        let sound = Sound::load(&path).expect("the wave loads");
        let stream = Stream::open(&path).expect("a test holds a wave stream");
        assert_eq!(stream.frame_count(), Some(sound.frame_count() as u64));
        let frames = drain(&stream, false, 0);
        assert_eq!(frames.len(), sound.frame_count());
        for (at, frame) in frames.iter().enumerate() {
            assert!((frame[0] - sound.samples()[at * 2]).abs() < 1e-6);
            assert!((frame[1] - sound.samples()[at * 2 + 1]).abs() < 1e-6);
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_looping_stream_restarts_the_file_it_plays() {
        let root = directory("loop");
        let path = root.join("ramp.wav");
        let samples = (0..64)
            .flat_map(|at| [(at * 512) as i16, (at * 256) as i16])
            .collect::<Vec<_>>();
        fs::write(&path, wave(8_000, 2, &samples)).expect("a test writes its wave");
        let stream = Stream::open(&path).expect("a test holds a wave stream");
        let mut track = stream.track(true, 0).expect("a test starts its track");
        assert_eq!(track.period(), Some(64));
        assert_eq!(track.total(), None);
        let mut frames = Vec::new();
        let begin = Instant::now();
        while frames.len() < 64 * 2 + 8 {
            while let Some(frame) = track.take() {
                frames.push(frame);
            }
            assert!(begin.elapsed() < Duration::from_secs(20), "a loop restarts");
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(track.total(), None);
        for at in 0..8 {
            assert_eq!(frames[64 + at], frames[at]);
        }

        let stream = Stream::open(TONE).expect("a test opens its vorbis stream");
        let mut track = stream.track(true, 0).expect("a test starts its track");
        let bytes = fs::read(TONE).expect("a test reads its vorbis file");
        let sound = Sound::decode(&bytes, "the tone").expect("the tone decodes");
        let count = sound.frame_count();
        assert_eq!(track.period(), Some(count as u64));
        assert_eq!(track.total(), None);
        let mut frames = Vec::new();
        let begin = Instant::now();
        while frames.len() < count * 2 + 64 {
            while let Some(frame) = track.take() {
                frames.push(frame);
            }
            assert!(begin.elapsed() < Duration::from_secs(20), "a loop restarts");
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(track.total(), None);
        for at in 0..32 {
            assert_eq!(frames[count + at], frames[at]);
        }

        let empty = root.join("silence.wav");
        fs::write(&empty, wave(8_000, 2, &[])).expect("a test writes its empty wave");
        let stream = Stream::open(&empty).expect("a test holds an empty wave stream");
        assert_eq!(stream.frame_count(), Some(0));
        let mut track = stream.track(true, 0).expect("a test starts its track");
        let begin = Instant::now();
        while track.total().is_none() {
            assert!(
                begin.elapsed() < Duration::from_secs(5),
                "an empty loop ends rather than spinning",
            );
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(track.total(), Some(0));
        assert_eq!(track.take(), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_vorbis_stream_decodes_a_tone() {
        let stream = Stream::open(TONE).expect("a test opens its vorbis stream");
        assert_eq!(stream.sample_rate(), 44_100);
        assert_eq!(stream.channels(), Channels::Mono);
        let frames = drain(&stream, false, 0);
        let count = frames.len();
        assert!(
            (43_900..=44_200).contains(&count),
            "a one second tone decodes {count} frames",
        );
        assert!((frames[0][0] - frames[0][1]).abs() < 1e-6);
        let bytes = fs::read(TONE).expect("a test reads its vorbis file");
        let sound = Sound::decode(&bytes, "the tone").expect("the tone decodes");
        assert_eq!(stream.frame_count(), Some(count as u64));
        assert_eq!(count, sound.frame_count());
        for (at, frame) in frames.iter().enumerate() {
            assert_eq!(frame[0], sound.samples()[at]);
            assert_eq!(frame[1], sound.samples()[at]);
        }
    }

    #[test]
    fn a_vorbis_file_decodes_into_a_sound() {
        let bytes = fs::read(TONE).expect("a test reads its vorbis file");
        let sound = Sound::decode(&bytes, "the tone").expect("the tone decodes");
        assert_eq!(sound.sample_rate(), 44_100);
        assert_eq!(sound.channels(), Channels::Mono);
        assert!((43_900..=44_200).contains(&sound.frame_count()));
        assert!(sound.samples().iter().any(|sample| sample.abs() > 0.05));
    }
}

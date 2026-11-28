use crate::sound::{Channels, Sound};
use lewton::inside_ogg::OggStreamReader;
use std::io::Cursor;

const OGG: &[u8; 4] = b"OggS";

pub(crate) fn decode(bytes: &[u8], what: &str) -> Result<Sound, String> {
    if bytes.len() < 4 || &bytes[..4] != OGG {
        return Err(format!("{what} holds no vorbis stream"));
    }
    let mut reader = OggStreamReader::new(Cursor::new(bytes))
        .map_err(|error| format!("{what} holds no vorbis stream: {error}"))?;
    let sample_rate = reader.ident_hdr.audio_sample_rate;
    if sample_rate == 0 {
        return Err(format!("{what} plays at no sample rate"));
    }
    let channels = match reader.ident_hdr.audio_channels {
        1 => Channels::Mono,
        2 => Channels::Stereo,
        channels => {
            return Err(format!(
                "{what} holds {channels} channels where one or two stand"
            ));
        }
    };
    let mut samples = Vec::new();
    loop {
        match reader.read_dec_packet_itl() {
            Ok(Some(packet)) => {
                samples.extend(packet.iter().map(|sample| f32::from(*sample) / 32_768.0));
            }
            Ok(None) => break,
            Err(error) => return Err(format!("{what} decodes no vorbis packet: {error}")),
        }
    }
    if let Some(total) = reader.get_last_absgp() {
        samples.truncate(total as usize * channels.count());
    }
    if samples.is_empty() {
        return Err(format!("{what} holds no sample frame"));
    }
    Ok(Sound::new(sample_rate, channels, samples))
}

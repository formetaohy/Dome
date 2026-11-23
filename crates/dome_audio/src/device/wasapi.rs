use crate::device::{OutputFormat, Render, Unavailable};
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, IAudioClient, IAudioRenderClient,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, eConsole, eRender,
};
use windows::Win32::Media::KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE};
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::IUnknown;

const WAIT_MILLISECONDS: u32 = 20;
const STREAM_BUFFERING: i64 = 0;
const CHANNEL_TAG_PCM: u16 = 1;

struct Com;

impl Com {
    fn new() -> Result<Self, Unavailable> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_err() {
            return Err(Unavailable::new(format!(
                "the audio thread joins a COM apartment: {result:?}",
            )));
        }
        Ok(Self)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct MixFormat(*mut WAVEFORMATEX);

impl MixFormat {
    fn open(client: &IAudioClient) -> Result<Self, Unavailable> {
        let format = unsafe { client.GetMixFormat() }.map_err(|error| {
            Unavailable::new(format!("the audio endpoint names no mix format: {error}"))
        })?;
        Ok(Self(format))
    }

    fn fields(&self) -> (u16, u16, u32, u16, u16, u16) {
        let format = unsafe { ptr::read_unaligned(self.0) };
        (
            format.wFormatTag,
            format.nChannels,
            format.nSamplesPerSec,
            format.wBitsPerSample,
            format.nBlockAlign,
            format.cbSize,
        )
    }

    fn subformat(&self) -> windows::core::GUID {
        let held = unsafe { ptr::read_unaligned(self.0.cast::<WAVEFORMATEXTENSIBLE>()) };
        held.SubFormat
    }
}

impl Drop for MixFormat {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.cast::<c_void>())) };
    }
}

#[derive(Clone, Copy)]
enum Storage {
    Float32,
    Integer16,
    Integer24,
    Integer32,
}

struct Event(HANDLE);

impl Event {
    fn new() -> Result<Self, Unavailable> {
        let handle = unsafe { CreateEventW(None, false, false, None) }
            .map_err(|error| Unavailable::new(format!("an audio event opens: {error}")))?;
        Ok(Self(handle))
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) }.expect("an audio event closes");
    }
}

pub struct Session {
    client: IAudioClient,
    render: IAudioRenderClient,
    event: Event,
    format: OutputFormat,
    storage: Storage,
    device_channels: usize,
    buffer_frames: u32,
    started: bool,
    scratch: Vec<f32>,
    _com: Com,
}

impl Session {
    pub fn open() -> Result<Self, Unavailable> {
        let com = Com::new()?;
        let enumerator = unsafe {
            CoCreateInstance::<_, IMMDeviceEnumerator>(
                &MMDeviceEnumerator,
                None::<&IUnknown>,
                CLSCTX_ALL,
            )
        }
        .map_err(|error| {
            Unavailable::new(format!("the audio endpoint enumerator opens: {error}"))
        })?;
        let endpoint =
            unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }.map_err(|error| {
                Unavailable::new(format!("a default audio render endpoint opens: {error}"))
            })?;
        let client: IAudioClient =
            unsafe { endpoint.Activate(CLSCTX_ALL, None) }.map_err(|error| {
                Unavailable::new(format!("the audio client of the endpoint opens: {error}"))
            })?;
        let mix = MixFormat::open(&client)?;
        let (tag, channels, rate, bits, block_align, extra) = mix.fields();
        let subformat = if tag == WAVE_FORMAT_EXTENSIBLE as u16 {
            mix.subformat()
        } else {
            windows::core::GUID::zeroed()
        };
        let storage = storage_of(tag, bits, extra, subformat)?;
        let device_channels = usize::from(channels);
        if device_channels == 0 || block_align == 0 {
            return Err(Unavailable::new(format!(
                "an audio endpoint of {device_channels} channels and {block_align} bytes belts no frame",
            )));
        }
        let event = Event::new()?;
        unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                STREAM_BUFFERING,
                STREAM_BUFFERING,
                mix.0,
                None,
            )
        }
        .map_err(|error| Unavailable::new(format!("the audio client starts: {error}")))?;
        unsafe { client.SetEventHandle(event.0) }
            .map_err(|error| Unavailable::new(format!("the audio client waits: {error}")))?;
        let render: IAudioRenderClient = unsafe { client.GetService() }
            .map_err(|error| Unavailable::new(format!("the audio renderer opens: {error}")))?;
        let buffer_frames = unsafe { client.GetBufferSize() }
            .map_err(|error| Unavailable::new(format!("the audio buffer sizes: {error}")))?;
        if buffer_frames == 0 {
            return Err(Unavailable::new(
                "an audio endpoint holds a buffer of no frame",
            ));
        }
        Ok(Self {
            client,
            render,
            event,
            format: OutputFormat::new(rate, 2),
            storage,
            device_channels,
            buffer_frames,
            started: false,
            scratch: vec![0.0; buffer_frames as usize * 2],
            _com: com,
        })
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    pub fn play(
        mut self,
        mut render: Render,
        stop: &AtomicBool,
        ready: &dyn Fn(),
    ) -> Result<(), Unavailable> {
        unsafe { self.client.Start() }
            .map_err(|error| Unavailable::new(format!("the audio client plays: {error}")))?;
        self.started = true;
        ready();
        while !stop.load(Ordering::Acquire) {
            let waited = unsafe { WaitForSingleObject(self.event.0, WAIT_MILLISECONDS) };
            if waited == WAIT_TIMEOUT {
                continue;
            }
            if waited != WAIT_OBJECT_0 {
                break;
            }
            let Ok(padding) = (unsafe { self.client.GetCurrentPadding() }) else {
                break;
            };
            let available = self.buffer_frames.saturating_sub(padding);
            if available == 0 {
                continue;
            }
            let frames = available as usize;
            let Ok(destination) = (unsafe { self.render.GetBuffer(available) }) else {
                break;
            };
            render(&mut self.scratch[..frames * 2]);
            self.write(destination, frames);
            if unsafe { self.render.ReleaseBuffer(available, 0) }.is_err() {
                break;
            }
        }
        let _ = unsafe { self.client.Stop() };
        self.started = false;
        Ok(())
    }

    fn channel_value(&self, left: f32, right: f32, channel: usize) -> f32 {
        if self.device_channels == 1 {
            return (left + right) * 0.5;
        }
        match channel {
            0 => left,
            1 => right,
            _ => 0.0,
        }
    }

    fn write(&self, destination: *mut u8, frames: usize) {
        let source = &self.scratch[..frames * 2];
        match self.storage {
            Storage::Float32 => {
                for frame in 0..frames {
                    let left = source[frame * 2];
                    let right = source[frame * 2 + 1];
                    for channel in 0..self.device_channels {
                        let value = self.channel_value(left, right, channel);
                        unsafe {
                            destination
                                .cast::<f32>()
                                .add(frame * self.device_channels + channel)
                                .write(value);
                        }
                    }
                }
            }
            Storage::Integer16 => {
                for frame in 0..frames {
                    let left = source[frame * 2];
                    let right = source[frame * 2 + 1];
                    for channel in 0..self.device_channels {
                        let value = self.channel_value(left, right, channel);
                        let held = (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
                        unsafe {
                            destination
                                .cast::<i16>()
                                .add(frame * self.device_channels + channel)
                                .write(held);
                        }
                    }
                }
            }
            Storage::Integer24 => {
                for frame in 0..frames {
                    let left = source[frame * 2];
                    let right = source[frame * 2 + 1];
                    for channel in 0..self.device_channels {
                        let value = self.channel_value(left, right, channel);
                        let held = (value.clamp(-1.0, 1.0) * 8_388_607.0) as i32;
                        let bytes = held.to_le_bytes();
                        let at = (frame * self.device_channels + channel) * 3;
                        unsafe {
                            ptr::copy_nonoverlapping(bytes.as_ptr(), destination.add(at), 3);
                        }
                    }
                }
            }
            Storage::Integer32 => {
                for frame in 0..frames {
                    let left = source[frame * 2];
                    let right = source[frame * 2 + 1];
                    for channel in 0..self.device_channels {
                        let value = self.channel_value(left, right, channel);
                        let held = (value.clamp(-1.0, 1.0) * 2_147_483_647.0) as i32;
                        unsafe {
                            destination
                                .cast::<i32>()
                                .add(frame * self.device_channels + channel)
                                .write(held);
                        }
                    }
                }
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.started {
            let _ = unsafe { self.client.Stop() };
        }
    }
}

fn storage_of(
    tag: u16,
    bits: u16,
    extra: u16,
    subformat: windows::core::GUID,
) -> Result<Storage, Unavailable> {
    let tag = if tag == WAVE_FORMAT_EXTENSIBLE as u16 {
        if extra < size_of::<WAVEFORMATEXTENSIBLE>() as u16 - size_of::<WAVEFORMATEX>() as u16 {
            return Err(Unavailable::new(format!(
                "an extensible audio format of {extra} extra bytes holds no subformat",
            )));
        }
        if subformat == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
            WAVE_FORMAT_IEEE_FLOAT as u16
        } else if subformat == KSDATAFORMAT_SUBTYPE_PCM {
            CHANNEL_TAG_PCM
        } else {
            return Err(Unavailable::new(format!(
                "an audio endpoint mixes the unknown sample format {subformat:?}",
            )));
        }
    } else {
        tag
    };
    let storage = match (tag, bits) {
        (tag, 32) if tag == WAVE_FORMAT_IEEE_FLOAT as u16 => Storage::Float32,
        (CHANNEL_TAG_PCM, 16) => Storage::Integer16,
        (CHANNEL_TAG_PCM, 24) => Storage::Integer24,
        (CHANNEL_TAG_PCM, 32) => Storage::Integer32,
        _ => {
            return Err(Unavailable::new(format!(
                "an audio endpoint mixes {bits} bit samples of format {tag}",
            )));
        }
    };
    Ok(storage)
}

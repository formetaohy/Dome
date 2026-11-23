use crate::device::{OutputFormat, Render, Unavailable};
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

const COMPONENT_OUTPUT: u32 = u32::from_be_bytes(*b"auou");
const COMPONENT_APPLE: u32 = u32::from_be_bytes(*b"appl");
#[cfg(target_os = "macos")]
const COMPONENT_DEFAULT_OUTPUT: u32 = u32::from_be_bytes(*b"def ");
#[cfg(target_os = "ios")]
const COMPONENT_DEFAULT_OUTPUT: u32 = u32::from_be_bytes(*b"rioc");
const SCOPE_INPUT: u32 = 1;
const PROPERTY_STREAM_FORMAT: u32 = 8;
const PROPERTY_RENDER_CALLBACK: u32 = 23;
const FORMAT_LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");
const FLAG_FLOAT: u32 = 1;
const FLAG_PACKED: u32 = 1 << 3;
const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u32 = 2;
const STOP_POLL: Duration = Duration::from_millis(10);

#[repr(C)]
struct ComponentDescription {
    component_type: u32,
    component_sub_type: u32,
    component_manufacturer: u32,
    component_flags: u32,
    component_flags_mask: u32,
}

#[repr(C)]
struct StreamDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

#[repr(C)]
struct AudioBuffer {
    number_channels: u32,
    data_byte_size: u32,
    data: *mut c_void,
}

#[repr(C)]
struct AudioBufferList {
    number_buffers: u32,
    buffers: [AudioBuffer; 1],
}

type OutputCallback = unsafe extern "C" fn(
    *mut c_void,
    *mut u32,
    *const c_void,
    u32,
    u32,
    *mut AudioBufferList,
) -> i32;

#[repr(C)]
struct RenderCallback {
    input_proc: OutputCallback,
    input_proc_ref_con: *mut c_void,
}

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioComponentFindNext(
        component: *mut c_void,
        description: *const ComponentDescription,
    ) -> *mut c_void;
    fn AudioComponentInstanceNew(component: *mut c_void, instance: *mut *mut c_void) -> i32;
    fn AudioComponentInstanceDispose(instance: *mut c_void) -> i32;
    fn AudioUnitSetProperty(
        unit: *mut c_void,
        property: u32,
        scope: u32,
        element: u32,
        data: *const c_void,
        size: u32,
    ) -> i32;
    fn AudioUnitInitialize(unit: *mut c_void) -> i32;
    fn AudioUnitUninitialize(unit: *mut c_void) -> i32;
    fn AudioOutputUnitStart(unit: *mut c_void) -> i32;
    fn AudioOutputUnitStop(unit: *mut c_void) -> i32;
}

pub struct Session {
    unit: *mut c_void,
    format: OutputFormat,
}

impl Session {
    pub fn open() -> Result<Self, Unavailable> {
        let description = ComponentDescription {
            component_type: COMPONENT_OUTPUT,
            component_sub_type: COMPONENT_DEFAULT_OUTPUT,
            component_manufacturer: COMPONENT_APPLE,
            component_flags: 0,
            component_flags_mask: 0,
        };
        let component = unsafe { AudioComponentFindNext(ptr::null_mut(), &description) };
        if component.is_null() {
            return Err(Unavailable::new(
                "the system holds no default audio output unit",
            ));
        }
        let mut unit: *mut c_void = ptr::null_mut();
        let created = unsafe { AudioComponentInstanceNew(component, &mut unit) };
        if created != 0 || unit.is_null() {
            return Err(Unavailable::new(format!(
                "the default audio output unit opens: {created}",
            )));
        }
        let session = Self {
            unit,
            format: OutputFormat::new(SAMPLE_RATE, 2),
        };
        session.describe()?;
        Ok(session)
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    fn describe(&self) -> Result<(), Unavailable> {
        let description = StreamDescription {
            sample_rate: f64::from(SAMPLE_RATE),
            format_id: FORMAT_LINEAR_PCM,
            format_flags: FLAG_FLOAT | FLAG_PACKED,
            bytes_per_packet: CHANNELS * size_of::<f32>() as u32,
            frames_per_packet: 1,
            bytes_per_frame: CHANNELS * size_of::<f32>() as u32,
            channels_per_frame: CHANNELS,
            bits_per_channel: 32,
            reserved: 0,
        };
        let status = unsafe {
            AudioUnitSetProperty(
                self.unit,
                PROPERTY_STREAM_FORMAT,
                SCOPE_INPUT,
                0,
                (&description as *const StreamDescription).cast(),
                size_of::<StreamDescription>() as u32,
            )
        };
        if status != 0 {
            return Err(Unavailable::new(format!(
                "the audio output unit accepts a stereo float stream: {status}",
            )));
        }
        Ok(())
    }

    pub fn play(
        self,
        render: Render,
        stop: &AtomicBool,
        ready: &dyn Fn(),
    ) -> Result<(), Unavailable> {
        let mut render = Box::new(render);
        let callback = RenderCallback {
            input_proc: output,
            input_proc_ref_con: (&mut *render as *mut Render).cast(),
        };
        let status = unsafe {
            AudioUnitSetProperty(
                self.unit,
                PROPERTY_RENDER_CALLBACK,
                SCOPE_INPUT,
                0,
                (&callback as *const RenderCallback).cast(),
                size_of::<RenderCallback>() as u32,
            )
        };
        if status != 0 {
            return Err(Unavailable::new(format!(
                "the audio output unit accepts a render callback: {status}",
            )));
        }
        let status = unsafe { AudioUnitInitialize(self.unit) };
        if status != 0 {
            return Err(Unavailable::new(format!(
                "the audio output unit initializes: {status}",
            )));
        }
        let status = unsafe { AudioOutputUnitStart(self.unit) };
        if status != 0 {
            return Err(Unavailable::new(format!(
                "the audio output unit starts: {status}",
            )));
        }
        ready();
        while !stop.load(Ordering::Acquire) {
            thread::sleep(STOP_POLL);
        }
        let _ = unsafe { AudioOutputUnitStop(self.unit) };
        let _ = unsafe { AudioUnitUninitialize(self.unit) };
        drop(render);
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = unsafe { AudioComponentInstanceDispose(self.unit) };
    }
}

unsafe extern "C" fn output(
    ref_con: *mut c_void,
    _flags: *mut u32,
    _time: *const c_void,
    _bus: u32,
    frames: u32,
    data: *mut AudioBufferList,
) -> i32 {
    if ref_con.is_null() || data.is_null() {
        return 0;
    }
    let render = unsafe { &mut *ref_con.cast::<Render>() };
    let list = unsafe { &mut *data };
    if list.number_buffers == 0 {
        return 0;
    }
    let buffer = unsafe { &mut *list.buffers.as_mut_ptr() };
    if buffer.data.is_null() {
        return 0;
    }
    let samples = frames as usize * CHANNELS as usize;
    let samples = unsafe { std::slice::from_raw_parts_mut(buffer.data.cast::<f32>(), samples) };
    render(samples);
    buffer.data_byte_size = (samples.len() * size_of::<f32>()) as u32;
    0
}

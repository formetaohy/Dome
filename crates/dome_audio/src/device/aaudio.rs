use crate::device::{OutputFormat, Render, Unavailable};
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

const DIRECTION_OUTPUT: i32 = 0;
const SHARING_SHARED: i32 = 1;
const FORMAT_PCM_FLOAT: i32 = 2;
const PERFORMANCE_LOW_LATENCY: i32 = 12;
const CHANNELS: i32 = 2;
const FRAMES: usize = 512;
const TIMEOUT_NANOS: i64 = 1_000_000_000;

#[link(name = "aaudio")]
unsafe extern "C" {
    fn AAudio_createStreamBuilder(builder: *mut *mut c_void) -> i32;
    fn AAudioStreamBuilder_setDirection(builder: *mut c_void, direction: i32);
    fn AAudioStreamBuilder_setSharingMode(builder: *mut c_void, sharing: i32);
    fn AAudioStreamBuilder_setFormat(builder: *mut c_void, format: i32);
    fn AAudioStreamBuilder_setChannelCount(builder: *mut c_void, channels: i32);
    fn AAudioStreamBuilder_setPerformanceMode(builder: *mut c_void, mode: i32);
    fn AAudioStreamBuilder_openStream(builder: *mut c_void, stream: *mut *mut c_void) -> i32;
    fn AAudioStreamBuilder_delete(builder: *mut c_void) -> i32;
    fn AAudioStream_getSampleRate(stream: *mut c_void) -> i32;
    fn AAudioStream_getChannelCount(stream: *mut c_void) -> i32;
    fn AAudioStream_requestStart(stream: *mut c_void) -> i32;
    fn AAudioStream_requestStop(stream: *mut c_void) -> i32;
    fn AAudioStream_write(
        stream: *mut c_void,
        buffer: *const c_void,
        frames: i32,
        timeout_nanos: i64,
    ) -> i32;
    fn AAudioStream_close(stream: *mut c_void) -> i32;
}

pub struct Session {
    stream: *mut c_void,
    format: OutputFormat,
}

impl Session {
    pub fn open() -> Result<Self, Unavailable> {
        let mut builder: *mut c_void = ptr::null_mut();
        let created = unsafe { AAudio_createStreamBuilder(&mut builder) };
        if created != 0 || builder.is_null() {
            return Err(Unavailable::new(format!(
                "the AAudio stream builder opens: {created}",
            )));
        }
        unsafe {
            AAudioStreamBuilder_setDirection(builder, DIRECTION_OUTPUT);
            AAudioStreamBuilder_setSharingMode(builder, SHARING_SHARED);
            AAudioStreamBuilder_setFormat(builder, FORMAT_PCM_FLOAT);
            AAudioStreamBuilder_setChannelCount(builder, CHANNELS);
            AAudioStreamBuilder_setPerformanceMode(builder, PERFORMANCE_LOW_LATENCY);
        }
        let mut stream: *mut c_void = ptr::null_mut();
        let opened = unsafe { AAudioStreamBuilder_openStream(builder, &mut stream) };
        unsafe { AAudioStreamBuilder_delete(builder) };
        if opened != 0 || stream.is_null() {
            return Err(Unavailable::new(format!(
                "the AAudio output stream opens: {opened}",
            )));
        }
        let rate = unsafe { AAudioStream_getSampleRate(stream) };
        let channels = unsafe { AAudioStream_getChannelCount(stream) };
        if rate <= 0 || channels != CHANNELS {
            unsafe { AAudioStream_close(stream) };
            return Err(Unavailable::new(format!(
                "the AAudio output stream plays {channels} channels at {rate} hertz",
            )));
        }
        Ok(Self {
            stream,
            format: OutputFormat::new(rate as u32, 2),
        })
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    pub fn play(
        self,
        mut render: Render,
        stop: &AtomicBool,
        ready: &dyn Fn(),
    ) -> Result<(), Unavailable> {
        let started = unsafe { AAudioStream_requestStart(self.stream) };
        if started != 0 {
            return Err(Unavailable::new(format!(
                "the AAudio output stream starts: {started}",
            )));
        }
        ready();
        let mut block = vec![0.0f32; FRAMES * 2];
        'playing: while !stop.load(Ordering::Acquire) {
            render(&mut block);
            let mut offset = 0usize;
            while offset < FRAMES {
                let remaining = (FRAMES - offset) as i32;
                let written = unsafe {
                    AAudioStream_write(
                        self.stream,
                        block.as_ptr().add(offset * 2).cast(),
                        remaining,
                        TIMEOUT_NANOS,
                    )
                };
                if written < 0 {
                    break 'playing;
                }
                if written == 0 {
                    break 'playing;
                }
                offset += written as usize;
            }
        }
        let _ = unsafe { AAudioStream_requestStop(self.stream) };
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = unsafe { AAudioStream_requestStop(self.stream) };
        let _ = unsafe { AAudioStream_close(self.stream) };
    }
}

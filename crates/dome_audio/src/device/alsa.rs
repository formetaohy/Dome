use crate::device::{OutputFormat, Render, Unavailable};
use libloading::{Library, Symbol};
use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

const STREAM_PLAYBACK: c_int = 0;
const MODE_BLOCKING: c_int = 0;
const ACCESS_RW_INTERLEAVED: c_int = 3;
const FORMAT_FLOAT_LE: c_int = 14;
const CHANNELS: c_uint = 2;
const SAMPLE_RATE: c_uint = 48_000;
const PERIOD_FRAMES: c_ulong = 512;

type Pcm = *mut c_void;
type HwParams = *mut c_void;

struct Fns {
    open: unsafe extern "C" fn(*mut Pcm, *const c_char, c_int, c_int) -> c_int,
    close: unsafe extern "C" fn(Pcm) -> c_int,
    prepare: unsafe extern "C" fn(Pcm) -> c_int,
    hw_params_malloc: unsafe extern "C" fn(*mut HwParams) -> c_int,
    hw_params_free: unsafe extern "C" fn(HwParams),
    hw_params_any: unsafe extern "C" fn(Pcm, HwParams) -> c_int,
    hw_params_set_access: unsafe extern "C" fn(Pcm, HwParams, c_int) -> c_int,
    hw_params_set_format: unsafe extern "C" fn(Pcm, HwParams, c_int) -> c_int,
    hw_params_set_channels: unsafe extern "C" fn(Pcm, HwParams, c_uint) -> c_int,
    hw_params_set_rate_near: unsafe extern "C" fn(Pcm, HwParams, *mut c_uint, *mut c_int) -> c_int,
    hw_params_set_period_size_near:
        unsafe extern "C" fn(Pcm, HwParams, *mut c_ulong, *mut c_int) -> c_int,
    hw_params: unsafe extern "C" fn(Pcm, HwParams) -> c_int,
    recover: unsafe extern "C" fn(Pcm, c_int, c_int) -> c_int,
    writei: unsafe extern "C" fn(Pcm, *const c_void, c_ulong) -> c_long,
}

impl Fns {
    fn load(library: &Library) -> Result<Self, Unavailable> {
        Ok(Self {
            open: symbol(library, b"snd_pcm_open\0")?,
            close: symbol(library, b"snd_pcm_close\0")?,
            prepare: symbol(library, b"snd_pcm_prepare\0")?,
            hw_params_malloc: symbol(library, b"snd_pcm_hw_params_malloc\0")?,
            hw_params_free: symbol(library, b"snd_pcm_hw_params_free\0")?,
            hw_params_any: symbol(library, b"snd_pcm_hw_params_any\0")?,
            hw_params_set_access: symbol(library, b"snd_pcm_hw_params_set_access\0")?,
            hw_params_set_format: symbol(library, b"snd_pcm_hw_params_set_format\0")?,
            hw_params_set_channels: symbol(library, b"snd_pcm_hw_params_set_channels\0")?,
            hw_params_set_rate_near: symbol(library, b"snd_pcm_hw_params_set_rate_near\0")?,
            hw_params_set_period_size_near: symbol(
                library,
                b"snd_pcm_hw_params_set_period_size_near\0",
            )?,
            hw_params: symbol(library, b"snd_pcm_hw_params\0")?,
            recover: symbol(library, b"snd_pcm_recover\0")?,
            writei: symbol(library, b"snd_pcm_writei\0")?,
        })
    }
}

fn symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T, Unavailable> {
    let symbol: Symbol<'_, T> = unsafe { library.get(name) }.map_err(|error| {
        Unavailable::new(format!(
            "the ALSA library holds no {}: {error}",
            String::from_utf8_lossy(name).trim_end_matches('\0'),
        ))
    })?;
    Ok(*symbol)
}

pub struct Session {
    _library: Library,
    fns: Fns,
    pcm: Pcm,
    format: OutputFormat,
}

impl Session {
    pub fn open() -> Result<Self, Unavailable> {
        let library = unsafe { Library::new("libasound.so.2") }
            .map_err(|error| Unavailable::new(format!("the ALSA library opens: {error}")))?;
        let fns = Fns::load(&library)?;
        let mut pcm: Pcm = ptr::null_mut();
        let name = b"default\0";
        let opened = unsafe {
            (fns.open)(
                &mut pcm,
                name.as_ptr().cast(),
                STREAM_PLAYBACK,
                MODE_BLOCKING,
            )
        };
        if opened < 0 || pcm.is_null() {
            return Err(Unavailable::new(format!(
                "the default ALSA device opens: {opened}",
            )));
        }
        let mut params: HwParams = ptr::null_mut();
        let result = configure(&fns, pcm, &mut params);
        match result {
            Ok(rate) => Ok(Self {
                _library: library,
                fns,
                pcm,
                format: OutputFormat::new(rate, 2),
            }),
            Err(error) => {
                unsafe { (fns.close)(pcm) };
                Err(error)
            }
        }
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
        ready();
        let mut block = vec![0.0f32; PERIOD_FRAMES as usize * 2];
        'playing: while !stop.load(Ordering::Acquire) {
            render(&mut block);
            let mut offset = 0usize;
            while offset < PERIOD_FRAMES as usize {
                let remaining = PERIOD_FRAMES as c_ulong - offset as c_ulong;
                let written = unsafe {
                    (self.fns.writei)(self.pcm, block.as_ptr().add(offset * 2).cast(), remaining)
                };
                if written < 0 {
                    let recovered = unsafe { (self.fns.recover)(self.pcm, written as c_int, 1) };
                    if recovered < 0 {
                        break 'playing;
                    }
                    continue;
                }
                if written == 0 {
                    break 'playing;
                }
                offset += written as usize;
            }
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { (self.fns.close)(self.pcm) };
    }
}

fn configure(fns: &Fns, pcm: Pcm, params: &mut HwParams) -> Result<u32, Unavailable> {
    let allocated = unsafe { (fns.hw_params_malloc)(params) };
    if allocated < 0 || params.is_null() {
        return Err(Unavailable::new(format!(
            "the ALSA device allocates its hardware parameters: {allocated}",
        )));
    }
    let result = apply(fns, pcm, *params);
    unsafe { (fns.hw_params_free)(*params) };
    result
}

fn apply(fns: &Fns, pcm: Pcm, params: HwParams) -> Result<u32, Unavailable> {
    check(
        unsafe { (fns.hw_params_any)(pcm, params) },
        "choose parameters",
    )?;
    check(
        unsafe { (fns.hw_params_set_access)(pcm, params, ACCESS_RW_INTERLEAVED) },
        "interleave frames",
    )?;
    check(
        unsafe { (fns.hw_params_set_format)(pcm, params, FORMAT_FLOAT_LE) },
        "carry float frames",
    )?;
    check(
        unsafe { (fns.hw_params_set_channels)(pcm, params, CHANNELS) },
        "carry two channels",
    )?;
    let mut rate = SAMPLE_RATE;
    let mut direction = 0;
    check(
        unsafe { (fns.hw_params_set_rate_near)(pcm, params, &mut rate, &mut direction) },
        "name a sample rate",
    )?;
    if rate == 0 {
        return Err(Unavailable::new("the ALSA device names no sample rate"));
    }
    let mut period = PERIOD_FRAMES;
    let mut direction = 0;
    check(
        unsafe { (fns.hw_params_set_period_size_near)(pcm, params, &mut period, &mut direction) },
        "name a period",
    )?;
    check(
        unsafe { (fns.hw_params)(pcm, params) },
        "apply its parameters",
    )?;
    check(unsafe { (fns.prepare)(pcm) }, "prepare its buffer")?;
    Ok(rate)
}

fn check(code: c_int, what: &str) -> Result<(), Unavailable> {
    if code < 0 {
        return Err(Unavailable::new(format!(
            "the ALSA device cannot {what}: {code}",
        )));
    }
    Ok(())
}

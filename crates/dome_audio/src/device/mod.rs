#[cfg(target_os = "windows")]
mod wasapi;

#[cfg(target_os = "windows")]
use wasapi::Session;

#[cfg(target_os = "linux")]
mod alsa;

#[cfg(target_os = "linux")]
use alsa::Session;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod coreaudio;

#[cfg(any(target_os = "macos", target_os = "ios"))]
use coreaudio::Session;

#[cfg(target_os = "android")]
mod aaudio;

#[cfg(target_os = "android")]
use aaudio::Session;

#[cfg(not(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "macos",
    target_os = "ios",
    target_os = "android"
)))]
compile_error!("no audio device backend targets this platform");

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::{self, JoinHandle};

pub type Render = Box<dyn FnMut(&mut [f32]) + Send>;

const THREAD: &str = "dome audio";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputFormat {
    sample_rate: u32,
    channels: u16,
}

impl OutputFormat {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        assert!(
            sample_rate > 0,
            "an audio output format of {sample_rate} hertz plays no frame",
        );
        assert!(
            channels == 2,
            "an audio output format of {channels} channels does not mix stereo frames",
        );
        Self {
            sample_rate,
            channels,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[derive(Clone, Debug)]
pub struct Unavailable(String);

impl Unavailable {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Unavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Unavailable {}

pub struct Output {
    format: OutputFormat,
    render: Option<SyncSender<Render>>,
    ready: Receiver<Result<(), Unavailable>>,
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Output {
    pub fn open() -> Result<Self, Unavailable> {
        let (format_send, format_recv) = sync_channel(1);
        let (render_send, render_recv) = sync_channel(1);
        let (ready_send, ready_recv) = sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let thread_stop = Arc::clone(&stop);
        let thread_alive = Arc::clone(&alive);
        let join = thread::Builder::new()
            .name(String::from(THREAD))
            .spawn(move || {
                let session = match Session::open() {
                    Ok(session) => session,
                    Err(error) => {
                        let _ = format_send.send(Err(error.clone()));
                        let _ = ready_send.send(Err(error));
                        thread_alive.store(false, Ordering::Release);
                        return;
                    }
                };
                if format_send.send(Ok(session.format())).is_err() {
                    thread_alive.store(false, Ordering::Release);
                    return;
                }
                let Ok(render) = render_recv.recv() else {
                    thread_alive.store(false, Ordering::Release);
                    return;
                };
                let ready = || {
                    let _ = ready_send.send(Ok(()));
                };
                if let Err(error) = session.play(render, &thread_stop, &ready) {
                    let _ = ready_send.send(Err(error));
                }
                thread_alive.store(false, Ordering::Release);
            })
            .expect("the audio device thread spawns");
        let format = match format_recv.recv() {
            Ok(Ok(format)) => format,
            Ok(Err(error)) => {
                let _ = join.join();
                return Err(error);
            }
            Err(_) => {
                let _ = join.join();
                return Err(Unavailable::new(
                    "the audio device thread stops before it names an output format",
                ));
            }
        };
        Ok(Self {
            format,
            render: Some(render_send),
            ready: ready_recv,
            stop,
            alive,
            join: Some(join),
        })
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    pub fn start(mut self, render: Render) -> Result<Stream, Unavailable> {
        let Some(sender) = self.render.take() else {
            panic!("an audio output starts one stream");
        };
        if sender.send(render).is_err() {
            self.stop();
            return Err(Unavailable::new(
                "the audio device thread stops before it plays a frame",
            ));
        }
        match self.ready.recv() {
            Ok(Ok(())) => {
                let join = self.join.take();
                Ok(Stream {
                    stop: Arc::clone(&self.stop),
                    alive: Arc::clone(&self.alive),
                    join,
                })
            }
            Ok(Err(error)) => {
                self.stop();
                Err(error)
            }
            Err(_) => {
                self.stop();
                Err(Unavailable::new(
                    "the audio device thread stops before it starts its stream",
                ))
            }
        }
    }

    fn stop(&mut self) {
        let Some(join) = self.join.take() else {
            return;
        };
        self.stop.store(true, Ordering::Release);
        self.render = None;
        let _ = join.join();
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct Stream {
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Stream {
    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

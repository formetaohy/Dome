use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command as Process, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

const TAIL_BYTES: usize = 4096;

pub(crate) struct Exit {
    pub(crate) code: Option<i32>,
    pub(crate) tail: String,
}

impl Exit {
    pub(crate) fn failed(&self) -> bool {
        self.code != Some(0)
    }
}

pub(crate) struct Player {
    child: Child,
    tail: Arc<Mutex<String>>,
    reader: Option<JoinHandle<()>>,
}

impl Player {
    pub(crate) fn start(root: &Path, debug: bool) -> Self {
        let mut process =
            Process::new(std::env::current_exe().expect("the editor names its own executable"));
        process
            .arg("--play")
            .arg(root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if debug {
            process.arg("--debug");
        }
        let mut child = process
            .spawn()
            .unwrap_or_else(|error| panic!("the editor starts its player: {error}"));
        let tail = Arc::new(Mutex::new(String::new()));
        let held = Arc::clone(&tail);
        let stderr = child
            .stderr
            .take()
            .expect("a launched player holds the error pipe it reports through");
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else {
                    return;
                };
                let mut tail = held.lock().expect("a player error tail is never poisoned");
                tail.push_str(&line);
                tail.push('\n');
                if tail.len() > TAIL_BYTES {
                    let cut = tail.len() - TAIL_BYTES;
                    let start = tail.ceil_char_boundary(cut);
                    tail.drain(..start);
                }
            }
        });
        Self {
            child,
            tail,
            reader: Some(reader),
        }
    }

    pub(crate) fn exited(&mut self) -> Option<Exit> {
        let status = self
            .child
            .try_wait()
            .unwrap_or_else(|error| panic!("the editor watches its player: {error}"))?;
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        let tail = self
            .tail
            .lock()
            .expect("a player error tail is never poisoned")
            .trim()
            .to_string();
        Some(Exit {
            code: status.code(),
            tail,
        })
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

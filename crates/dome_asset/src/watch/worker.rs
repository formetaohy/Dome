use super::{Change, Kind, Message, Report};
use crate::digest::Digest;
use dome_gltf::Model;
use dome_scene::Folder;
use notify::{Event, RecursiveMode, Watcher};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const QUIET: Duration = Duration::from_millis(60);
const IDLE: Duration = Duration::from_secs(3600);
const EXTENSION: &str = "amp";

struct Dependencies {
    kind: Kind,
    digest: Option<Digest>,
    paths: Vec<PathBuf>,
}

struct Tracked {
    kind: Kind,
    digest: Option<Digest>,
    dependencies: Option<Dependencies>,
}

impl Tracked {
    fn new(kind: Kind, digest: Option<Digest>) -> Self {
        Self {
            kind,
            digest,
            dependencies: None,
        }
    }
}

pub(super) fn spawn(
    requests: Receiver<Message>,
    commands: Sender<Message>,
    reports: Sender<Report>,
    wake: Arc<dyn Fn() + Send + Sync>,
    project: PathBuf,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name(String::from("dome watch"))
        .spawn(move || {
            Worker {
                requests,
                commands,
                reports,
                wake,
                project,
                files: BTreeMap::new(),
                watched: BTreeSet::new(),
                derived: BTreeSet::new(),
                dirty: BTreeSet::new(),
                settle: None,
                watcher: None,
                watch_errors: Vec::new(),
                dependency_errors: Vec::new(),
                sent: Vec::new(),
            }
            .run()
        })
        .expect("the file watcher thread spawns")
}

struct Worker {
    requests: Receiver<Message>,
    commands: Sender<Message>,
    reports: Sender<Report>,
    wake: Arc<dyn Fn() + Send + Sync>,
    project: PathBuf,
    files: BTreeMap<PathBuf, Tracked>,
    watched: BTreeSet<PathBuf>,
    derived: BTreeSet<PathBuf>,
    dirty: BTreeSet<PathBuf>,
    settle: Option<Instant>,
    watcher: Option<notify::RecommendedWatcher>,
    watch_errors: Vec<String>,
    dependency_errors: Vec<String>,
    sent: Vec<String>,
}

impl Worker {
    fn run(mut self) {
        self.watch();
        loop {
            let timeout = match self.settle {
                Some(at) => at.saturating_duration_since(Instant::now()),
                None => IDLE,
            };
            match self.requests.recv_timeout(timeout) {
                Ok(Message::Set(desired)) => self.set(desired),
                Ok(Message::Adopt(path)) => self.adopt(&path),
                Ok(Message::Event(event)) => self.observe(event),
                Ok(Message::Flush(flushed)) => {
                    let _ = flushed.send(());
                }
                Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {
                    self.settle = None;
                    self.settle_now();
                }
            }
        }
    }

    fn watch(&mut self) {
        let commands = self.commands.clone();
        let config = notify::Config::default().with_event_kinds(notify::EventKindMask::CORE);
        let watcher = notify::RecommendedWatcher::new(
            move |result: notify::Result<Event>| {
                let _ = commands.send(Message::Event(result));
            },
            config,
        );
        let mut watcher = match watcher {
            Ok(watcher) => watcher,
            Err(error) => {
                self.fail(format!(
                    "the file watcher of {} opens: {error}",
                    self.project.display(),
                ));
                return;
            }
        };
        match watcher.watch(&self.project, RecursiveMode::Recursive) {
            Ok(()) => {
                self.watched.insert(self.project.clone());
                self.watcher = Some(watcher);
            }
            Err(error) => self.fail(format!(
                "the file watcher of {} watches: {error}",
                self.project.display(),
            )),
        }
    }

    fn fail(&mut self, message: String) {
        self.watch_errors.push(message);
        self.publish(Change::default());
    }

    fn set(&mut self, mut desired: BTreeMap<PathBuf, Kind>) {
        self.scripts(&mut desired);
        for (path, kind) in &desired {
            match self.files.get_mut(path) {
                Some(tracked) => tracked.kind = kind.clone(),
                None => {
                    self.files
                        .insert(path.clone(), Tracked::new(kind.clone(), digest_of(path)));
                }
            }
        }
        let derived = self.expand(&desired);
        let dirty = &self.dirty;
        self.files.retain(|path, _| {
            desired.contains_key(path) || derived.contains(path) || dirty.contains(path)
        });
        for path in &derived {
            if !self.files.contains_key(path) {
                self.files
                    .insert(path.clone(), Tracked::new(Kind::Asset, digest_of(path)));
            }
        }
        self.derived = derived;
        self.outside();
        self.publish(Change::default());
    }

    fn scripts(&self, desired: &mut BTreeMap<PathBuf, Kind>) {
        let Ok(entries) = fs::read_dir(self.project.join(Folder::Script.name())) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_script(&path) {
                desired.insert(path, Kind::Script);
            }
        }
    }

    fn expand(&mut self, desired: &BTreeMap<PathBuf, Kind>) -> BTreeSet<PathBuf> {
        let mut derived = BTreeSet::new();
        let mut errors = Vec::new();
        for (path, kind) in desired {
            let Kind::Model(id) = kind else {
                continue;
            };
            let digest = self.files.get(path).and_then(|tracked| tracked.digest);
            let stale = self
                .files
                .get(path)
                .and_then(|tracked| tracked.dependencies.as_ref())
                .is_none_or(|held| held.digest != digest || held.kind != *kind);
            if stale {
                match Model::sources(path) {
                    Ok(sources) => {
                        let mut paths = Vec::new();
                        for dependency in sources.buffers.iter().chain(&sources.images) {
                            let inside = dependency
                                .strip_prefix(&self.project)
                                .is_ok_and(|relative| Folder::Asset.holds(relative));
                            if inside {
                                paths.push(dependency.clone());
                            } else {
                                errors.push(format!(
                                    "the model {id} reads {} outside the assets folder",
                                    dependency.display(),
                                ));
                            }
                        }
                        if let Some(tracked) = self.files.get_mut(path) {
                            tracked.dependencies = Some(Dependencies {
                                kind: kind.clone(),
                                digest,
                                paths,
                            });
                        }
                    }
                    Err(error) => errors.push(error),
                }
            }
            if let Some(tracked) = self.files.get(path)
                && let Some(held) = &tracked.dependencies
            {
                derived.extend(held.paths.iter().cloned());
            }
        }
        self.dependency_errors = errors;
        derived
    }

    fn outside(&mut self) {
        let Some(watcher) = &mut self.watcher else {
            return;
        };
        for path in self.files.keys() {
            let Some(parent) = path.parent() else {
                continue;
            };
            if parent.starts_with(&self.project) || !self.watched.insert(parent.to_path_buf()) {
                continue;
            }
            if let Err(error) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                let message = format!("the file watcher of {} watches: {error}", parent.display(),);
                if !self.watch_errors.contains(&message) {
                    self.watch_errors.push(message);
                }
            }
        }
    }

    fn adopt(&mut self, path: &Path) {
        self.dirty.remove(path);
        if let Some(tracked) = self.files.get_mut(path) {
            tracked.digest = digest_of(path);
        }
    }

    fn observe(&mut self, event: notify::Result<Event>) {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                let message = format!(
                    "the file watcher of {} reports: {error}",
                    self.project.display(),
                );
                if !self.watch_errors.contains(&message) {
                    self.watch_errors.push(message);
                }
                self.dirty.extend(self.files.keys().cloned());
                self.settle = Some(Instant::now() + QUIET);
                return;
            }
        };
        for path in event.paths {
            if is_script(&path) && !self.files.contains_key(&path) {
                self.files
                    .insert(path.clone(), Tracked::new(Kind::Script, None));
            }
            if self.files.contains_key(&path) {
                self.dirty.insert(path);
            }
        }
        let missing = self
            .files
            .iter()
            .filter(|(_, tracked)| tracked.digest.is_none())
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        self.dirty.extend(missing);
        if !self.dirty.is_empty() {
            self.settle = Some(Instant::now() + QUIET);
        }
    }

    fn settle_now(&mut self) {
        let mut change = Change::default();
        for path in std::mem::take(&mut self.dirty) {
            let digest = digest_of(&path);
            let Some(tracked) = self.files.get_mut(&path) else {
                continue;
            };
            if tracked.digest == digest {
                continue;
            }
            tracked.digest = digest;
            change.absorb(tracked.kind.change());
        }
        self.publish(change);
    }

    fn publish(&mut self, change: Change) {
        let errors = self
            .watch_errors
            .iter()
            .chain(&self.dependency_errors)
            .cloned()
            .collect::<Vec<_>>();
        let fresh = if errors == self.sent {
            None
        } else {
            self.sent = errors.clone();
            Some(errors)
        };
        if !change.any() && fresh.is_none() {
            return;
        }
        let _ = self.reports.send(Report {
            change,
            errors: fresh,
        });
        (self.wake)();
    }
}

fn is_script(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|kind| kind == EXTENSION)
}

fn digest_of(path: &Path) -> Option<Digest> {
    fs::read(path).ok().map(|bytes| Digest::of(&bytes))
}

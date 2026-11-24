mod worker;

use dome_scene::Scene;
use notify::Event;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub scene: bool,
    pub model: bool,
    pub asset: bool,
    pub script: bool,
}

impl Change {
    pub fn any(self) -> bool {
        self.scene || self.model || self.asset || self.script
    }

    pub fn absorb(&mut self, other: Self) {
        self.scene |= other.scene;
        self.model |= other.model;
        self.asset |= other.asset;
        self.script |= other.script;
    }
}

#[derive(Clone, Debug)]
pub struct Report {
    pub change: Change,
    pub errors: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Scene,
    Model(String),
    Asset,
    Script,
}

impl Kind {
    fn change(&self) -> Change {
        let mut change = Change::default();
        match self {
            Self::Scene => change.scene = true,
            Self::Model(_) => change.model = true,
            Self::Asset => change.asset = true,
            Self::Script => change.script = true,
        }
        change
    }
}

enum Message {
    Set(BTreeMap<PathBuf, Kind>),
    Adopt(PathBuf),
    Event(notify::Result<Event>),
    Flush(Sender<()>),
    Stop,
}

pub struct Watch {
    path: PathBuf,
    commands: Option<Sender<Message>>,
    reports: Receiver<Report>,
    worker: Option<JoinHandle<()>>,
}

impl Watch {
    pub fn new(scene: &Path, root: &Scene, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (commands, requests) = channel();
        let (reports, received) = channel();
        let worker = worker::spawn(
            requests,
            commands.clone(),
            reports,
            Arc::new(wake),
            root.root().to_path_buf(),
        );
        let mut watch = Self {
            path: scene.to_path_buf(),
            commands: Some(commands),
            reports: received,
            worker: Some(worker),
        };
        watch.renew(root);
        watch.flush();
        watch
    }

    pub fn renew(&mut self, root: &Scene) {
        let mut desired = BTreeMap::new();
        desired.insert(self.path.clone(), Kind::Scene);
        for owners in root.instances().values() {
            for prefab in owners {
                desired.insert(root.prefab(prefab), Kind::Scene);
            }
        }
        for image in root.images() {
            if let Some(path) = image.source.path() {
                desired.insert(root.asset(path), Kind::Asset);
            }
        }
        for sound in root.sounds() {
            desired.insert(root.asset(&sound.path), Kind::Asset);
        }
        for environment in root.environments() {
            if let Some(path) = environment.panorama() {
                desired.insert(root.asset(path), Kind::Asset);
            }
        }
        for model in root.models() {
            desired.insert(root.asset(&model.path), Kind::Model(model.id.clone()));
        }
        self.send(Message::Set(desired));
    }

    pub fn adopt(&mut self, path: &Path) {
        self.send(Message::Adopt(path.to_path_buf()));
    }

    pub fn changed(&mut self) -> Option<Report> {
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            let worker = self
                .worker
                .take()
                .expect("a finished watcher holds its thread");
            match worker.join() {
                Ok(()) => panic!("the file watcher thread stops before its watch stops"),
                Err(payload) => std::panic::resume_unwind(payload),
            }
        }
        let mut report: Option<Report> = None;
        while let Ok(held) = self.reports.try_recv() {
            let merged = report.get_or_insert(Report {
                change: Change::default(),
                errors: None,
            });
            merged.change.absorb(held.change);
            if held.errors.is_some() {
                merged.errors = held.errors;
            }
        }
        report
    }

    fn send(&self, message: Message) {
        if let Some(commands) = &self.commands {
            let _ = commands.send(message);
        }
    }

    fn flush(&self) {
        let (flushed, ready) = channel();
        self.send(Message::Flush(flushed));
        let _ = ready.recv();
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.send(Message::Stop);
        self.commands = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Report, Watch};
    use dome_scene::Scene;
    use dome_schema::Registry;
    use std::fs;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    const SCENE: &str = r#"format = "dome_scene"
images = []
meshes = []
materials = []
nodes = []
"#;
    const SCRIPT: &str = "fn update() {\n}\n";

    #[test]
    fn a_watch_reports_the_script_it_witnesses() {
        let root = std::env::temp_dir().join(format!("dome_watch_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("scripts")).expect("a watch test opens its scripts");
        let path = root.join("scene.toml");
        fs::write(&path, SCENE).expect("a watch test writes its scene");
        let script = root.join("scripts/main.lampas");
        fs::write(&script, SCRIPT).expect("a watch test writes its script");
        let scene = Scene::load(&path, root.clone(), Arc::new(Registry::default()));
        let mut watch = Watch::new(&path, &scene, || {});
        assert!(
            watch.changed().is_none(),
            "a watch reports nothing before it witnesses a write",
        );
        fs::write(&script, format!("{SCRIPT}fn fixed() {{\n}}\n"))
            .expect("a watch test rewrites its script");
        let report = settled(&mut watch).expect("a watch reports the script it witnesses");
        assert!(
            report.change.script,
            "a watch reports the script it witnessed: {:?}",
            report.change,
        );
        assert!(
            !report.change.scene && !report.change.model && !report.change.asset,
            "a watch reports only the script it witnessed: {:?}",
            report.change,
        );
        let _ = fs::remove_dir_all(&root);
    }

    fn settled(watch: &mut Watch) -> Option<Report> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Some(report) = watch.changed() {
                return Some(report);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }
}

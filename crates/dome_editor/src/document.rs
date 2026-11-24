use dome_asset::Digest;
use dome_scene::Scene;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum State {
    Clean,
    Edited,
    Conflicted,
}

pub(crate) struct Document {
    path: PathBuf,
    base: Digest,
    state: State,
}

impl Document {
    pub(crate) fn open(path: PathBuf) -> Self {
        let base = digest(&path);
        Self {
            path,
            base,
            state: State::Clean,
        }
    }

    pub(crate) fn state(&self) -> State {
        self.state
    }

    pub(crate) fn dirty(&self) -> bool {
        self.state != State::Clean
    }

    pub(crate) fn conflicted(&self) -> bool {
        self.state == State::Conflicted
    }

    pub(crate) fn edited(&mut self) {
        if self.state == State::Clean {
            self.state = State::Edited;
        }
    }

    pub(crate) fn adopted(&mut self) {
        self.base = digest(&self.path);
        self.state = State::Clean;
    }

    pub(crate) fn foreign(&self) -> bool {
        digest(&self.path) != self.base
    }

    pub(crate) fn mark_foreign(&mut self) {
        if self.state == State::Edited {
            self.state = State::Conflicted;
        }
    }

    pub(crate) fn write(&mut self, scene: &Scene) -> Result<(), String> {
        if self.foreign() {
            self.state = State::Conflicted;
            return Err(format!(
                "{} changed on disk: reload it or save over it",
                self.path.display(),
            ));
        }
        self.write_over(scene)
    }

    pub(crate) fn write_over(&mut self, scene: &Scene) -> Result<(), String> {
        scene.save(&self.path)?;
        self.adopted();
        Ok(())
    }
}

fn digest(path: &Path) -> Digest {
    fs::read(path).map_or_else(|_| Digest::default(), |bytes| Digest::of(&bytes))
}

#[cfg(test)]
mod tests {
    use super::{Document, State};
    use dome_scene::Scene;
    use dome_schema::Registry;
    use std::fs;
    use std::sync::Arc;

    #[test]
    fn a_write_refuses_the_file_that_changed_under_it() {
        let root = std::env::temp_dir().join(format!("dome_document_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a document test opens its folder");
        let path = root.join("scene.toml");
        let scene = Scene::new(&root, Arc::new(Registry::default()));
        scene.save(&path).expect("a document writes its scene");
        let mut document = Document::open(path.clone());
        assert_eq!(document.state(), State::Clean);
        document.edited();
        fs::write(&path, "format = \"dome_scene\"\n").expect("a document test rewrites its file");
        assert!(
            document.write(&scene).is_err(),
            "a write refuses the file another writer changed",
        );
        assert_eq!(document.state(), State::Conflicted);
        document
            .write_over(&scene)
            .expect("a deliberate overwrite writes its scene");
        assert_eq!(document.state(), State::Clean);
        assert!(!document.foreign(), "an overwritten file is its own base");
        let _ = fs::remove_dir_all(&root);
    }
}

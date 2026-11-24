use dome_asset::Digest;
use dome_scene::Scene;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) struct Document {
    path: PathBuf,
    base: Digest,
    saved: Scene,
    readable: bool,
    conflicted: bool,
}

impl Document {
    pub(crate) fn open(path: PathBuf, scene: &Scene) -> Self {
        Self {
            base: digest(&path),
            path,
            saved: scene.clone(),
            readable: true,
            conflicted: false,
        }
    }

    pub(crate) fn rejected(path: PathBuf, scene: &Scene) -> Self {
        Self {
            readable: false,
            ..Self::open(path, scene)
        }
    }

    pub(crate) fn dirty(&self, scene: &Scene) -> bool {
        scene != &self.saved
    }

    pub(crate) fn conflicted(&self) -> bool {
        self.conflicted
    }

    pub(crate) fn foreign(&self) -> bool {
        digest(&self.path) != self.base
    }

    pub(crate) fn mark_foreign(&mut self) {
        self.conflicted = true;
    }

    pub(crate) fn adopted(&mut self, scene: &Scene) {
        self.base = digest(&self.path);
        self.saved = scene.clone();
        self.readable = true;
        self.conflicted = false;
    }

    pub(crate) fn write(&mut self, scene: &Scene) -> Result<(), String> {
        if !self.readable {
            return Err(format!(
                "{} did not load: reload it before saving",
                self.path.display(),
            ));
        }
        if self.foreign() {
            self.conflicted = true;
            return Err(format!(
                "{} changed on disk: reload it or save over it",
                self.path.display(),
            ));
        }
        self.write_over(scene)
    }

    pub(crate) fn write_over(&mut self, scene: &Scene) -> Result<(), String> {
        scene.save(&self.path)?;
        self.adopted(scene);
        Ok(())
    }
}

fn digest(path: &Path) -> Digest {
    fs::read(path).map_or_else(|_| Digest::default(), |bytes| Digest::of(&bytes))
}

#[cfg(test)]
mod tests {
    use super::Document;
    use dome_scene::{Scene, Value};
    use dome_schema::Registry;
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn folder() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "dome_document_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a document test opens its folder");
        root
    }

    #[test]
    fn a_write_refuses_the_file_that_changed_under_it() {
        let root = folder();
        let path = root.join("scene.toml");
        let scene = Scene::new(&root, Arc::new(Registry::default()));
        scene.save(&path).expect("a document writes its scene");
        let mut document = Document::open(path.clone(), &scene);
        assert!(!document.dirty(&scene), "a fresh document reads its scene");
        fs::write(&path, "format = \"dome_scene\"\n").expect("a document test rewrites its file");
        assert!(
            document.write(&scene).is_err(),
            "a write refuses the file another writer changed",
        );
        assert!(document.conflicted(), "a refused write marks a conflict");
        document
            .write_over(&scene)
            .expect("a deliberate overwrite writes its scene");
        assert!(
            !document.conflicted(),
            "an overwritten file is its own base"
        );
        assert!(!document.foreign(), "an overwritten file is its own base");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_scene_that_returns_to_its_saved_state_is_clean() {
        let root = folder();
        let path = root.join("scene.toml");
        let mut scene = Scene::parse(
            "format = \"dome_scene\"\nimages = []\nmeshes = []\nmaterials = []\nnodes = [{ id = \"node\" }, { id = \"other\" }]\n",
            &root,
            "the saved scene",
            Arc::new(Registry::default()),
        )
        .expect("a document test parses its scene");
        scene.save(&path).expect("a document writes its scene");
        let document = Document::open(path, &scene);
        let target = dome_scene::Target::Node(String::from("node"));
        let (_, delta) = scene
            .edit(|edit| edit.set_property(&target, "hidden", &Value::Bool(true)))
            .expect("a document test edits its scene");
        assert!(document.dirty(&scene), "an edited scene is dirty");
        delta.revert(&mut scene);
        assert!(
            !document.dirty(&scene),
            "a scene undone to its saved state is clean",
        );
        let _ = fs::remove_dir_all(&root);
    }
}

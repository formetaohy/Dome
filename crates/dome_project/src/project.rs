use crate::manifest::Manifest;
use crate::starter;
use dome_scene::{Folder, Scene};
use dome_schema::Registry;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const MANIFEST: &str = "project.toml";
pub const ENTRY: &str = "main.scene.toml";

pub struct Project {
    root: PathBuf,
    manifest: Manifest,
}

impl Project {
    pub fn create(root: impl AsRef<Path>, name: &str) -> Self {
        let name = name.trim();
        assert!(!name.is_empty(), "a project holds a name");
        let root = absolute(root.as_ref());
        assert!(
            vacant(&root),
            "the project folder {} holds files",
            root.display(),
        );
        fs::create_dir_all(&root)
            .unwrap_or_else(|error| panic!("the project folder {} opens: {error}", root.display()));
        for folder in Folder::ALL {
            fs::create_dir_all(root.join(folder.name())).unwrap_or_else(|error| {
                panic!(
                    "the project folder {} opens: {error}",
                    root.join(folder.name()).display(),
                )
            });
        }
        let manifest = Manifest {
            name: String::from(name),
        };
        write(&root.join(MANIFEST), &manifest.write());
        starter::write(&root);
        Self { root, manifest }
    }

    pub fn open(root: impl AsRef<Path>) -> Self {
        let root = absolute(root.as_ref());
        let path = root.join(MANIFEST);
        let text = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("the project {} reads no manifest: {error}", root.display())
        });
        let manifest = Manifest::parse(&text, &format!("the project manifest {}", path.display()));
        let entry = root.join(Folder::Scene.name()).join(ENTRY);
        assert!(
            entry.is_file(),
            "the project {} holds no entry scene {}",
            root.display(),
            entry.display(),
        );
        Scene::load(&entry, &root, Arc::new(Registry::default()));
        Self { root, manifest }
    }

    pub fn holds(root: impl AsRef<Path>) -> bool {
        root.as_ref().join(MANIFEST).is_file()
    }

    pub fn vacant(root: impl AsRef<Path>) -> bool {
        vacant(root.as_ref())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn entry(&self) -> PathBuf {
        self.root.join(Folder::Scene.name()).join(ENTRY)
    }

    pub fn folder(&self, folder: Folder) -> PathBuf {
        self.root.join(folder.name())
    }
}

fn vacant(root: &Path) -> bool {
    if !root.exists() {
        return true;
    }
    root.is_dir()
        && fs::read_dir(root)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
}

fn write(path: &Path, text: &str) {
    fs::write(path, text).unwrap_or_else(|error| {
        panic!("the project writes {}: {error}", path.display());
    });
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|error| {
        panic!("the project root {} resolves: {error}", path.display());
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dome_scene::Scene;
    use dome_schema::Registry;
    use std::sync::Arc;

    const OUTSIDE: &str = r#"format = "dome_scene"
images = [{ id = "outside", path = "../outside.png", min_filter = "linear", mag_filter = "linear", mip_filter = "linear", wrap_u = "repeat", wrap_v = "repeat" }]
meshes = []
materials = []
nodes = []
"#;

    #[test]
    fn a_created_project_opens_the_entry_scene_it_scaffolds() {
        let root = std::env::temp_dir().join(format!("dome_project_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let project = Project::create(&root, "Test Game");
        assert_eq!(project.name(), "Test Game");
        for folder in Folder::ALL {
            assert!(
                project.folder(folder).is_dir(),
                "a project holds its {} folder",
                folder.name(),
            );
        }
        let opened = Project::open(&root);
        let scene = Scene::load(opened.entry(), opened.root(), Arc::new(Registry::default()));
        assert!(
            !scene.nodes().is_empty(),
            "a scaffolded project holds an entry scene with nodes",
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    #[should_panic(expected = "outside the assets folder")]
    fn a_scene_keeps_the_images_it_names_inside_the_assets_folder() {
        Scene::parse(
            OUTSIDE,
            ".",
            "the outside scene",
            Arc::new(Registry::default()),
        );
    }
}

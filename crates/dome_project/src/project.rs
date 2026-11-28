use crate::manifest::Manifest;
use crate::starter;
use dome_scene::Folder;
use std::fs;
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "project.toml";
pub const ENTRY: &str = "main.scene.toml";

pub struct Project {
    root: PathBuf,
    manifest: Manifest,
}

impl Project {
    pub fn create(root: impl AsRef<Path>, name: &str) -> Result<Self, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(String::from("a project holds a name"));
        }
        let root = absolute(root.as_ref())?;
        if !vacant(&root) {
            return Err(format!("the project folder {} holds files", root.display()));
        }
        fs::create_dir_all(&root)
            .map_err(|error| format!("the project folder {} opens: {error}", root.display()))?;
        for folder in Folder::ALL {
            fs::create_dir_all(root.join(folder.name())).map_err(|error| {
                format!(
                    "the project folder {} opens: {error}",
                    root.join(folder.name()).display(),
                )
            })?;
        }
        let manifest = Manifest {
            name: String::from(name),
        };
        write(&root.join(MANIFEST), &manifest.write())?;
        starter::write(&root)?;
        Ok(Self { root, manifest })
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = absolute(root.as_ref())?;
        let path = root.join(MANIFEST);
        let text = fs::read_to_string(&path).map_err(|error| {
            format!("the project {} reads no manifest: {error}", root.display())
        })?;
        let manifest = Manifest::parse(&text, &format!("the project manifest {}", path.display()))?;
        let entry = root.join(Folder::Scene.name()).join(ENTRY);
        if !entry.is_file() {
            return Err(format!(
                "the project {} holds no entry scene {}",
                root.display(),
                entry.display(),
            ));
        }
        Ok(Self { root, manifest })
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

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|error| format!("the project writes {}: {error}", path.display()))
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    std::path::absolute(path)
        .map_err(|error| format!("the project root {} resolves: {error}", path.display()))
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
        let project = Project::create(&root, "Test Game").expect("the project is created");
        assert_eq!(project.name(), "Test Game");
        for folder in Folder::ALL {
            assert!(
                project.folder(folder).is_dir(),
                "a project holds its {} folder",
                folder.name(),
            );
        }
        let opened = Project::open(&root).expect("the project opens");
        let scene = Scene::load(opened.entry(), opened.root(), Arc::new(Registry::default()))
            .expect("the entry scene parses");
        assert!(
            !scene.nodes().is_empty(),
            "a scaffolded project holds an entry scene with nodes",
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_scene_keeps_the_images_it_names_inside_the_assets_folder() {
        let error = Scene::parse(
            OUTSIDE,
            ".",
            "the outside scene",
            Arc::new(Registry::default()),
        )
        .expect_err("the outside scene parses no image outside the assets folder");
        assert_eq!(
            error,
            "the outside scene holds the image outside of path ../outside.png outside the assets folder",
        );
    }
}

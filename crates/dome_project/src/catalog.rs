use crate::project::Project;
use dome_toml::{Fields, Toml};
use std::fs;
use std::path::{Path, PathBuf};

const FORMAT: &str = "dome_launcher";
const LIMIT: usize = 8;

pub struct Recent {
    name: String,
    path: PathBuf,
}

impl Recent {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub struct Catalog {
    file: PathBuf,
    entries: Vec<Recent>,
}

impl Catalog {
    pub fn load() -> Self {
        Self::at(home().join("launcher.toml"))
    }

    pub fn at(file: PathBuf) -> Self {
        if !file.is_file() {
            return Self {
                file,
                entries: Vec::new(),
            };
        }
        let text = fs::read_to_string(&file).unwrap_or_else(|error| {
            panic!(
                "the launcher catalog {} reads no text: {error}",
                file.display()
            )
        });
        let what = format!("the launcher catalog {}", file.display());
        let mut fields = Fields::of(Toml::parse(&text, &what), &what);
        let format = fields
            .need("format")
            .string_value(&format!("the format of {what}"));
        assert!(format == FORMAT, "{what} holds the format {format}");
        let entries = fields
            .take("project")
            .map(|value| value.array_items(&format!("the projects of {what}")))
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(index, item)| recent(item, &format!("project {index} of {what}")))
            .collect();
        fields.done();
        Self { file, entries }
    }

    pub fn entries(&self) -> &[Recent] {
        &self.entries
    }

    pub fn record(&mut self, project: &Project) {
        self.entries.retain(|held| held.path != project.root());
        self.entries.insert(
            0,
            Recent {
                name: String::from(project.name()),
                path: project.root().to_path_buf(),
            },
        );
        self.entries.truncate(LIMIT);
        self.save();
    }

    pub fn forget(&mut self, index: usize) {
        assert!(
            index < self.entries.len(),
            "the launcher forgets a project it holds",
        );
        self.entries.remove(index);
        self.save();
    }

    fn save(&self) {
        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| {
                panic!(
                    "the launcher catalog folder {} opens: {error}",
                    parent.display(),
                )
            });
        }
        let projects = self
            .entries
            .iter()
            .map(|held| {
                Toml::object([
                    ("name", Toml::string(held.name.as_str())),
                    (
                        "path",
                        Toml::string(held.path.to_str().unwrap_or_else(|| {
                            panic!(
                                "the launcher catalog path {} holds no text",
                                held.path.display(),
                            )
                        })),
                    ),
                ])
            })
            .collect();
        let document = Toml::object([
            ("format", Toml::string(FORMAT)),
            ("project", Toml::array(projects)),
        ]);
        fs::write(&self.file, document.write()).unwrap_or_else(|error| {
            panic!(
                "the launcher catalog {} writes no text: {error}",
                self.file.display(),
            )
        });
    }
}

fn recent(value: Toml, what: &str) -> Recent {
    let mut fields = Fields::of(value, what);
    let name = fields
        .need("name")
        .string_value(&format!("the name of {what}"));
    let path = fields
        .need("path")
        .string_value(&format!("the path of {what}"));
    fields.done();
    Recent {
        name,
        path: PathBuf::from(path),
    }
}

fn home() -> PathBuf {
    if let Some(held) = std::env::var_os("DOME_HOME") {
        return PathBuf::from(held);
    }
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
    });
    #[cfg(not(any(windows, target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    base.unwrap_or_else(|| panic!("the launcher finds no home folder"))
        .join("Dome")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_catalog_remembers_the_project_it_records() {
        let root = std::env::temp_dir().join(format!("dome_catalog_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let project = Project::create(root.join("Game"), "Game");
        let file = root.join("launcher.toml");
        let mut catalog = Catalog::at(file.clone());
        catalog.record(&project);
        let loaded = Catalog::at(file);
        assert_eq!(loaded.entries().len(), 1);
        assert_eq!(loaded.entries()[0].name(), "Game");
        assert_eq!(loaded.entries()[0].path(), project.root());
        let _ = fs::remove_dir_all(&root);
    }
}

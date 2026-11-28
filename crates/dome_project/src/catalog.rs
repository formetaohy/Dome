use crate::project::Project;
use dome_toml::{Fields, Toml};
use std::fs;
use std::path::{Path, PathBuf};

const FORMAT: &str = "dome_launcher";
const LIMIT: usize = 8;

#[derive(Clone)]
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
    pub fn load() -> Result<Self, String> {
        Self::at(home()?.join("launcher.toml"))
    }

    pub fn at(file: PathBuf) -> Result<Self, String> {
        Self::read(file)
    }

    fn read(file: PathBuf) -> Result<Self, String> {
        if !file.is_file() {
            return Ok(Self {
                file,
                entries: Vec::new(),
            });
        }
        let text = fs::read_to_string(&file).map_err(|error| {
            format!(
                "the launcher catalog {} reads no text: {error}",
                file.display()
            )
        })?;
        let what = format!("the launcher catalog {}", file.display());
        let mut fields = Fields::of(Toml::parse(&text, &what)?, &what)?;
        let format = fields
            .need("format")?
            .string_value(&format!("the format of {what}"))?;
        if format != FORMAT {
            return Err(format!("{what} holds the format {format}"));
        }
        let entries = match fields.take("project") {
            Some(value) => value.array_items(&format!("the projects of {what}"))?,
            None => Vec::new(),
        }
        .into_iter()
        .enumerate()
        .map(|(index, item)| recent(item, &format!("project {index} of {what}")))
        .collect::<Result<Vec<_>, String>>()?;
        fields.done()?;
        Ok(Self { file, entries })
    }

    pub fn entries(&self) -> &[Recent] {
        &self.entries
    }

    pub fn record(&mut self, project: &Project) -> Result<(), String> {
        let mut entries = self.entries.clone();
        entries.retain(|held| held.path != project.root());
        entries.insert(
            0,
            Recent {
                name: String::from(project.name()),
                path: project.root().to_path_buf(),
            },
        );
        entries.truncate(LIMIT);
        save(&self.file, &entries)?;
        self.entries = entries;
        Ok(())
    }

    pub fn forget(&mut self, index: usize) -> Result<(), String> {
        assert!(
            index < self.entries.len(),
            "the launcher forgets a project it holds",
        );
        let mut entries = self.entries.clone();
        entries.remove(index);
        save(&self.file, &entries)?;
        self.entries = entries;
        Ok(())
    }
}

fn save(file: &Path, entries: &[Recent]) -> Result<(), String> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "the launcher catalog folder {} opens: {error}",
                parent.display(),
            )
        })?;
    }
    let mut projects = Vec::with_capacity(entries.len());
    for held in entries {
        let path = held.path.to_str().ok_or_else(|| {
            format!(
                "the launcher catalog path {} holds no text",
                held.path.display(),
            )
        })?;
        projects.push(Toml::object([
            ("name", Toml::string(held.name.as_str())),
            ("path", Toml::string(path)),
        ]));
    }
    let document = Toml::object([
        ("format", Toml::string(FORMAT)),
        ("project", Toml::array(projects)),
    ]);
    let text = document.write();
    let mut staging = file.as_os_str().to_owned();
    staging.push(format!(".{}.tmp", std::process::id()));
    let staging = PathBuf::from(staging);
    let written = fs::File::create(&staging)
        .and_then(|mut held| {
            std::io::Write::write_all(&mut held, text.as_bytes())?;
            held.sync_all()
        })
        .and_then(|()| fs::rename(&staging, file));
    written.map_err(|error| {
        let _ = fs::remove_file(&staging);
        format!(
            "the launcher catalog {} writes no text: {error}",
            file.display(),
        )
    })
}

fn recent(value: Toml, what: &str) -> Result<Recent, String> {
    let mut fields = Fields::of(value, what)?;
    let name = fields
        .need("name")?
        .string_value(&format!("the name of {what}"))?;
    let path = fields
        .need("path")?
        .string_value(&format!("the path of {what}"))?;
    fields.done()?;
    Ok(Recent {
        name,
        path: PathBuf::from(path),
    })
}

fn home() -> Result<PathBuf, String> {
    if let Some(held) = std::env::var_os("DOME_HOME") {
        return Ok(PathBuf::from(held));
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
    Ok(base
        .ok_or_else(|| String::from("the launcher finds no home folder"))?
        .join("Dome"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn a_catalog_remembers_the_project_it_records() {
        let root = std::env::temp_dir().join(format!("dome_catalog_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let project = Project::create(root.join("Game"), "Game").expect("the project is created");
        let file = root.join("launcher.toml");
        let mut catalog = Catalog::at(file.clone()).expect("the catalog opens");
        catalog.record(&project).expect("the catalog records");
        catalog
            .record(&project)
            .expect("the catalog replaces the file it holds");
        let loaded = Catalog::at(file).expect("the catalog reloads");
        assert_eq!(loaded.entries().len(), 1);
        assert_eq!(loaded.entries()[0].name(), "Game");
        assert_eq!(loaded.entries()[0].path(), project.root());
        assert_eq!(
            names(&root),
            vec![OsString::from("Game"), OsString::from("launcher.toml")],
            "a recorded catalog leaves its file in place and no staging file",
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_catalog_that_cannot_replace_its_file_reports_and_leaves_no_staging_one() {
        let root = std::env::temp_dir().join(format!("dome_catalog_stuck_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let file = root.join("launcher.toml");
        fs::create_dir_all(&file).expect("the test holds a catalog target folder");
        fs::write(file.join("held.toml"), "").expect("the test fills the catalog target folder");
        let project = Project::create(root.join("Game"), "Game").expect("the project is created");
        let mut catalog = Catalog::at(file).expect("the catalog opens");
        assert!(
            catalog.record(&project).is_err(),
            "a catalog whose file cannot be replaced reports the failure",
        );
        assert_eq!(
            names(&root),
            vec![OsString::from("Game"), OsString::from("launcher.toml")],
            "a failed catalog write leaves no staging file",
        );
        let _ = fs::remove_dir_all(&root);
    }

    fn names(root: &Path) -> Vec<OsString> {
        let mut held = fs::read_dir(root)
            .expect("the catalog folder reads")
            .map(|entry| {
                entry
                    .expect("the catalog folder holds its entries")
                    .file_name()
            })
            .collect::<Vec<_>>();
        held.sort();
        held
    }
}

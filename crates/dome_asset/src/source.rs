use crate::digest::Digest;
use dome_gltf::{Model, ModelSources};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone, Default)]
pub struct Dependencies {
    attempted: Option<Digest>,
    sources: ModelSources,
    error: Option<String>,
}

#[derive(Clone)]
struct Held {
    stamp: Option<Stamp>,
    digest: Result<Digest, String>,
    dependencies: Dependencies,
}

#[derive(Clone, Default)]
pub struct Sources {
    files: BTreeMap<PathBuf, Held>,
}

impl Sources {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn digest(&mut self, path: &Path) -> Result<Digest, String> {
        self.current(path).digest.clone()
    }

    pub fn read(&mut self, path: &Path) -> Result<(Digest, Vec<u8>), String> {
        let stamp = stamp_of(path);
        let bytes = fs::read(path).map_err(|error| {
            format!(
                "the asset source {} reads no bytes: {error}",
                path.display()
            )
        })?;
        let digest = Digest::of(&bytes);
        let dependencies = self.dependencies_of(path);
        self.files.insert(
            path.to_path_buf(),
            Held {
                stamp,
                digest: Ok(digest),
                dependencies,
            },
        );
        Ok((digest, bytes))
    }

    pub fn dependencies(&mut self, path: &Path) -> Dependencies {
        let digest = match self.digest(path) {
            Ok(digest) => digest,
            Err(error) => {
                let held = self
                    .files
                    .get_mut(path)
                    .expect("a digested source holds its file");
                held.dependencies.error = Some(error);
                return held.dependencies.clone();
            }
        };
        let held = self
            .files
            .get_mut(path)
            .expect("a digested source holds its file");
        if held.dependencies.attempted == Some(digest) {
            return held.dependencies.clone();
        }
        match Model::sources(path) {
            Ok(sources) => {
                held.dependencies = Dependencies {
                    attempted: Some(digest),
                    sources,
                    error: None,
                }
            }
            Err(error) => {
                held.dependencies.attempted = Some(digest);
                held.dependencies.error = Some(error);
            }
        }
        held.dependencies.clone()
    }

    pub fn retain(&mut self, used: &BTreeSet<PathBuf>) {
        self.files.retain(|path, _| used.contains(path));
    }

    fn current(&mut self, path: &Path) -> &Held {
        let stamp = stamp_of(path);
        let fresh = self.files.get(path).is_some_and(|held| held.stamp == stamp);
        if !fresh {
            let dependencies = self.dependencies_of(path);
            self.files.insert(
                path.to_path_buf(),
                Held {
                    stamp,
                    digest: read_digest(path),
                    dependencies,
                },
            );
        }
        self.files
            .get(path)
            .expect("a refreshed source holds its file")
    }

    fn dependencies_of(&self, path: &Path) -> Dependencies {
        self.files
            .get(path)
            .map(|held| held.dependencies.clone())
            .unwrap_or_default()
    }
}

impl Dependencies {
    pub fn sources(&self) -> &ModelSources {
        &self.sources
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

fn read_digest(path: &Path) -> Result<Digest, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "the asset source {} reads no bytes: {error}",
            path.display()
        )
    })?;
    Ok(Digest::of(&bytes))
}

fn stamp_of(path: &Path) -> Option<Stamp> {
    let metadata = fs::metadata(path).ok()?;
    Some(Stamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

#[cfg(test)]
mod tests {
    use super::Sources;
    use std::collections::BTreeSet;
    use std::fs;

    const MODEL: &[u8] =
        br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"mesh.bin","byteLength":4}]}"#;
    const REPAIRED: &[u8] = br#"{"asset":{"version":"2.0"}}"#;

    #[test]
    fn a_broken_model_keeps_the_dependencies_it_discovered() {
        let root = std::env::temp_dir().join(format!("dome_scene_source_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a source test opens its directory");
        let model = root.join("mesh.gltf");
        let buffer = root.join("mesh.bin");
        fs::write(&buffer, b"four").expect("a source test writes its buffer");
        fs::write(&model, MODEL).expect("a source test writes its model");

        let mut sources = Sources::new();
        let (digest, bytes) = sources.read(&model).expect("a source test reads its model");
        assert_eq!(bytes, MODEL);
        assert_eq!(
            digest,
            sources
                .digest(&model)
                .expect("a source test hashes its model"),
        );
        let held = sources.dependencies(&model);
        assert_eq!(
            held.sources().buffers.as_slice(),
            [buffer.as_path()].as_slice()
        );
        assert!(held.error().is_none());

        fs::write(&model, b"half a model").expect("a source test breaks its model");
        assert_ne!(
            digest,
            sources
                .digest(&model)
                .expect("a source test hashes its model"),
        );
        let held = sources.dependencies(&model);
        assert_eq!(
            held.sources().buffers.as_slice(),
            [buffer.as_path()].as_slice(),
            "a model that breaks keeps the dependencies it discovered",
        );
        assert!(
            held.error()
                .is_some_and(|error| error.contains("glTF text")),
            "a model that breaks reports the reason it holds no dependency",
        );

        fs::write(&model, REPAIRED).expect("a source test repairs its model");
        let held = sources.dependencies(&model);
        assert!(held.sources().buffers.is_empty());
        assert!(held.error().is_none());

        sources.retain(&BTreeSet::from([model.clone()]));
        assert!(
            sources.digest(&model).is_ok(),
            "a retained source still hashes its model",
        );
        let _ = fs::remove_dir_all(&root);
    }
}

use std::path::{Component, Path, PathBuf};

pub fn relative_path(text: &str) -> PathBuf {
    PathBuf::from(text.replace('\\', "/"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Folder {
    Asset,
    Scene,
    Prefab,
    Script,
}

impl Folder {
    pub const ALL: [Self; 4] = [Self::Asset, Self::Scene, Self::Prefab, Self::Script];

    pub fn name(self) -> &'static str {
        match self {
            Self::Asset => "assets",
            Self::Scene => "scenes",
            Self::Prefab => "prefabs",
            Self::Script => "scripts",
        }
    }

    pub fn holds(self, path: &Path) -> bool {
        let mut parts = path.components();
        let named = matches!(
            parts.next(),
            Some(Component::Normal(head)) if head.to_str() == Some(self.name()),
        );
        named && parts.next().is_some() && parts.all(|part| matches!(part, Component::Normal(_)))
    }

    pub fn resolve(self, root: &Path, path: &Path) -> PathBuf {
        assert!(
            self.holds(path),
            "the path {} leaves the {} folder",
            path.display(),
            self.name(),
        );
        root.join(path)
    }
}

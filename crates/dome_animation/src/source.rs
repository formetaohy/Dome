use crate::Clip;
use dome_ecs::Entity;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ClipSource {
    clip: Arc<Clip>,
    name: String,
    targets: Arc<[Entity]>,
}

impl ClipSource {
    pub fn new(name: impl Into<String>, clip: Arc<Clip>, targets: Arc<[Entity]>) -> Self {
        let name = name.into();
        assert!(!name.is_empty(), "an animation source names no clip");
        assert!(!clip.is_empty(), "the animation {name} holds no track");
        assert_eq!(
            targets.len(),
            clip.tracks().len(),
            "the animation {name} names {} targets for {} tracks",
            targets.len(),
            clip.tracks().len(),
        );
        Self {
            clip,
            name,
            targets,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn clip(&self) -> &Clip {
        &self.clip
    }

    pub fn targets(&self) -> &[Entity] {
        &self.targets
    }
}

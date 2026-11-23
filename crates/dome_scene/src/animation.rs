use crate::reference::{ModelId, NodeId};

#[derive(Clone, Debug, PartialEq)]
pub struct AnimationAsset {
    model: ModelId,
    animation: u32,
    targets: Vec<NodeId>,
}

impl AnimationAsset {
    pub fn model(model: ModelId, animation: u32, targets: Vec<NodeId>) -> Self {
        assert!(
            !targets.is_empty(),
            "the animation asset {model} {animation} targets no node",
        );
        Self {
            model,
            animation,
            targets,
        }
    }

    pub fn model_of(&self) -> &ModelId {
        &self.model
    }

    pub fn index(&self) -> u32 {
        self.animation
    }

    pub fn targets(&self) -> &[NodeId] {
        &self.targets
    }

    pub(crate) fn rebased(&self, prefix: &str) -> Self {
        Self {
            model: self.model.clone(),
            animation: self.animation,
            targets: self
                .targets
                .iter()
                .map(|target| NodeId::new(format!("{prefix}{target}")))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnimationEntry {
    pub id: String,
    pub asset: AnimationAsset,
}

impl AnimationEntry {
    pub fn new(id: impl Into<String>, asset: AnimationAsset) -> Self {
        Self {
            id: id.into(),
            asset,
        }
    }
}

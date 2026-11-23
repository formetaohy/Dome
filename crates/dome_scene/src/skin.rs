use crate::reference::{ModelId, NodeId};
use dome_math::Mat4;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct SkinAsset {
    model: ModelId,
    skin: u32,
    joints: Vec<NodeId>,
}

impl SkinAsset {
    pub fn model(model: ModelId, skin: u32, joints: Vec<NodeId>) -> Self {
        assert!(
            !joints.is_empty(),
            "the skin asset {model} {skin} names no joint",
        );
        Self {
            model,
            skin,
            joints,
        }
    }

    pub fn model_of(&self) -> &ModelId {
        &self.model
    }

    pub fn index(&self) -> u32 {
        self.skin
    }

    pub fn joints(&self) -> &[NodeId] {
        &self.joints
    }

    pub(crate) fn rebased(&self, prefix: &str) -> Self {
        Self {
            model: self.model.clone(),
            skin: self.skin,
            joints: self
                .joints
                .iter()
                .map(|joint| NodeId::new(format!("{prefix}{joint}")))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkinEntry {
    pub id: String,
    pub asset: SkinAsset,
}

impl SkinEntry {
    pub fn new(id: impl Into<String>, asset: SkinAsset) -> Self {
        Self {
            id: id.into(),
            asset,
        }
    }
}

#[derive(Clone)]
pub struct SkinData {
    joints: Vec<NodeId>,
    inverse_binds: Arc<[Mat4]>,
}

impl SkinData {
    pub fn new(joints: Vec<NodeId>, inverse_binds: Arc<[Mat4]>) -> Self {
        assert!(!joints.is_empty(), "a skin of no joint deforms no vertex",);
        assert_eq!(
            joints.len(),
            inverse_binds.len(),
            "a skin holds {} joints for {} inverse bind matrices",
            joints.len(),
            inverse_binds.len(),
        );
        Self {
            joints,
            inverse_binds,
        }
    }

    pub fn joints(&self) -> &[NodeId] {
        &self.joints
    }

    pub fn inverse_binds(&self) -> &[Mat4] {
        &self.inverse_binds
    }
}

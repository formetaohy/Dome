use dome_ecs::Entity;
use dome_math::Mat4;
use std::sync::Arc;

#[derive(Clone)]
pub struct Skin {
    source: String,
    joints: Vec<Entity>,
    inverse_binds: Arc<[Mat4]>,
}

impl Skin {
    pub fn new(source: impl Into<String>, joints: Vec<Entity>, inverse_binds: Arc<[Mat4]>) -> Self {
        let source = source.into();
        assert!(!source.is_empty(), "a skin names no source",);
        assert!(!joints.is_empty(), "a skin of no joint deforms no vertex",);
        assert_eq!(
            joints.len(),
            inverse_binds.len(),
            "a skin holds {} joints for {} inverse bind matrices",
            joints.len(),
            inverse_binds.len(),
        );
        Self {
            source,
            joints,
            inverse_binds,
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn joints(&self) -> &[Entity] {
        &self.joints
    }

    pub fn inverse_binds(&self) -> &[Mat4] {
        &self.inverse_binds
    }
}

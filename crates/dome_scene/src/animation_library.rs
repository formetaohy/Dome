use crate::change::Changed;
use crate::install::NodeMap;
use crate::registry::Assets;
use crate::scene::Scene;
use crate::schema::ReferenceKind;
use dome_animation::ClipSource;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct AnimationLibrary {
    sources: HashMap<String, Arc<ClipSource>>,
}

impl AnimationLibrary {
    pub(crate) fn rebuild(&mut self, scene: &Scene, assets: &dyn Assets, nodes: &NodeMap) {
        self.revise_with(scene, assets, nodes, |_| true);
    }

    pub(crate) fn revise(
        &mut self,
        scene: &Scene,
        assets: &dyn Assets,
        nodes: &NodeMap,
        changed: &Changed,
    ) -> HashSet<String> {
        self.revise_with(scene, assets, nodes, |id| {
            changed.touches(ReferenceKind::Animation, id)
        })
    }

    fn revise_with(
        &mut self,
        scene: &Scene,
        assets: &dyn Assets,
        nodes: &NodeMap,
        stale: impl Fn(&str) -> bool,
    ) -> HashSet<String> {
        let held = scene
            .animations()
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<HashSet<_>>();
        self.sources.retain(|id, _| held.contains(id.as_str()));
        let mut rebuilt = HashSet::new();
        for entry in scene.animations() {
            if self.sources.contains_key(&entry.id) && !stale(&entry.id) {
                continue;
            }
            let clip = assets.animation(entry.id.as_str());
            let targets = entry
                .asset
                .targets()
                .iter()
                .map(|target| nodes.entity(target.as_str()))
                .collect::<Vec<_>>();
            self.sources.insert(
                entry.id.clone(),
                Arc::new(ClipSource::new(entry.id.as_str(), clip, Arc::from(targets))),
            );
            rebuilt.insert(entry.id.clone());
        }
        rebuilt
    }

    pub fn source(&self, id: &str) -> Arc<ClipSource> {
        Arc::clone(self.sources.get(id).unwrap_or_else(|| {
            panic!("the scene plays the animation {id}, which it does not hold")
        }))
    }

    pub fn get(&self, id: &str) -> Option<Arc<ClipSource>> {
        self.sources.get(id).cloned()
    }
}

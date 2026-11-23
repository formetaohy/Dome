use crate::animation_library::AnimationLibrary;
use dome_animation::{AnimationGraph, ConditionDesc, ParameterDesc, StateDesc, TransitionDesc};
use dome_scene::Changed;
use dome_scene::GraphEntry;
use dome_scene::ReferenceKind;
use dome_scene::Scene;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct GraphLibrary {
    graphs: HashMap<String, Arc<AnimationGraph>>,
}

impl GraphLibrary {
    pub(crate) fn rebuild(&mut self, scene: &Scene, animations: &AnimationLibrary) {
        self.revise_with(scene, animations, &HashSet::new(), |_| true);
    }

    pub(crate) fn revise(
        &mut self,
        scene: &Scene,
        animations: &AnimationLibrary,
        rebuilt: &HashSet<String>,
        changed: &Changed,
    ) {
        self.revise_with(scene, animations, rebuilt, |id| {
            changed.touches(ReferenceKind::Graph, id)
        });
    }

    fn revise_with(
        &mut self,
        scene: &Scene,
        animations: &AnimationLibrary,
        rebuilt: &HashSet<String>,
        stale: impl Fn(&str) -> bool,
    ) {
        let held = scene
            .graphs()
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<HashSet<_>>();
        self.graphs.retain(|id, _| held.contains(id.as_str()));
        for entry in scene.graphs() {
            let references = entry
                .asset
                .states()
                .iter()
                .any(|state| rebuilt.contains(state.clip().as_str()));
            if self.graphs.contains_key(&entry.id) && !stale(&entry.id) && !references {
                continue;
            }
            self.graphs
                .insert(entry.id.clone(), Arc::new(graph(entry, animations)));
        }
    }

    pub fn get(&self, id: &str) -> Option<Arc<AnimationGraph>> {
        self.graphs.get(id).cloned()
    }

    pub fn source(&self, id: &str) -> Arc<AnimationGraph> {
        self.get(id).unwrap_or_else(|| {
            panic!("the scene plays the animation graph {id}, which it does not hold")
        })
    }
}

fn graph(entry: &GraphEntry, animations: &AnimationLibrary) -> AnimationGraph {
    let asset = &entry.asset;
    let parameters = asset
        .parameters()
        .iter()
        .map(|held| ParameterDesc {
            name: String::from(held.name()),
            default: held.default(),
        })
        .collect();
    let states = asset
        .states()
        .iter()
        .map(|held| StateDesc {
            id: String::from(held.id()),
            source: animations.source(held.clip().as_str()),
            speed: held.speed(),
            looped: held.looped(),
        })
        .collect();
    let transitions = asset
        .transitions()
        .iter()
        .map(|held| TransitionDesc {
            from: String::from(held.from()),
            to: String::from(held.to()),
            fade: held.fade(),
            exit: held.exit(),
            conditions: held
                .conditions()
                .iter()
                .map(|held| ConditionDesc {
                    parameter: String::from(held.parameter()),
                    comparison: held.comparison(),
                    value: held.value(),
                })
                .collect(),
        })
        .collect();
    let position = asset
        .states()
        .iter()
        .position(|held| held.id() == asset.entry())
        .expect("a validated graph enters a state it holds");
    AnimationGraph::new(entry.id.as_str(), parameters, states, transitions, position)
}

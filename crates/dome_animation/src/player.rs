use crate::ClipSource;
use crate::graph::AnimationGraph;
use crate::layer::AnimationLayer;
use crate::pose::Pose;
use dome_ecs::Entity;
use dome_ecs::World;
use dome_math::Transform;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct AnimationPlayer {
    layers: Vec<AnimationLayer>,
    primary: usize,
    ended: bool,
}

impl AnimationPlayer {
    pub(crate) fn new(source: Arc<ClipSource>, speed: f32, looped: bool, playing: bool) -> Self {
        Self {
            layers: vec![AnimationLayer::new(
                source, 0.0, speed, looped, playing, 1.0,
            )],
            primary: 0,
            ended: false,
        }
    }

    pub(crate) fn layers(&self) -> &[AnimationLayer] {
        &self.layers
    }

    pub(crate) fn time(&self) -> f32 {
        self.primary_layer().time()
    }

    pub(crate) fn set_time(&mut self, time: f32) {
        self.primary_layer_mut().set_time(time);
    }

    pub(crate) fn take_ended(&mut self) -> bool {
        std::mem::take(&mut self.ended)
    }

    pub(crate) fn set_playing(&mut self, playing: bool) {
        for layer in &mut self.layers {
            layer.set_playing(playing);
        }
    }

    pub(crate) fn fade_to(
        &mut self,
        source: Arc<ClipSource>,
        speed: f32,
        looped: bool,
        duration: f32,
    ) {
        assert!(
            duration.is_finite() && duration >= 0.0,
            "an animation fade of {duration} is no finite non-negative span",
        );
        let primary = self.primary;
        self.ended = false;
        if duration == 0.0 {
            self.layers.clear();
            self.layers
                .push(AnimationLayer::new(source, 0.0, speed, looped, true, 1.0));
            self.primary = 0;
            return;
        }
        self.layers[primary].fade_to(0.0, duration);
        self.layers.push({
            let mut layer = AnimationLayer::new(source, 0.0, speed, looped, true, 0.0);
            layer.fade_to(1.0, duration);
            layer
        });
        self.primary = self.layers.len() - 1;
    }

    pub(crate) fn rebase(&mut self, graph: &AnimationGraph) {
        let mut index = 0;
        while index < self.layers.len() {
            let name = self.layers[index].source().name().to_owned();
            match clip_of(graph, &name) {
                Some((source, speed, looped)) => {
                    self.layers[index].rebase(Arc::clone(source), speed, looped);
                    index += 1;
                }
                None => {
                    self.layers.remove(index);
                    if index < self.primary {
                        self.primary -= 1;
                    }
                }
            }
        }
        if !self.layers.is_empty() {
            self.primary = self.primary.min(self.layers.len() - 1);
        }
    }

    pub(crate) fn advance(&mut self, delta: f32) -> bool {
        assert!(delta.is_finite(), "an animation delta of {delta} is finite");
        let mut changed = false;
        for layer in &mut self.layers {
            changed |= layer.advance(delta);
        }
        self.ended |= self
            .layers
            .get(self.primary)
            .is_some_and(AnimationLayer::finished);
        let mut index = 0;
        while index < self.layers.len() {
            if self.layers[index].retiring() {
                self.layers.remove(index);
                changed = true;
                if self.layers.is_empty() {
                    self.primary = 0;
                } else if index < self.primary {
                    self.primary -= 1;
                } else if index == self.primary {
                    self.primary = self.layers.len() - 1;
                }
            } else {
                index += 1;
            }
        }
        changed
    }

    pub(crate) fn pose(&self, world: &World) -> Vec<(Entity, Transform)> {
        let layers = self
            .layers
            .iter()
            .filter(|layer| layer.weight() > 0.0)
            .collect::<Vec<_>>();
        let mut pose = Pose::new(world, layers.iter().map(|layer| layer.source()));
        for layer in layers {
            pose.sample(layer);
        }
        pose.transforms()
    }

    fn primary_layer(&self) -> &AnimationLayer {
        self.layers
            .get(self.primary)
            .unwrap_or_else(|| panic!("the animation holds no layer to play"))
    }

    fn primary_layer_mut(&mut self) -> &mut AnimationLayer {
        let primary = self.primary;
        self.layers
            .get_mut(primary)
            .unwrap_or_else(|| panic!("the animation holds no layer to play"))
    }
}

fn clip_of<'a>(graph: &'a AnimationGraph, name: &str) -> Option<(&'a Arc<ClipSource>, f32, bool)> {
    graph
        .states()
        .iter()
        .find(|state| state.source().name() == name)
        .map(|state| (state.source(), state.speed(), state.looped()))
}

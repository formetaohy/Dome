use crate::AnimationGraph;
use crate::Parameter;
use crate::graph::State;
use crate::player::AnimationPlayer;
use dome_ecs::{Entity, World};
use dome_math::Transform;
use std::sync::Arc;

#[derive(Clone)]
pub struct AnimationMachine {
    graph: Arc<AnimationGraph>,
    values: Vec<Parameter>,
    current: usize,
    player: AnimationPlayer,
    playing: bool,
    fade: f32,
}

impl AnimationMachine {
    pub fn new(graph: Arc<AnimationGraph>, playing: bool, fade: f32) -> Self {
        assert!(
            fade.is_finite() && fade >= 0.0,
            "an animation fade of {fade} is no finite non-negative span",
        );
        let entry = graph.entry();
        let state = &graph.states()[entry];
        let player = AnimationPlayer::new(
            Arc::clone(state.source()),
            state.speed(),
            state.looped(),
            playing,
        );
        Self {
            values: graph.defaults(),
            graph,
            current: entry,
            player,
            playing,
            fade,
        }
    }

    pub fn graph(&self) -> &AnimationGraph {
        &self.graph
    }

    pub fn rebase(&mut self, graph: Arc<AnimationGraph>) {
        let state = String::from(self.state().id());
        let defaults = graph.defaults();
        let mut values = defaults.clone();
        for (index, (name, _)) in graph.parameters().enumerate() {
            if let Some(value) = self.parameter(name)
                && value.shares_kind(defaults[index])
            {
                values[index] = value;
            }
        }
        self.values = values;
        match graph.state(&state) {
            Some(current) => {
                self.player.rebase(&graph);
                if self.player.layers().is_empty() {
                    self.player = self.enterable(&graph, current);
                } else {
                    self.player.set_playing(self.playing);
                }
                self.current = current;
            }
            None => {
                self.current = graph.entry();
                self.player = self.enterable(&graph, self.current);
            }
        }
        self.graph = graph;
    }

    pub fn state(&self) -> &State {
        &self.graph.states()[self.current]
    }

    pub fn time(&self) -> f32 {
        self.player.time()
    }

    pub fn set_time(&mut self, time: f32) {
        self.player.set_time(time);
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        self.player.set_playing(playing);
    }

    pub fn fade(&self) -> f32 {
        self.fade
    }

    pub fn set_fade(&mut self, fade: f32) {
        assert!(
            fade.is_finite() && fade >= 0.0,
            "an animation fade of {fade} is no finite non-negative span",
        );
        self.fade = fade;
    }

    pub fn set_state(&mut self, id: &str) -> Result<(), String> {
        let index = self.graph.state(id).ok_or_else(|| {
            format!(
                "the animation graph {} holds no state {id}",
                self.graph.name()
            )
        })?;
        let fade = self.fade;
        self.enter(index, fade);
        Ok(())
    }

    pub fn parameter(&self, name: &str) -> Option<Parameter> {
        self.graph.parameter(name).map(|index| self.values[index])
    }

    pub fn parameters(&self) -> impl Iterator<Item = (&str, Parameter)> + '_ {
        self.graph
            .parameters()
            .zip(self.values.iter().copied())
            .map(|((name, _), value)| (name, value))
    }

    pub fn set_parameter(&mut self, name: &str, value: Parameter) -> Result<(), String> {
        let index = self.graph.parameter(name).ok_or_else(|| {
            format!(
                "the animation graph {} holds no parameter {name}",
                self.graph.name(),
            )
        })?;
        if !self.values[index].shares_kind(value) {
            return Err(format!(
                "the animation parameter {name} of {} holds a {}, not a {}",
                self.graph.name(),
                self.values[index].name(),
                value.name(),
            ));
        }
        self.values[index] = value;
        Ok(())
    }

    pub fn advance(&mut self, delta: f32) -> bool {
        assert!(delta.is_finite(), "an animation delta of {delta} is finite");
        let mut changed = self.player.advance(delta);
        if !self.playing() || self.player.layers().len() > 1 {
            return changed;
        }
        let reach = self.reach();
        let finished = self.player.finished();
        let mut chosen = None;
        for transition in self.graph.transitions() {
            let exited = transition.exit() == 0.0 || reach >= transition.exit() || finished;
            if transition.from() == self.current && exited && transition.holds(&self.values) {
                chosen = Some((transition.to(), transition.fade()));
                break;
            }
        }
        if let Some((to, fade)) = chosen {
            self.enter(to, fade);
            changed = true;
        }
        changed
    }

    pub fn pose(&self, world: &World) -> Vec<(Entity, Transform)> {
        self.player.pose(world)
    }

    fn reach(&self) -> f32 {
        let duration = self.state().source().clip().duration();
        if duration > 0.0 {
            (self.player.time() / duration).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    fn enter(&mut self, index: usize, fade: f32) {
        let state = &self.graph.states()[index];
        self.player.fade_to(
            Arc::clone(state.source()),
            state.speed(),
            state.looped(),
            fade,
        );
        self.player.set_playing(self.playing);
        self.current = index;
    }

    fn enterable(&self, graph: &AnimationGraph, index: usize) -> AnimationPlayer {
        let state = &graph.states()[index];
        AnimationPlayer::new(
            Arc::clone(state.source()),
            state.speed(),
            state.looped(),
            self.playing,
        )
    }
}

mod property;
mod toml;

pub(crate) use toml::parse;

use crate::reference::AnimationId;
use crate::target::valid_id;
use dome_animation::{Comparison, Parameter};

#[derive(Clone, Debug, PartialEq)]
pub struct GraphEntry {
    pub id: String,
    pub asset: GraphAsset,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphAsset {
    entry: String,
    parameters: Vec<GraphParameter>,
    states: Vec<GraphState>,
    transitions: Vec<GraphTransition>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphParameter {
    name: String,
    default: Parameter,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphState {
    id: String,
    clip: AnimationId,
    speed: f32,
    looped: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphTransition {
    from: String,
    to: String,
    fade: f32,
    exit: f32,
    conditions: Vec<GraphCondition>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphCondition {
    parameter: String,
    comparison: Comparison,
    value: Parameter,
}

impl GraphEntry {
    pub fn new(id: impl Into<String>, asset: GraphAsset) -> Self {
        let id = id.into();
        assert!(
            valid_id(&id),
            "the graph id {id} holds no letter, digit, _ or -"
        );
        Self { id, asset }
    }
}

impl GraphAsset {
    pub fn new(
        entry: impl Into<String>,
        parameters: Vec<GraphParameter>,
        states: Vec<GraphState>,
        transitions: Vec<GraphTransition>,
    ) -> Self {
        assert!(!states.is_empty(), "an animation graph holds no state");
        Self {
            entry: entry.into(),
            parameters,
            states,
            transitions,
        }
    }

    pub fn entry(&self) -> &str {
        &self.entry
    }

    pub fn parameters(&self) -> &[GraphParameter] {
        &self.parameters
    }

    pub fn parameter(&self, name: &str) -> Option<&GraphParameter> {
        self.parameters.iter().find(|held| held.name == name)
    }

    pub fn states(&self) -> &[GraphState] {
        &self.states
    }

    pub fn state(&self, id: &str) -> Option<&GraphState> {
        self.states.iter().find(|held| held.id == id)
    }

    pub(crate) fn rebased(&self, prefix: &str) -> Self {
        let mut held = self.clone();
        for state in &mut held.states {
            state.clip = AnimationId::new(format!("{prefix}{}", state.clip.as_str()));
        }
        held
    }

    pub(crate) fn references_clip(&self, id: &str) -> bool {
        self.states.iter().any(|state| state.clip.as_str() == id)
    }

    fn state_mut(&mut self, id: &str) -> Result<&mut GraphState, String> {
        self.states
            .iter_mut()
            .find(|held| held.id == id)
            .ok_or_else(|| format!("the animation graph holds no state {id}"))
    }

    pub fn transitions(&self) -> &[GraphTransition] {
        &self.transitions
    }

    fn transition_mut(&mut self, index: usize) -> Result<&mut GraphTransition, String> {
        let count = self.transitions.len();
        self.transitions
            .get_mut(index)
            .ok_or_else(|| format!("the animation graph holds {count} transitions, not {index}"))
    }

    pub fn add_parameter(&mut self, name: &str, default: Parameter) -> Result<(), String> {
        if !valid_id(name) {
            return Err(format!(
                "the animation parameter id {name} holds no letter, digit, _ or -"
            ));
        }
        if self.parameter(name).is_some() {
            return Err(format!("the animation graph holds the parameter {name}"));
        }
        self.parameters.push(GraphParameter {
            name: String::from(name),
            default,
        });
        Ok(())
    }

    pub fn remove_parameter(&mut self, name: &str) -> Result<(), String> {
        let index = self
            .parameters
            .iter()
            .position(|held| held.name == name)
            .ok_or_else(|| format!("the animation graph holds no parameter {name}"))?;
        if self
            .transitions
            .iter()
            .any(|held| held.conditions.iter().any(|held| held.parameter == name))
        {
            return Err(format!(
                "the animation graph conditions the parameter {name}"
            ));
        }
        self.parameters.remove(index);
        Ok(())
    }

    pub fn add_state(
        &mut self,
        id: &str,
        clip: &str,
        speed: f32,
        looped: bool,
    ) -> Result<(), String> {
        if !valid_id(id) {
            return Err(format!(
                "the animation state id {id} holds no letter, digit, _ or -"
            ));
        }
        if self.state(id).is_some() {
            return Err(format!("the animation graph holds the state {id}"));
        }
        if !valid_id(clip) {
            return Err(format!(
                "the animation id {clip} holds no letter, digit, _ or -"
            ));
        }
        self.states
            .push(GraphState::new(id, AnimationId::new(clip), speed, looped)?);
        Ok(())
    }

    pub fn remove_state(&mut self, id: &str) -> Result<(), String> {
        let index = self
            .states
            .iter()
            .position(|held| held.id == id)
            .ok_or_else(|| format!("the animation graph holds no state {id}"))?;
        if self.states.len() == 1 {
            return Err(String::from("an animation graph keeps a state to play"));
        }
        if self.entry == id {
            return Err(format!("the animation graph enters {id}"));
        }
        if self
            .transitions
            .iter()
            .any(|held| held.from == id || held.to == id)
        {
            return Err(format!("the animation graph transitions through {id}"));
        }
        self.states.remove(index);
        Ok(())
    }

    pub fn add_transition(
        &mut self,
        from: &str,
        to: &str,
        fade: f32,
        exit: f32,
    ) -> Result<(), String> {
        if self.state(from).is_none() {
            return Err(format!("the animation graph holds no state {from}"));
        }
        if self.state(to).is_none() {
            return Err(format!("the animation graph holds no state {to}"));
        }
        let transition = GraphTransition::new(from, to, fade, exit, Vec::new())?;
        self.transitions.push(transition);
        Ok(())
    }

    pub fn remove_transition(&mut self, index: usize) -> Result<(), String> {
        self.transition_mut(index)?;
        self.transitions.remove(index);
        Ok(())
    }

    pub fn add_condition(
        &mut self,
        index: usize,
        parameter: &str,
        comparison: Comparison,
        value: Parameter,
    ) -> Result<(), String> {
        let default = self
            .parameter(parameter)
            .ok_or_else(|| format!("the animation graph holds no parameter {parameter}"))?
            .default;
        if !default.shares_kind(value) {
            return Err(format!(
                "the animation condition of {parameter} compares a {} with a {}",
                default.name(),
                value.name(),
            ));
        }
        if matches!(default, Parameter::Boolean(_))
            && !matches!(comparison, Comparison::Equal | Comparison::NotEqual)
        {
            return Err(format!(
                "the boolean parameter {parameter} compares with {}",
                comparison.name(),
            ));
        }
        self.transition_mut(index)?
            .conditions
            .push(GraphCondition::new(parameter, comparison, value));
        Ok(())
    }

    pub fn remove_condition(&mut self, index: usize, at: usize) -> Result<(), String> {
        let transition = self.transition_mut(index)?;
        let count = transition.conditions.len();
        if at >= count {
            return Err(format!(
                "the animation transition holds {count} conditions, not {at}"
            ));
        }
        if count == 1 && transition.exit == 0.0 {
            return Err(String::from(
                "an animation transition keeps a condition or an exit time",
            ));
        }
        transition.conditions.remove(at);
        Ok(())
    }

    pub fn validate(&self, holds_animation: impl Fn(&str) -> bool) {
        assert!(
            self.state(&self.entry).is_some(),
            "the animation graph enters {}, which it does not hold",
            self.entry,
        );
        let mut parameters = Vec::new();
        for parameter in &self.parameters {
            assert!(
                valid_id(&parameter.name),
                "the animation graph parameter {} holds no letter, digit, _ or -",
                parameter.name,
            );
            assert!(
                !parameters.contains(&parameter.name.as_str()),
                "the animation graph holds the parameter {} twice",
                parameter.name,
            );
            parameters.push(parameter.name.as_str());
        }
        assert!(
            !self.states.is_empty(),
            "the animation graph holds no state",
        );
        let mut states = Vec::new();
        for state in &self.states {
            assert!(
                valid_id(&state.id),
                "the animation state {} holds no letter, digit, _ or -",
                state.id,
            );
            assert!(
                !states.contains(&state.id.as_str()),
                "the animation graph holds the state {} twice",
                state.id,
            );
            states.push(state.id.as_str());
            assert!(
                holds_animation(state.clip.as_str()),
                "the animation state {} plays the animation {}, which the scene does not hold",
                state.id,
                state.clip,
            );
        }
        for transition in &self.transitions {
            assert!(
                states.contains(&transition.from.as_str()),
                "the animation transition leaves {}, which the graph does not hold",
                transition.from,
            );
            assert!(
                states.contains(&transition.to.as_str()),
                "the animation transition enters {}, which the graph does not hold",
                transition.to,
            );
            assert!(
                transition.from != transition.to,
                "the animation transition leaves {} for itself",
                transition.from,
            );
            assert!(
                transition.fade.is_finite() && transition.fade >= 0.0,
                "the animation transition {} to {} fades over {}",
                transition.from,
                transition.to,
                transition.fade,
            );
            assert!(
                transition.exit.is_finite() && (0.0..=1.0).contains(&transition.exit),
                "the animation transition {} to {} exits at {}",
                transition.from,
                transition.to,
                transition.exit,
            );
            assert!(
                !transition.conditions.is_empty() || transition.exit > 0.0,
                "the animation transition {} to {} holds no condition and no exit time",
                transition.from,
                transition.to,
            );
            for condition in &transition.conditions {
                let parameter = self.parameter(&condition.parameter).unwrap_or_else(|| {
                    panic!(
                        "the animation transition {} to {} conditions the parameter {}, which the graph does not hold",
                        transition.from, transition.to, condition.parameter,
                    )
                });
                assert!(
                    parameter.default.shares_kind(condition.value),
                    "the animation transition {} to {} compares the {} parameter {} with a {}",
                    transition.from,
                    transition.to,
                    parameter.default.name(),
                    condition.parameter,
                    condition.value.name(),
                );
                assert!(
                    !matches!(parameter.default, Parameter::Boolean(_))
                        || matches!(
                            condition.comparison,
                            Comparison::Equal | Comparison::NotEqual
                        ),
                    "the animation transition {} to {} compares the boolean parameter {} with {}",
                    transition.from,
                    transition.to,
                    condition.parameter,
                    condition.comparison.name(),
                );
            }
        }
    }
}

impl GraphParameter {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn default(&self) -> Parameter {
        self.default
    }
}

impl GraphState {
    pub fn new(id: &str, clip: AnimationId, speed: f32, looped: bool) -> Result<Self, String> {
        if !speed.is_finite() {
            return Err(format!("the animation state {id} holds {speed}"));
        }
        Ok(Self {
            id: String::from(id),
            clip,
            speed,
            looped,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn clip(&self) -> &AnimationId {
        &self.clip
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn looped(&self) -> bool {
        self.looped
    }
}

impl GraphTransition {
    fn new(
        from: &str,
        to: &str,
        fade: f32,
        exit: f32,
        conditions: Vec<GraphCondition>,
    ) -> Result<Self, String> {
        if !fade.is_finite() || fade < 0.0 {
            return Err(format!(
                "the animation transition {from} to {to} fades over {fade}"
            ));
        }
        if from == to {
            return Err(format!("the animation transition leaves {from} for itself"));
        }
        if !(0.0..=1.0).contains(&exit) {
            return Err(format!(
                "the animation transition {from} to {to} exits at {exit}"
            ));
        }
        Ok(Self {
            from: String::from(from),
            to: String::from(to),
            fade,
            exit,
            conditions,
        })
    }

    pub fn from(&self) -> &str {
        &self.from
    }

    pub fn to(&self) -> &str {
        &self.to
    }

    pub fn fade(&self) -> f32 {
        self.fade
    }

    pub fn exit(&self) -> f32 {
        self.exit
    }

    pub fn conditions(&self) -> &[GraphCondition] {
        &self.conditions
    }
}

impl GraphCondition {
    fn new(parameter: &str, comparison: Comparison, value: Parameter) -> Self {
        Self {
            parameter: String::from(parameter),
            comparison,
            value,
        }
    }

    pub fn parameter(&self) -> &str {
        &self.parameter
    }

    pub fn comparison(&self) -> Comparison {
        self.comparison
    }

    pub fn value(&self) -> Parameter {
        self.value
    }
}

use crate::ClipSource;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Parameter {
    Number(f32),
    Boolean(bool),
}

impl Parameter {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Number(_) => "number",
            Self::Boolean(_) => "boolean",
        }
    }

    pub fn shares_kind(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Number(_), Self::Number(_)) | (Self::Boolean(_), Self::Boolean(_))
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    Above,
    AboveOrEqual,
    Below,
    BelowOrEqual,
    Equal,
    NotEqual,
}

impl Comparison {
    pub const ALL: [Self; 6] = [
        Self::Above,
        Self::AboveOrEqual,
        Self::Below,
        Self::BelowOrEqual,
        Self::Equal,
        Self::NotEqual,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Above => "above",
            Self::AboveOrEqual => "above_or_equal",
            Self::Below => "below",
            Self::BelowOrEqual => "below_or_equal",
            Self::Equal => "equal",
            Self::NotEqual => "not_equal",
        }
    }

    pub fn of(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|held| held.name() == name)
    }

    pub fn holds(self, left: Parameter, right: Parameter) -> bool {
        assert!(
            left.shares_kind(right),
            "a {} parameter compares with a {} parameter",
            left.name(),
            right.name(),
        );
        match (left, right) {
            (Parameter::Number(left), Parameter::Number(right)) => self.holds_number(left, right),
            (Parameter::Boolean(left), Parameter::Boolean(right)) => match self {
                Self::Equal => left == right,
                Self::NotEqual => left != right,
                _ => panic!("a boolean parameter compares no {}", self.name()),
            },
            _ => unreachable!("a compared pair of parameters shares its kind"),
        }
    }

    fn holds_number(self, left: f32, right: f32) -> bool {
        match self {
            Self::Above => left > right,
            Self::AboveOrEqual => left >= right,
            Self::Below => left < right,
            Self::BelowOrEqual => left <= right,
            Self::Equal => left == right,
            Self::NotEqual => left != right,
        }
    }
}

pub struct ParameterDesc {
    pub name: String,
    pub default: Parameter,
}

pub struct StateDesc {
    pub id: String,
    pub source: Arc<ClipSource>,
    pub speed: f32,
    pub looped: bool,
}

pub struct ConditionDesc {
    pub parameter: String,
    pub comparison: Comparison,
    pub value: Parameter,
}

pub struct TransitionDesc {
    pub from: String,
    pub to: String,
    pub fade: f32,
    pub exit: f32,
    pub conditions: Vec<ConditionDesc>,
}

pub struct ParameterEntry {
    name: String,
    default: Parameter,
}

pub struct State {
    id: String,
    source: Arc<ClipSource>,
    speed: f32,
    looped: bool,
}

pub struct Condition {
    parameter: usize,
    comparison: Comparison,
    value: Parameter,
}

pub struct Transition {
    from: usize,
    to: usize,
    fade: f32,
    exit: f32,
    conditions: Vec<Condition>,
}

pub struct AnimationGraph {
    name: String,
    parameters: Vec<ParameterEntry>,
    states: Vec<State>,
    transitions: Vec<Transition>,
    entry: usize,
}

impl AnimationGraph {
    pub fn new(
        name: impl Into<String>,
        parameters: Vec<ParameterDesc>,
        states: Vec<StateDesc>,
        transitions: Vec<TransitionDesc>,
        entry: usize,
    ) -> Self {
        let name = name.into();
        assert!(!name.is_empty(), "an animation graph holds no name");
        assert!(
            !states.is_empty(),
            "the animation graph {name} holds no state",
        );
        let mut held = Self {
            name,
            parameters: Vec::with_capacity(parameters.len()),
            states: Vec::with_capacity(states.len()),
            transitions: Vec::with_capacity(transitions.len()),
            entry,
        };
        for parameter in parameters {
            let name = assert_unique(
                &format!("the animation graph {}", held.name),
                "parameter",
                &parameter.name,
                held.parameters.iter().map(|held| held.name.as_str()),
            );
            held.parameters.push(ParameterEntry {
                name,
                default: parameter.default,
            });
        }
        for state in states {
            assert!(
                state.speed.is_finite(),
                "the animation state {} of {} holds the speed {}",
                state.id,
                held.name,
                state.speed,
            );
            held.states.push(State {
                id: assert_unique(
                    &format!("the animation graph {}", held.name),
                    "state",
                    &state.id,
                    held.states.iter().map(|held| held.id.as_str()),
                ),
                source: state.source,
                speed: state.speed,
                looped: state.looped,
            });
        }
        assert!(
            held.entry < held.states.len(),
            "the animation graph {} enters the state {} of {}",
            held.name,
            held.entry,
            held.states.len(),
        );
        for transition in transitions {
            let entry = Self::transition(&held, transition);
            held.transitions.push(entry);
        }
        held
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn parameters(&self) -> impl Iterator<Item = (&str, Parameter)> + '_ {
        self.parameters
            .iter()
            .map(|parameter| (parameter.name.as_str(), parameter.default))
    }

    pub fn states(&self) -> &[State] {
        &self.states
    }

    pub fn entry(&self) -> usize {
        self.entry
    }

    pub fn state(&self, id: &str) -> Option<usize> {
        self.states.iter().position(|state| state.id == id)
    }

    pub(crate) fn defaults(&self) -> Vec<Parameter> {
        self.parameters
            .iter()
            .map(|parameter| parameter.default)
            .collect()
    }

    pub(crate) fn parameter(&self, name: &str) -> Option<usize> {
        self.parameters
            .iter()
            .position(|parameter| parameter.name == name)
    }

    pub(crate) fn transitions(&self) -> &[Transition] {
        &self.transitions
    }

    fn transition(&self, transition: TransitionDesc) -> Transition {
        let from = self.state(&transition.from).unwrap_or_else(|| {
            panic!(
                "the animation graph {} transitions from {}, which it does not hold",
                self.name, transition.from,
            )
        });
        let to = self.state(&transition.to).unwrap_or_else(|| {
            panic!(
                "the animation graph {} transitions to {}, which it does not hold",
                self.name, transition.to,
            )
        });
        assert!(
            from != to,
            "the animation graph {} transitions from {} to itself",
            self.name,
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
        let conditions = transition
            .conditions
            .into_iter()
            .map(|condition| {
                let parameter = self.parameter(&condition.parameter).unwrap_or_else(|| {
                    panic!(
                        "the animation transition {} to {} conditions the parameter {}, which the graph {} does not hold",
                        transition.from, transition.to, condition.parameter, self.name,
                    )
                });
                let default = self.parameters[parameter].default;
                assert!(
                    default.shares_kind(condition.value),
                    "the animation transition {} to {} compares the {} parameter {} with a {}",
                    transition.from,
                    transition.to,
                    default.name(),
                    condition.parameter,
                    condition.value.name(),
                );
                assert!(
                    !matches!(default, Parameter::Boolean(_))
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
                Condition {
                    parameter,
                    comparison: condition.comparison,
                    value: condition.value,
                }
            })
            .collect();
        Transition {
            from,
            to,
            fade: transition.fade,
            exit: transition.exit,
            conditions,
        }
    }
}

impl State {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn source(&self) -> &Arc<ClipSource> {
        &self.source
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn looped(&self) -> bool {
        self.looped
    }
}

impl Transition {
    pub(crate) fn from(&self) -> usize {
        self.from
    }

    pub(crate) fn to(&self) -> usize {
        self.to
    }

    pub(crate) fn fade(&self) -> f32 {
        self.fade
    }

    pub(crate) fn exit(&self) -> f32 {
        self.exit
    }

    pub(crate) fn holds(&self, values: &[Parameter]) -> bool {
        self.conditions
            .iter()
            .all(|condition| condition.holds(values))
    }
}

impl Condition {
    fn holds(&self, values: &[Parameter]) -> bool {
        self.comparison.holds(values[self.parameter], self.value)
    }
}

fn assert_unique<'a>(
    what: &str,
    kind: &str,
    id: &str,
    mut held: impl Iterator<Item = &'a str>,
) -> String {
    assert!(!id.is_empty(), "{what} holds a {kind} of no name");
    assert!(
        !held.any(|name| name == id),
        "{what} holds the {kind} {id} twice",
    );
    String::from(id)
}

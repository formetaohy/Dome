use super::{GraphAsset, GraphCondition, GraphEntry, GraphParameter, GraphState, GraphTransition};
use crate::reference::AnimationId;
use crate::scene_value::SceneValue;
use dome_animation::{Comparison, Parameter};
use dome_toml::{Fields, Toml};

impl GraphEntry {
    pub fn authored(&self) -> Toml {
        Toml::object([
            ("id", Toml::string(self.id.as_str())),
            ("entry", Toml::string(self.asset.entry.as_str())),
            (
                "parameters",
                Toml::array(self.asset.parameters.iter().map(parameter).collect()),
            ),
            (
                "states",
                Toml::array(self.asset.states.iter().map(state).collect()),
            ),
            (
                "transitions",
                Toml::array(self.asset.transitions.iter().map(transition).collect()),
            ),
        ])
    }
}

fn parameter(parameter: &GraphParameter) -> Toml {
    let mut fields = vec![("name", Toml::string(parameter.name.as_str()))];
    fields.push(match parameter.default {
        Parameter::Number(value) => ("number", Toml::from(value)),
        Parameter::Boolean(value) => ("boolean", Toml::boolean(value)),
    });
    Toml::object(fields)
}

fn state(state: &GraphState) -> Toml {
    Toml::object([
        ("id", Toml::string(state.id.as_str())),
        ("clip", Toml::string(state.clip.as_str())),
        ("speed", Toml::from(state.speed)),
        ("looped", Toml::boolean(state.looped)),
    ])
}

fn transition(transition: &GraphTransition) -> Toml {
    Toml::object([
        ("from", Toml::string(transition.from.as_str())),
        ("to", Toml::string(transition.to.as_str())),
        ("fade", Toml::from(transition.fade)),
        ("exit", Toml::from(transition.exit)),
        (
            "conditions",
            Toml::array(transition.conditions.iter().map(condition).collect()),
        ),
    ])
}

fn condition(condition: &GraphCondition) -> Toml {
    let mut fields = vec![
        ("parameter", Toml::string(condition.parameter.as_str())),
        ("comparison", Toml::string(condition.comparison.name())),
    ];
    fields.push(match condition.value {
        Parameter::Number(value) => ("number", Toml::from(value)),
        Parameter::Boolean(value) => ("boolean", Toml::boolean(value)),
    });
    Toml::object(fields)
}

pub(crate) fn parse(value: Toml, what: &str) -> GraphEntry {
    let mut fields = Fields::of(value, what);
    let id = fields.need("id").string_value(&format!("the id of {what}"));
    let entry = fields
        .need("entry")
        .string_value(&format!("the entry of {what}"));
    let parameters = match fields.take("parameters") {
        Some(value) => value
            .array_items(&format!("the parameters of {what}"))
            .into_iter()
            .enumerate()
            .map(|(index, value)| parameter_of(value, &format!("parameter {index} of {what}")))
            .collect(),
        None => Vec::new(),
    };
    let states = fields
        .need("states")
        .array_items(&format!("the states of {what}"))
        .into_iter()
        .enumerate()
        .map(|(index, value)| state_of(value, &format!("state {index} of {what}")))
        .collect::<Vec<_>>();
    let transitions = match fields.take("transitions") {
        Some(value) => value
            .array_items(&format!("the transitions of {what}"))
            .into_iter()
            .enumerate()
            .map(|(index, value)| transition_of(value, &format!("transition {index} of {what}")))
            .collect(),
        None => Vec::new(),
    };
    fields.done();
    GraphEntry::new(id, GraphAsset::new(entry, parameters, states, transitions))
}

fn parameter_of(value: Toml, what: &str) -> GraphParameter {
    let mut fields = Fields::of(value, what);
    let name = fields
        .need("name")
        .string_value(&format!("the name of {what}"));
    let default = parameter_value(&mut fields, what);
    fields.done();
    GraphParameter { name, default }
}

fn parameter_value(fields: &mut Fields, what: &str) -> Parameter {
    match (fields.take("number"), fields.take("boolean")) {
        (Some(value), None) => Parameter::Number(value.f32_value(what)),
        (None, Some(value)) => Parameter::Boolean(value.boolean_value(what)),
        _ => panic!("{what} holds no one number or boolean"),
    }
}

fn state_of(value: Toml, what: &str) -> GraphState {
    let mut fields = Fields::of(value, what);
    let id = fields.need("id").string_value(&format!("the id of {what}"));
    let clip = AnimationId::parse(fields.need("clip"), &format!("the clip of {what}"));
    let speed = fields
        .need("speed")
        .f32_value(&format!("the speed of {what}"));
    let looped = fields
        .need("looped")
        .boolean_value(&format!("the loop of {what}"));
    fields.done();
    GraphState::new(&id, clip, speed, looped).unwrap_or_else(|error| panic!("{error}"))
}

fn transition_of(value: Toml, what: &str) -> GraphTransition {
    let mut fields = Fields::of(value, what);
    let from = fields
        .need("from")
        .string_value(&format!("the source of {what}"));
    let to = fields
        .need("to")
        .string_value(&format!("the target of {what}"));
    let fade = fields
        .need("fade")
        .f32_value(&format!("the fade of {what}"));
    let exit = fields
        .need("exit")
        .f32_value(&format!("the exit of {what}"));
    let conditions = match fields.take("conditions") {
        Some(value) => value
            .array_items(&format!("the conditions of {what}"))
            .into_iter()
            .enumerate()
            .map(|(index, value)| condition_of(value, &format!("condition {index} of {what}")))
            .collect(),
        None => Vec::new(),
    };
    fields.done();
    GraphTransition::new(&from, &to, fade, exit, conditions)
        .unwrap_or_else(|error| panic!("{error}"))
}

fn condition_of(value: Toml, what: &str) -> GraphCondition {
    let mut fields = Fields::of(value, what);
    let parameter = fields
        .need("parameter")
        .string_value(&format!("the parameter of {what}"));
    let name = fields
        .need("comparison")
        .string_value(&format!("the comparison of {what}"));
    let comparison =
        Comparison::of(&name).unwrap_or_else(|| panic!("{what} holds the comparison {name}"));
    let value = parameter_value(&mut fields, what);
    fields.done();
    GraphCondition::new(&parameter, comparison, value)
}

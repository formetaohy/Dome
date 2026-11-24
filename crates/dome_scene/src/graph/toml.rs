use super::{GraphAsset, GraphCondition, GraphEntry, GraphParameter, GraphState, GraphTransition};
use crate::reference::AnimationId;
use crate::scene_value::SceneValue;
use crate::target::valid_id;
use dome_animation::{Comparison, Parameter};
use dome_toml::{Fields, Toml};

impl GraphEntry {
    pub(crate) fn authored_fields(&self) -> Vec<(String, Toml)> {
        vec![
            (String::from("id"), Toml::string(self.id.as_str())),
            (
                String::from("entry"),
                Toml::string(self.asset.entry.as_str()),
            ),
            (
                String::from("parameters"),
                Toml::array(self.asset.parameters.iter().map(parameter).collect()),
            ),
            (
                String::from("states"),
                Toml::array(self.asset.states.iter().map(state).collect()),
            ),
            (
                String::from("transitions"),
                Toml::array(self.asset.transitions.iter().map(transition).collect()),
            ),
        ]
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

pub(crate) fn parse(value: Toml, what: &str) -> Result<GraphEntry, String> {
    let mut fields = Fields::of(value, what)?;
    let id = fields
        .need("id")?
        .string_value(&format!("the id of {what}"))?;
    let entry = fields
        .need("entry")?
        .string_value(&format!("the entry of {what}"))?;
    let parameters = match fields.take("parameters") {
        Some(value) => value
            .array_items(&format!("the parameters of {what}"))?
            .into_iter()
            .enumerate()
            .map(|(index, value)| parameter_of(value, &format!("parameter {index} of {what}")))
            .collect::<Result<Vec<_>, String>>()?,
        None => Vec::new(),
    };
    let states = fields
        .need("states")?
        .array_items(&format!("the states of {what}"))?
        .into_iter()
        .enumerate()
        .map(|(index, value)| state_of(value, &format!("state {index} of {what}")))
        .collect::<Result<Vec<_>, String>>()?;
    let transitions = match fields.take("transitions") {
        Some(value) => value
            .array_items(&format!("the transitions of {what}"))?
            .into_iter()
            .enumerate()
            .map(|(index, value)| transition_of(value, &format!("transition {index} of {what}")))
            .collect::<Result<Vec<_>, String>>()?,
        None => Vec::new(),
    };
    fields.done()?;
    if !valid_id(&id) {
        return Err(format!("the graph id {id} holds no letter, digit, _ or -"));
    }
    if states.is_empty() {
        return Err(String::from("an animation graph holds no state"));
    }
    Ok(GraphEntry::new(
        id,
        GraphAsset::new(entry, parameters, states, transitions),
    ))
}

fn parameter_of(value: Toml, what: &str) -> Result<GraphParameter, String> {
    let mut fields = Fields::of(value, what)?;
    let name = fields
        .need("name")?
        .string_value(&format!("the name of {what}"))?;
    let default = parameter_value(&mut fields, what)?;
    fields.done()?;
    Ok(GraphParameter { name, default })
}

fn parameter_value(fields: &mut Fields, what: &str) -> Result<Parameter, String> {
    match (fields.take("number"), fields.take("boolean")) {
        (Some(value), None) => Ok(Parameter::Number(value.f32_value(what)?)),
        (None, Some(value)) => Ok(Parameter::Boolean(value.boolean_value(what)?)),
        _ => Err(format!("{what} holds no one number or boolean")),
    }
}

fn state_of(value: Toml, what: &str) -> Result<GraphState, String> {
    let mut fields = Fields::of(value, what)?;
    let id = fields
        .need("id")?
        .string_value(&format!("the id of {what}"))?;
    let clip = AnimationId::parse(fields.need("clip")?, &format!("the clip of {what}"))?;
    let speed = fields
        .need("speed")?
        .f32_value(&format!("the speed of {what}"))?;
    let looped = fields
        .need("looped")?
        .boolean_value(&format!("the loop of {what}"))?;
    fields.done()?;
    GraphState::new(&id, clip, speed, looped)
}

fn transition_of(value: Toml, what: &str) -> Result<GraphTransition, String> {
    let mut fields = Fields::of(value, what)?;
    let from = fields
        .need("from")?
        .string_value(&format!("the source of {what}"))?;
    let to = fields
        .need("to")?
        .string_value(&format!("the target of {what}"))?;
    let fade = fields
        .need("fade")?
        .f32_value(&format!("the fade of {what}"))?;
    let exit = fields
        .need("exit")?
        .f32_value(&format!("the exit of {what}"))?;
    let conditions = match fields.take("conditions") {
        Some(value) => value
            .array_items(&format!("the conditions of {what}"))?
            .into_iter()
            .enumerate()
            .map(|(index, value)| condition_of(value, &format!("condition {index} of {what}")))
            .collect::<Result<Vec<_>, String>>()?,
        None => Vec::new(),
    };
    fields.done()?;
    GraphTransition::new(&from, &to, fade, exit, conditions)
}

fn condition_of(value: Toml, what: &str) -> Result<GraphCondition, String> {
    let mut fields = Fields::of(value, what)?;
    let parameter = fields
        .need("parameter")?
        .string_value(&format!("the parameter of {what}"))?;
    let name = fields
        .need("comparison")?
        .string_value(&format!("the comparison of {what}"))?;
    let comparison =
        Comparison::of(&name).ok_or_else(|| format!("{what} holds the comparison {name}"))?;
    let value = parameter_value(&mut fields, what)?;
    fields.done()?;
    Ok(GraphCondition::new(&parameter, comparison, value))
}

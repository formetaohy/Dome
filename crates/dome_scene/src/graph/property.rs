use super::{GraphAsset, GraphState, GraphTransition};
use crate::scene_value::SceneValue;
use dome_animation::{Comparison, Parameter};

impl GraphAsset {
    pub(crate) fn read(&self, path: &str) -> Option<crate::value::Value> {
        let segments = path.split('.').collect::<Vec<_>>();
        match segments.as_slice() {
            ["entry"] => Some(crate::value::Value::Text(self.entry.clone())),
            ["parameter", name, "value"] => self.parameter(name).map(|held| value_of(held.default)),
            ["state", id, field] => {
                let state = self.state(id)?;
                match *field {
                    "clip" => Some(crate::value::Value::Text(String::from(state.clip.as_str()))),
                    "speed" => Some(crate::value::Value::Number(state.speed)),
                    "looped" => Some(crate::value::Value::Bool(state.looped)),
                    _ => None,
                }
            }
            ["transition", index, field] => {
                let transition = self.transition_at(index)?;
                match *field {
                    "from" => Some(crate::value::Value::Text(transition.from.clone())),
                    "to" => Some(crate::value::Value::Text(transition.to.clone())),
                    "fade" => Some(crate::value::Value::Number(transition.fade)),
                    "exit" => Some(crate::value::Value::Number(transition.exit)),
                    _ => None,
                }
            }
            ["transition", index, "condition", at, field] => {
                let condition = self
                    .transition_at(index)?
                    .conditions
                    .get(at.parse::<usize>().ok()?)?;
                match *field {
                    "parameter" => Some(crate::value::Value::Text(condition.parameter.clone())),
                    "comparison" => Some(crate::value::Value::Text(String::from(
                        condition.comparison.name(),
                    ))),
                    "value" => Some(value_of(condition.value)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    pub(crate) fn write(
        &mut self,
        path: &str,
        value: &crate::value::Value,
        holds_animation: impl Fn(&str) -> bool,
    ) -> Result<(), String> {
        let segments = path.split('.').collect::<Vec<_>>();
        let what = format!("the animation graph property {path}");
        match segments.as_slice() {
            ["entry"] => {
                let name = value.text()?;
                self.state_name(name)?;
                self.entry = String::from(name);
                Ok(())
            }
            ["parameter", name, "value"] => {
                let parameter = self
                    .parameters
                    .iter_mut()
                    .find(|held| held.name == *name)
                    .ok_or_else(|| format!("the animation graph holds no parameter {name}"))?;
                parameter.default = parameter_value_of(parameter.default, value, &what)?;
                Ok(())
            }
            ["state", id, field] => {
                let state = self.state_mut(id)?;
                match *field {
                    "clip" => {
                        let clip =
                            crate::reference::AnimationId::write(&state.clip, None, value, &what)?;
                        if !holds_animation(clip.as_str()) {
                            return Err(format!(
                                "the animation state {id} plays the animation {clip}, which the scene does not hold"
                            ));
                        }
                        state.clip = clip;
                    }
                    "speed" => {
                        let speed = crate::field::number(value, &what)?;
                        state.speed = speed;
                    }
                    "looped" => state.looped = value.boolean()?,
                    _ => {
                        return Err(format!(
                            "the animation state {id} holds no property {field}"
                        ));
                    }
                }
                Ok(())
            }
            ["transition", index, field] => {
                let at = parse_index(index, &what)?;
                match *field {
                    "from" => {
                        let name = value.text()?;
                        let name = String::from(self.state_name(name)?);
                        let transition = self.transition_mut(at)?;
                        if transition.to == name {
                            return Err(format!("{what} transitions {name} to itself"));
                        }
                        transition.from = name;
                    }
                    "to" => {
                        let name = value.text()?;
                        let name = String::from(self.state_name(name)?);
                        let transition = self.transition_mut(at)?;
                        if transition.from == name {
                            return Err(format!("{what} transitions {name} to itself"));
                        }
                        transition.to = name;
                    }
                    "fade" => {
                        let fade = crate::field::number(value, &what)?;
                        if !fade.is_finite() || fade < 0.0 {
                            return Err(format!("{what} holds {fade}"));
                        }
                        self.transition_mut(at)?.fade = fade;
                    }
                    "exit" => {
                        let exit = crate::field::number(value, &what)?;
                        if !(0.0..=1.0).contains(&exit) {
                            return Err(format!("{what} holds {exit}"));
                        }
                        let transition = self.transition_mut(at)?;
                        if exit == 0.0 && transition.conditions.is_empty() {
                            return Err(format!(
                                "{what} exits an animation transition of no condition"
                            ));
                        }
                        transition.exit = exit;
                    }
                    _ => {
                        return Err(format!(
                            "the animation transition {index} holds no property {field}"
                        ));
                    }
                }
                Ok(())
            }
            ["transition", index, "condition", at, field] => {
                let Self {
                    parameters,
                    transitions,
                    ..
                } = self;
                let transition = transitions
                    .get_mut(parse_index(index, &what)?)
                    .ok_or_else(|| format!("{what} names no transition"))?;
                let condition = transition
                    .conditions
                    .get_mut(parse_index(at, &what)?)
                    .ok_or_else(|| format!("{what} names no condition"))?;
                match *field {
                    "parameter" => {
                        let name = value.text()?;
                        let parameter = parameters
                            .iter()
                            .find(|held| held.name == name)
                            .ok_or_else(|| {
                                format!("the animation graph holds no parameter {name}")
                            })?;
                        if !parameter.default.shares_kind(condition.value) {
                            return Err(format!(
                                "the animation condition of {name} compares a {} with a {}",
                                parameter.default.name(),
                                condition.value.name(),
                            ));
                        }
                        condition.parameter = String::from(name);
                    }
                    "comparison" => {
                        let name = value.text()?;
                        let comparison = Comparison::of(name)
                            .ok_or_else(|| format!("{what} holds the comparison {name}"))?;
                        let parameter = parameters
                            .iter()
                            .find(|held| held.name == condition.parameter)
                            .ok_or_else(|| {
                                format!(
                                    "the animation graph holds no parameter {}",
                                    condition.parameter
                                )
                            })?;
                        if matches!(parameter.default, Parameter::Boolean(_))
                            && !matches!(comparison, Comparison::Equal | Comparison::NotEqual)
                        {
                            return Err(format!(
                                "the boolean parameter {} compares with {name}",
                                condition.parameter,
                            ));
                        }
                        condition.comparison = comparison;
                    }
                    "value" => {
                        let parameter = parameters
                            .iter()
                            .find(|held| held.name == condition.parameter)
                            .ok_or_else(|| {
                                format!(
                                    "the animation graph holds no parameter {}",
                                    condition.parameter
                                )
                            })?;
                        condition.value = parameter_value_of(parameter.default, value, &what)?;
                    }
                    _ => return Err(format!("{what} names no condition property")),
                }
                Ok(())
            }
            _ => Err(format!("{what} names no property")),
        }
    }

    fn transition_at(&self, index: &str) -> Option<&GraphTransition> {
        self.transitions.get(index.parse::<usize>().ok()?)
    }

    fn state_name(&self, id: &str) -> Result<&str, String> {
        self.state(id)
            .map(GraphState::id)
            .ok_or_else(|| format!("the animation graph holds no state {id}"))
    }
}

fn parse_index(index: &str, what: &str) -> Result<usize, String> {
    index
        .parse::<usize>()
        .map_err(|_| format!("{what} names no index {index}"))
}

fn parameter_value_of(
    current: Parameter,
    value: &crate::value::Value,
    what: &str,
) -> Result<Parameter, String> {
    match current {
        Parameter::Number(_) => Ok(Parameter::Number(crate::field::number(value, what)?)),
        Parameter::Boolean(_) => Ok(Parameter::Boolean(value.boolean()?)),
    }
}

fn value_of(parameter: Parameter) -> crate::value::Value {
    match parameter {
        Parameter::Number(value) => crate::value::Value::Number(value),
        Parameter::Boolean(value) => crate::value::Value::Bool(value),
    }
}

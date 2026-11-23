use crate::field::{axis_value, split};
use crate::node::SceneNode;
use crate::registry::Registry;
use crate::target::Target;
use crate::value::Value;
use dome_ecs::World;

pub fn read_runtime_property(
    world: &World,
    registry: &Registry,
    target: &Target,
    path: &str,
) -> Option<Value> {
    let Target::Node(id) = target else {
        return None;
    };
    let entity = world
        .iter::<SceneNode>()
        .find(|(_, node)| node.id == *id)
        .map(|(entity, _)| entity)?;
    let (component, rest) = path.split_once('.')?;
    let registration = registry.registration(component)?;
    let (field, axis) = field_of(registration.spec.runtime_fields, rest);
    let held = (registration.observe)(world, entity)?;
    axis_value(held.read(field)?, axis)
}

pub fn set_runtime_property(
    world: &mut World,
    registry: &Registry,
    target: &Target,
    path: &str,
    value: &Value,
) -> Result<(), String> {
    let Target::Node(id) = target else {
        return Err(format!("the runtime edits no {target}"));
    };
    let entity = world
        .iter::<SceneNode>()
        .find(|(_, node)| node.id == *id)
        .map(|(entity, _)| entity)
        .ok_or_else(|| format!("the runtime holds no node {id}"))?;
    let (component, rest) = path
        .split_once('.')
        .ok_or_else(|| format!("the runtime node {id} holds no property {path}"))?;
    let registration = registry
        .registration(component)
        .ok_or_else(|| format!("the runtime node {id} holds no component {component}"))?;
    if !registration
        .spec
        .runtime_fields
        .iter()
        .any(|held| holds_runtime_field(held, rest) || holds_runtime_field(held, split(rest).0))
    {
        return Err(format!("the runtime node {id} holds no property {path}"));
    }
    let (field, axis) = field_of(registration.spec.runtime_fields, rest);
    (registration.write_world)(world, entity, field, axis, value)
}

fn field_of<'a>(patterns: &[&str], rest: &'a str) -> (&'a str, Option<crate::field::Axis>) {
    if patterns.iter().any(|held| holds_runtime_field(held, rest)) {
        (rest, None)
    } else {
        split(rest)
    }
}

fn holds_runtime_field(pattern: &str, field: &str) -> bool {
    match pattern.strip_suffix(".*") {
        Some(prefix) => field
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.') && rest.len() > 1),
        None => pattern == field,
    }
}

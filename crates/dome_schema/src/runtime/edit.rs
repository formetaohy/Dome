use crate::SceneNode;
use crate::registry::Registry;
use dome_ecs::World;
use dome_scene::Schema;
use dome_scene::Target;
use dome_scene::Value;
use dome_scene::field::{axis_value, split};

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
    if component == "transform" {
        let (field, axis) = crate::runtime::transform::split(rest);
        return crate::runtime::transform::read(world, entity, field)
            .and_then(|value| axis_value(value, axis));
    }
    let spec = registry.spec(component)?;
    let (field, axis) = field_of(spec.runtime_fields, rest);
    let held = registry.observe(component, world, entity)?;
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
    if component == "transform" {
        let (field, axis) = crate::runtime::transform::split(rest);
        crate::runtime::transform::write(world, entity, field, axis, value)?;
        return Ok(());
    }
    let spec = registry
        .spec(component)
        .ok_or_else(|| format!("the runtime node {id} holds no component {component}"))?;
    if !spec
        .runtime_fields
        .iter()
        .any(|held| holds_runtime_field(held, rest) || holds_runtime_field(held, split(rest).0))
    {
        return Err(format!("the runtime node {id} holds no property {path}"));
    }
    let (field, axis) = field_of(spec.runtime_fields, rest);
    registry.write_world(component, world, entity, field, axis, value)
}

fn field_of<'a>(patterns: &[&str], rest: &'a str) -> (&'a str, Option<dome_scene::field::Axis>) {
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

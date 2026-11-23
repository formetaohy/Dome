use dome_ecs::{Entity, LocalTransform, Parent, World, propagate};
use dome_math::{Quat, Transform};
use dome_scene::field::{Axis, euler, finite, vector_toml, with_axis};
use dome_scene::{Effect, Value};
use dome_toml::Toml;

pub fn split(path: &str) -> (&str, Option<Axis>) {
    dome_scene::field::split(path)
}

pub fn read(world: &World, entity: Entity, field: &str) -> Option<Value> {
    let transform = world.get::<Transform>(entity)?;
    Some(match field {
        "translation" => Value::Vector(transform.translation),
        "rotation" => Value::Vector(euler(transform.rotation)),
        "scale" => Value::Vector(transform.scale),
        _ => return None,
    })
}

pub fn observe(world: &World, entity: Entity) -> Option<Toml> {
    let transform = world.get::<Transform>(entity)?;
    Some(Toml::object([
        ("translation", vector_toml(transform.translation.to_array())),
        (
            "rotation",
            vector_toml(euler(transform.rotation).to_array()),
        ),
        ("scale", vector_toml(transform.scale.to_array())),
    ]))
}

pub fn write(
    world: &mut World,
    entity: Entity,
    field: &str,
    axis: Option<Axis>,
    value: &Value,
) -> Result<Effect, String> {
    let parent = match world.get::<Parent>(entity) {
        Some(parent) => *world.get::<Transform>(parent.entity()).ok_or_else(|| {
            format!(
                "the runtime node {} holds no parent transform",
                parent.entity().index()
            )
        })?,
        None => Transform::IDENTITY,
    };
    let current = *world.get::<Transform>(entity).ok_or_else(|| {
        format!(
            "the runtime node {} holds no world transform",
            entity.index()
        )
    })?;
    let mut world_transform = current;
    match field {
        "translation" => {
            let translation = with_axis(current.translation, axis, value, "the translation")?;
            finite(translation, "the translation")?;
            world_transform.translation = translation;
        }
        "rotation" => {
            let angles = with_axis(euler(current.rotation), axis, value, "the rotation")?;
            if !angles.x.is_finite() || !angles.y.is_finite() || !angles.z.is_finite() {
                return Err(format!("the rotation holds {angles:?}"));
            }
            world_transform.rotation = Quat::from_yaw_pitch_roll(
                angles.x.to_radians(),
                angles.y.to_radians(),
                angles.z.to_radians(),
            );
        }
        "scale" => {
            let scale = with_axis(current.scale, axis, value, "the scale")?;
            finite(scale, "the scale")?;
            if scale.x == 0.0 || scale.y == 0.0 || scale.z == 0.0 {
                return Err(format!("the scale {scale:?} collapses the transform"));
            }
            world_transform.scale = scale;
        }
        _ => return Err(format!("the runtime node holds no property {field}")),
    }
    if parent.scale.x == 0.0 || parent.scale.y == 0.0 || parent.scale.z == 0.0 {
        return Err(format!(
            "the runtime node {} holds the parent scale {:?}",
            entity.index(),
            parent.scale
        ));
    }
    world.insert(entity, LocalTransform(parent.decompose(world_transform)));
    propagate(world);
    Ok(Effect::Value)
}

pub fn install(world: &mut World, entity: Entity, transform: Transform) {
    world.insert(entity, LocalTransform(transform));
}

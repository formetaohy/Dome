use crate::component::{Field, FieldKind};
use crate::field::{Axis, array_toml, euler, finite, vector_toml, with_axis};
use crate::value::{Effect, Value};
use dome_math::{Quat, Transform, Vec3};
use dome_toml::{Fields, Toml, floats};

pub const TRANSFORM_FIELDS: &[Field] = &[
    Field {
        name: "translation",
        label: "Translation",
        kind: FieldKind::Vector {
            range: [-10000.0, 10000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "rotation",
        label: "Rotation",
        kind: FieldKind::Rotation {
            range: [-180.0, 180.0],
            speed: 0.5,
        },
    },
    Field {
        name: "scale",
        label: "Scale",
        kind: FieldKind::Vector {
            range: [0.001, 10000.0],
            speed: 0.01,
        },
    },
];

pub fn read(transform: &Transform, field: &str) -> Option<Value> {
    Some(match field {
        "translation" => Value::Vector(transform.translation),
        "rotation" => Value::Vector(euler(transform.rotation)),
        "scale" => Value::Vector(transform.scale),
        _ => return None,
    })
}

pub fn write(
    transform: &mut Transform,
    field: &str,
    axis: Option<Axis>,
    value: &Value,
) -> Result<Effect, String> {
    match field {
        "translation" => {
            let translation = with_axis(transform.translation, axis, value, "the translation")?;
            finite(translation, "the translation")?;
            transform.translation = translation;
        }
        "rotation" => {
            let angles = with_axis(euler(transform.rotation), axis, value, "the rotation")?;
            if !angles.x.is_finite() || !angles.y.is_finite() || !angles.z.is_finite() {
                return Err(format!("the rotation holds {angles:?}"));
            }
            transform.rotation = Quat::from_yaw_pitch_roll(
                angles.x.to_radians(),
                angles.y.to_radians(),
                angles.z.to_radians(),
            );
        }
        "scale" => {
            let scale = with_axis(transform.scale, axis, value, "the scale")?;
            finite(scale, "the scale")?;
            if scale.x == 0.0 || scale.y == 0.0 || scale.z == 0.0 {
                return Err(format!("the scale {scale:?} collapses the transform"));
            }
            transform.scale = scale;
        }
        _ => return Err(format!("the transform holds no property {field}")),
    }
    Ok(Effect::Value)
}

pub fn authored(transform: &Transform) -> Toml {
    Toml::object([
        ("translation", vector_toml(transform.translation.to_array())),
        (
            "rotation",
            array_toml([
                transform.rotation.x,
                transform.rotation.y,
                transform.rotation.z,
                transform.rotation.w,
            ]),
        ),
        ("scale", vector_toml(transform.scale.to_array())),
    ])
}

pub fn observed(transform: &Transform) -> Toml {
    Toml::object([
        ("translation", vector_toml(transform.translation.to_array())),
        (
            "rotation",
            vector_toml(euler(transform.rotation).to_array()),
        ),
        ("scale", vector_toml(transform.scale.to_array())),
    ])
}

pub fn parse(value: Toml, what: &str) -> Result<Transform, String> {
    let mut fields = Fields::of(value, what)?;
    let translation = Vec3::from_array(floats::<3>(
        fields.need("translation")?,
        &format!("the translation of {what}"),
    )?);
    let [x, y, z, w] = floats(fields.need("rotation")?, &format!("the rotation of {what}"))?;
    let scale = Vec3::from_array(floats::<3>(
        fields.need("scale")?,
        &format!("the scale of {what}"),
    )?);
    fields.done()?;
    if scale.x == 0.0 || scale.y == 0.0 || scale.z == 0.0 {
        return Err(format!("{what} holds the scale {scale:?}"));
    }
    let rotation = Quat::new(x, y, z, w);
    if (rotation.dot(rotation) - 1.0).abs() >= 1e-3 {
        return Err(format!(
            "{what} holds the rotation {rotation:?} of length {}",
            rotation.dot(rotation).sqrt(),
        ));
    }
    Ok(Transform {
        translation,
        rotation,
        scale,
    })
}

pub fn validate(transform: &Transform) -> Result<(), String> {
    finite(transform.translation, "the translation")?;
    finite(transform.scale, "the scale")?;
    if transform.scale.x == 0.0 || transform.scale.y == 0.0 || transform.scale.z == 0.0 {
        return Err(format!(
            "the scale {:?} collapses the transform",
            transform.scale
        ));
    }
    if (transform.rotation.dot(transform.rotation) - 1.0).abs() > 1e-3 {
        return Err(format!(
            "the rotation {:?} spans no unit length",
            transform.rotation
        ));
    }
    Ok(())
}

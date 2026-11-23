use crate::value::Value;
use dome_math::{Quat, Vec3};
use dome_toml::Toml;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    fn place(self, current: Vec3, value: f32) -> Vec3 {
        match self {
            Self::X => Vec3::new(value, current.y, current.z),
            Self::Y => Vec3::new(current.x, value, current.z),
            Self::Z => Vec3::new(current.x, current.y, value),
        }
    }
}

pub fn split(path: &str) -> (&str, Option<Axis>) {
    let Some((field, axis)) = path.rsplit_once('.') else {
        return (path, None);
    };
    match axis {
        "x" => (field, Some(Axis::X)),
        "y" => (field, Some(Axis::Y)),
        "z" => (field, Some(Axis::Z)),
        _ => (path, None),
    }
}

pub fn axis_value(value: Value, axis: Option<Axis>) -> Option<Value> {
    let Some(axis) = axis else {
        return Some(value);
    };
    let vector = match value {
        Value::Vector(vector) => vector,
        Value::Color(color) => color.truncate(),
        _ => return None,
    };
    Some(Value::Number(match axis {
        Axis::X => vector.x,
        Axis::Y => vector.y,
        Axis::Z => vector.z,
    }))
}

pub fn no_axis(axis: Option<Axis>, what: &str) -> Result<(), String> {
    match axis {
        Some(_) => Err(format!("{what} holds one value")),
        None => Ok(()),
    }
}

pub fn with_axis(
    current: Vec3,
    axis: Option<Axis>,
    value: &Value,
    what: &str,
) -> Result<Vec3, String> {
    let Some(axis) = axis else {
        return match value {
            Value::Vector(vector) => Ok(*vector),
            Value::Color(color) => Ok(color.truncate()),
            other => Err(format!("{what} reads a vector from a {}", other.kind())),
        };
    };
    let number = number(value, what)?;
    Ok(axis.place(current, number))
}

pub fn number(value: &Value, what: &str) -> Result<f32, String> {
    match value {
        Value::Number(number) if number.is_finite() => Ok(*number),
        Value::Number(number) => Err(format!("{what} holds {number}")),
        other => Err(format!("{what} reads a number from a {}", other.kind())),
    }
}

pub fn whole(value: &Value, max: u16, what: &str) -> Result<u16, String> {
    let number = number(value, what)?;
    if number < 0.0 || number > f32::from(max) || number.fract() != 0.0 {
        return Err(format!("{what} holds {number}"));
    }
    Ok(number as u16)
}

pub fn finite(vector: Vec3, what: &str) -> Result<(), String> {
    if vector.x.is_finite() && vector.y.is_finite() && vector.z.is_finite() {
        Ok(())
    } else {
        Err(format!("{what} holds {vector:?}"))
    }
}

pub fn positive(number: f32, what: &str) -> Result<f32, String> {
    if number > 0.0 && number.is_finite() {
        Ok(number)
    } else {
        Err(format!("{what} holds {number}"))
    }
}

pub fn unit(vector: Vec3, what: &str) -> Result<(), String> {
    if (0.0..=1.0).contains(&vector.x)
        && (0.0..=1.0).contains(&vector.y)
        && (0.0..=1.0).contains(&vector.z)
    {
        Ok(())
    } else {
        Err(format!(
            "{what} holds the color {vector:?} outside the unit cube"
        ))
    }
}

pub fn non_negative(vector: Vec3, what: &str) -> Result<(), String> {
    if vector.x >= 0.0 && vector.y >= 0.0 && vector.z >= 0.0 {
        Ok(())
    } else {
        Err(format!("{what} holds {vector:?} below zero"))
    }
}

pub fn euler(rotation: Quat) -> Vec3 {
    let angles = rotation.to_yaw_pitch_roll();
    Vec3::new(
        angles.x.to_degrees(),
        angles.y.to_degrees(),
        angles.z.to_degrees(),
    )
}

pub fn vector_toml(values: [f32; 3]) -> Toml {
    Toml::array(
        values
            .into_iter()
            .map(|value| Toml::from(zero(value)))
            .collect(),
    )
}

pub fn array_toml<const N: usize>(values: [f32; N]) -> Toml {
    Toml::array(
        values
            .into_iter()
            .map(|value| Toml::from(zero(value)))
            .collect(),
    )
}

pub fn zero(value: f32) -> f32 {
    if value == 0.0 { 0.0 } else { value }
}

use crate::component::{FieldKind, ReferenceKind};
use crate::field::Axis;
use crate::field::{finite, no_axis, number, with_axis};
use crate::scene::Scene;
use crate::value::Value;
use dome_math::Vec3;
use dome_toml::{Toml, floats};

pub trait SceneValue: Clone + PartialEq + Send + Sync + Default + 'static {
    const KIND: FieldKind;

    fn create(_scene: &Scene) -> Result<Self, String> {
        Ok(Self::default())
    }

    fn read(&self) -> Value;

    fn write(current: &Self, axis: Option<Axis>, value: &Value, what: &str)
    -> Result<Self, String>;

    fn authored(&self) -> Toml;

    fn parse(value: Toml, what: &str) -> Result<Self, String>;

    fn validate(&self, scene: &Scene, what: &str) -> Result<(), String>;

    fn references(&self, _kind: ReferenceKind, _id: &str) -> bool {
        false
    }
}

impl SceneValue for f32 {
    const KIND: FieldKind = FieldKind::Number {
        range: [-10000.0, 10000.0],
        speed: 0.01,
    };

    fn read(&self) -> Value {
        Value::Number(*self)
    }

    fn write(
        _current: &Self,
        axis: Option<Axis>,
        value: &Value,
        what: &str,
    ) -> Result<Self, String> {
        no_axis(axis, what)?;
        number(value, what)
    }

    fn authored(&self) -> Toml {
        Toml::from(*self)
    }

    fn parse(value: Toml, what: &str) -> Result<Self, String> {
        let value = value.f32_value(what)?;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(format!("{what} holds {value}"))
        }
    }

    fn validate(&self, _scene: &Scene, what: &str) -> Result<(), String> {
        if self.is_finite() {
            Ok(())
        } else {
            Err(format!("{what} holds {self}"))
        }
    }
}

impl SceneValue for bool {
    const KIND: FieldKind = FieldKind::Bool;

    fn read(&self) -> Value {
        Value::Bool(*self)
    }

    fn write(
        _current: &Self,
        axis: Option<Axis>,
        value: &Value,
        what: &str,
    ) -> Result<Self, String> {
        no_axis(axis, what)?;
        value.boolean()
    }

    fn authored(&self) -> Toml {
        Toml::from(*self)
    }

    fn parse(value: Toml, what: &str) -> Result<Self, String> {
        value.boolean_value(what)
    }

    fn validate(&self, _scene: &Scene, _what: &str) -> Result<(), String> {
        Ok(())
    }
}

impl SceneValue for String {
    const KIND: FieldKind = FieldKind::Text;

    fn read(&self) -> Value {
        Value::Text(self.clone())
    }

    fn write(
        _current: &Self,
        axis: Option<Axis>,
        value: &Value,
        what: &str,
    ) -> Result<Self, String> {
        no_axis(axis, what)?;
        Ok(String::from(value.text()?))
    }

    fn authored(&self) -> Toml {
        Toml::string(self.as_str())
    }

    fn parse(value: Toml, what: &str) -> Result<Self, String> {
        value.string_value(what)
    }

    fn validate(&self, _scene: &Scene, _what: &str) -> Result<(), String> {
        Ok(())
    }
}

impl SceneValue for Vec3 {
    const KIND: FieldKind = FieldKind::Vector {
        range: [-10000.0, 10000.0],
        speed: 0.01,
    };

    fn read(&self) -> Value {
        Value::Vector(*self)
    }

    fn write(
        current: &Self,
        axis: Option<Axis>,
        value: &Value,
        what: &str,
    ) -> Result<Self, String> {
        let vector = with_axis(*current, axis, value, what)?;
        finite(vector, what)?;
        Ok(vector)
    }

    fn authored(&self) -> Toml {
        Toml::array(vec![
            Toml::from(self.x),
            Toml::from(self.y),
            Toml::from(self.z),
        ])
    }

    fn parse(value: Toml, what: &str) -> Result<Self, String> {
        let vector = Vec3::from_array(floats::<3>(value, what)?);
        if vector.x.is_finite() && vector.y.is_finite() && vector.z.is_finite() {
            Ok(vector)
        } else {
            Err(format!("{what} holds {vector:?}"))
        }
    }

    fn validate(&self, _scene: &Scene, what: &str) -> Result<(), String> {
        finite(*self, what)
    }
}

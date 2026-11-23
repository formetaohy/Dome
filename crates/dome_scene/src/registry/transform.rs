use super::{Assets, SceneComponent};
use crate::field::{Axis, array_toml, euler, finite, vector_toml, with_axis};
use crate::scene::Scene;
use crate::schema::{ComponentSpec, Field, FieldKind, Reach};
use crate::value::{Effect, Value};
use dome_ecs::{Entity, LocalTransform, Parent, World, propagate};
use dome_math::{Quat, Transform, Vec3};
use dome_toml::{Fields, Toml, floats};

static FIELDS: &[Field] = &[
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

static RUNTIME_FIELDS: &[&str] = &["translation", "rotation", "scale"];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "transform",
    label: "Transform",
    required: true,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::Node,
};

impl SceneComponent for Transform {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(match field {
            "translation" => Value::Vector(self.translation),
            "rotation" => Value::Vector(euler(self.rotation)),
            "scale" => Value::Vector(self.scale),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        match field {
            "translation" => {
                let translation = with_axis(self.translation, axis, value, "the translation")?;
                finite(translation, "the translation")?;
                self.translation = translation;
            }
            "rotation" => {
                let angles = with_axis(euler(self.rotation), axis, value, "the rotation")?;
                if !angles.x.is_finite() || !angles.y.is_finite() || !angles.z.is_finite() {
                    return Err(format!("the rotation holds {angles:?}"));
                }
                self.rotation = Quat::from_yaw_pitch_roll(
                    angles.x.to_radians(),
                    angles.y.to_radians(),
                    angles.z.to_radians(),
                );
            }
            "scale" => {
                let scale = with_axis(self.scale, axis, value, "the scale")?;
                finite(scale, "the scale")?;
                if scale.x == 0.0 || scale.y == 0.0 || scale.z == 0.0 {
                    return Err(format!("the scale {scale:?} collapses the transform"));
                }
                self.scale = scale;
            }
            _ => return Err(format!("the transform holds no property {field}")),
        }
        Ok(Effect::Value)
    }

    fn authored(&self) -> Toml {
        Toml::object([
            ("translation", vector_toml(self.translation.to_array())),
            (
                "rotation",
                array_toml([
                    self.rotation.x,
                    self.rotation.y,
                    self.rotation.z,
                    self.rotation.w,
                ]),
            ),
            ("scale", vector_toml(self.scale.to_array())),
        ])
    }

    fn observed(&self) -> Toml {
        Toml::object([
            ("translation", vector_toml(self.translation.to_array())),
            ("rotation", vector_toml(euler(self.rotation).to_array())),
            ("scale", vector_toml(self.scale.to_array())),
        ])
    }

    fn install(
        &self,
        _scene: &crate::scene::Scene,
        _node: &str,
        world: &mut World,
        entity: Entity,
        _assets: &dyn Assets,
    ) {
        world.insert(entity, LocalTransform(*self));
    }

    fn validate(&self, _scene: &Scene, _node: &str) -> Result<(), String> {
        finite(self.translation, "the translation")?;
        finite(self.scale, "the scale")?;
        if self.scale.x == 0.0 || self.scale.y == 0.0 || self.scale.z == 0.0 {
            return Err(format!(
                "the scale {:?} collapses the transform",
                self.scale
            ));
        }
        if (self.rotation.dot(self.rotation) - 1.0).abs() > 1e-3 {
            return Err(format!(
                "the rotation {:?} spans no unit length",
                self.rotation
            ));
        }
        Ok(())
    }

    fn create(_scene: &Scene) -> Result<Self, String> {
        Ok(Self::IDENTITY)
    }

    fn from_toml(value: Toml, what: &str) -> Self {
        let mut fields = Fields::of(value, what);
        let translation = Vec3::from_array(floats::<3>(
            fields.need("translation"),
            &format!("the translation of {what}"),
        ));
        let [x, y, z, w] = floats(fields.need("rotation"), &format!("the rotation of {what}"));
        let scale = Vec3::from_array(floats::<3>(
            fields.need("scale"),
            &format!("the scale of {what}"),
        ));
        fields.done();
        assert!(
            scale.x != 0.0 && scale.y != 0.0 && scale.z != 0.0,
            "{what} holds the scale {scale:?}",
        );
        let rotation = Quat::new(x, y, z, w);
        assert!(
            (rotation.dot(rotation) - 1.0).abs() < 1e-3,
            "{what} holds the rotation {rotation:?} of length {}",
            rotation.dot(rotation).sqrt(),
        );
        Self {
            translation,
            rotation,
            scale,
        }
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        world.get::<Transform>(entity).copied()
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<LocalTransform>(entity);
        world.remove::<Transform>(entity);
    }

    fn write_world(
        world: &mut World,
        entity: Entity,
        field: &str,
        axis: Option<Axis>,
        value: &Value,
    ) -> Result<(), String> {
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
        Ok(())
    }
}

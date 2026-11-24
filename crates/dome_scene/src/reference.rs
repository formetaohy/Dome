use crate::component::{FieldKind, ReferenceKind};
use crate::field::{Axis, no_axis};
use crate::scene::Scene;
use crate::scene_value::SceneValue;
use crate::target::valid_id;
use crate::value::Value;
use dome_toml::Toml;
use std::fmt;
use std::ops::Deref;

macro_rules! reference {
    ($name:ident, $kind:expr) => {
        #[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            pub const REFERENCE: ReferenceKind = $kind;

            pub fn new(id: impl Into<String>) -> Self {
                let id = id.into();
                assert!(
                    valid_id(&id),
                    "the {} id {id} holds no letter, digit, _ or -",
                    $kind.name(),
                );
                Self(id)
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self::new(id)
            }
        }

        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self::new(id)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl Deref for $name {
            type Target = str;

            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl SceneValue for $name {
            const KIND: FieldKind = FieldKind::Reference($kind);

            fn create(scene: &Scene) -> Result<Self, String> {
                scene
                    .asset_ids($kind)
                    .first()
                    .map(|id| Self(String::from(*id)))
                    .ok_or_else(|| format!("the scene holds no {} to reference", $kind.name()))
            }

            fn read(&self) -> Value {
                Value::Text(self.0.clone())
            }

            fn write(
                _current: &Self,
                axis: Option<Axis>,
                value: &Value,
                what: &str,
            ) -> Result<Self, String> {
                no_axis(axis, what)?;
                let id = value.text()?;
                if !valid_id(id) {
                    return Err(format!("{what} holds the {} id {id}", $kind.name()));
                }
                Ok(Self(String::from(id)))
            }

            fn authored(&self) -> Toml {
                Toml::string(self.as_str())
            }

            fn parse(value: Toml, what: &str) -> Result<Self, String> {
                let id = value.string_value(what)?;
                if !valid_id(&id) {
                    return Err(format!("{what} holds the {} id {id}", $kind.name()));
                }
                Ok(Self(id))
            }

            fn validate(&self, scene: &Scene, what: &str) -> Result<(), String> {
                if !valid_id(self.as_str()) {
                    return Err(format!("{what} holds the {} id {}", $kind.name(), self.0));
                }
                if !scene.holds_reference($kind, self.as_str()) {
                    return Err(format!(
                        "{what} names the {} {}, which the scene does not hold",
                        $kind.name(),
                        self.0,
                    ));
                }
                Ok(())
            }

            fn references(&self, kind: ReferenceKind, id: &str) -> bool {
                kind == $kind && id == self.as_str()
            }
        }
    };
}

reference!(NodeId, ReferenceKind::Node);
reference!(MeshId, ReferenceKind::Mesh);
reference!(MaterialId, ReferenceKind::Material);
reference!(ImageId, ReferenceKind::Image);
reference!(ModelId, ReferenceKind::Model);
reference!(SkinId, ReferenceKind::Skin);
reference!(AnimationId, ReferenceKind::Animation);
reference!(SoundId, ReferenceKind::Sound);
reference!(AudioBusId, ReferenceKind::AudioBus);
reference!(EnvironmentId, ReferenceKind::Environment);
reference!(GraphId, ReferenceKind::Graph);

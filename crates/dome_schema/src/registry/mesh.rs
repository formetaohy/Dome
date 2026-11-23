use crate::assets::Assets;
use crate::component::SceneComponent;
use crate::node::MeshRef;
use dome_ecs::{Entity, World};
use dome_math::Vec3;
use dome_render::MeshInstance;
use dome_scene::Changed;
use dome_scene::Scene;
use dome_scene::SceneValue;
use dome_scene::field::{Axis, non_negative, vector_toml, with_axis};
use dome_scene::{ComponentSpec, Field, FieldKind, Reach, ReferenceKind};
use dome_scene::{Effect, Value};
use dome_scene::{MaterialId, MeshId};
use dome_toml::{Fields, Toml, floats};

static FIELDS: &[Field] = &[
    Field {
        name: "mesh",
        label: "Mesh",
        kind: FieldKind::Reference(ReferenceKind::Mesh),
    },
    Field {
        name: "material",
        label: "Material",
        kind: FieldKind::Reference(ReferenceKind::Material),
    },
    Field {
        name: "tint",
        label: "Tint",
        kind: FieldKind::Color,
    },
];

static RUNTIME_FIELDS: &[&str] = &["tint"];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "mesh",
    label: "Mesh",
    required: false,
    fields: FIELDS,
    runtime_fields: RUNTIME_FIELDS,
    reach: Reach::Node,
};

impl SceneComponent for MeshRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(match field {
            "mesh" => Value::Text(String::from(self.mesh.as_str())),
            "material" => Value::Text(String::from(self.material.as_str())),
            "tint" => Value::Vector(self.tint),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        match field {
            "mesh" => {
                self.mesh = MeshId::write(&self.mesh, axis, value, "the mesh of the component")?;
                Ok(Effect::Structure)
            }
            "material" => {
                self.material = MaterialId::write(
                    &self.material,
                    axis,
                    value,
                    "the material of the component",
                )?;
                Ok(Effect::Structure)
            }
            "tint" => {
                let tint = with_axis(self.tint, axis, value, "the tint")?;
                non_negative(tint, "the tint")?;
                self.tint = tint;
                Ok(Effect::Value)
            }
            _ => Err(format!("the mesh holds no property {field}")),
        }
    }

    fn authored(&self) -> Toml {
        Toml::object([
            ("mesh", Toml::string(self.mesh.as_str())),
            ("material", Toml::string(self.material.as_str())),
            ("tint", vector_toml(self.tint.to_array())),
        ])
    }

    fn observed(&self) -> Toml {
        self.authored()
    }

    fn runtime(&self) -> Toml {
        Toml::object([("tint", vector_toml(self.tint.to_array()))])
    }

    fn install(
        &self,
        _scene: &dome_scene::Scene,
        _node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    ) {
        let mesh = assets.mesh(self.mesh.as_str());
        let material = assets.material(self.material.as_str());
        world.insert(
            entity,
            MeshInstance {
                mesh,
                material,
                tint: self.tint,
            },
        );
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        self.mesh
            .validate(scene, &format!("the mesh of node {node}"))?;
        self.material
            .validate(scene, &format!("the material of node {node}"))?;
        non_negative(self.tint, &format!("the tint of node {node}"))
    }

    fn create(scene: &Scene) -> Result<Self, String> {
        let mesh = MeshId::new(
            scene
                .meshes()
                .first()
                .ok_or_else(|| String::from("the scene holds no mesh to render"))?
                .id
                .as_str(),
        );
        let material = MaterialId::new(
            scene
                .materials()
                .first()
                .ok_or_else(|| String::from("the scene holds no material to render"))?
                .id
                .as_str(),
        );
        Ok(Self {
            mesh,
            material,
            tint: Vec3::ONE,
        })
    }

    fn from_toml(value: Toml, what: &str) -> Self {
        let mut fields = Fields::of(value, what);
        let mesh = MeshId::parse(fields.need("mesh"), &format!("the mesh of {what}"));
        let material =
            MaterialId::parse(fields.need("material"), &format!("the material of {what}"));
        let tint = Vec3::from_array(floats::<3>(
            fields.need("tint"),
            &format!("the tint of {what}"),
        ));
        fields.done();
        Self {
            mesh,
            material,
            tint,
        }
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        world.get::<MeshInstance>(entity).map(|instance| Self {
            mesh: MeshId::default(),
            material: MaterialId::default(),
            tint: instance.tint,
        })
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<MeshInstance>(entity);
    }

    fn references(&self, kind: ReferenceKind, id: &str) -> bool {
        (kind == ReferenceKind::Mesh && self.mesh.as_str() == id)
            || (kind == ReferenceKind::Material && self.material.as_str() == id)
    }

    fn stale(&self, _changed: &Changed) -> bool {
        false
    }

    fn write_world(
        world: &mut World,
        entity: Entity,
        field: &str,
        axis: Option<Axis>,
        value: &Value,
    ) -> Result<(), String> {
        if field != "tint" {
            return Err(format!("the runtime mesh holds no property {field}"));
        }
        let mut instance = *world
            .get::<MeshInstance>(entity)
            .ok_or_else(|| format!("the runtime node {} renders no mesh", entity.index()))?;
        let tint = with_axis(instance.tint, axis, value, "the tint")?;
        non_negative(tint, "the tint")?;
        instance.tint = tint;
        world.insert(entity, instance);
        Ok(())
    }
}

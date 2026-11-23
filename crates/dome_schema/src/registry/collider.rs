use crate::assets::Assets;
use crate::body::BodyRef;
use crate::collider::{ColliderRef, ShapeKind};
use crate::component::NodeComponents;
use crate::component::SceneComponent;
use dome_ecs::{Entity, World};
use dome_math::Vec3;
use dome_physics::{ALL_LAYERS, BodyKind, Collider, LAYER_COUNT};
use dome_scene::MeshId;
use dome_scene::Scene;
use dome_scene::field::{Axis, finite, no_axis, number, positive, vector_toml, whole, with_axis};
use dome_scene::valid_id;
use dome_scene::{ComponentSpec, Field, FieldKind, Reach, ReferenceKind};
use dome_scene::{Effect, Value};
use dome_toml::{Fields, Toml, floats};

static LAYER_CHOICES: &[&str] = &[
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15",
];

static FIELDS: &[Field] = &[
    Field {
        name: "shape",
        label: "Shape",
        kind: FieldKind::Choice(&["sphere", "cuboid", "capsule", "mesh"]),
    },
    Field {
        name: "mesh",
        label: "Mesh",
        kind: FieldKind::OptionalReference(ReferenceKind::Mesh),
    },
    Field {
        name: "radius",
        label: "Radius",
        kind: FieldKind::Number {
            range: [0.0001, 10000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "half_extents",
        label: "Half Extents",
        kind: FieldKind::Vector {
            range: [0.0001, 10000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "half_height",
        label: "Half Height",
        kind: FieldKind::Number {
            range: [0.0, 10000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "center",
        label: "Center",
        kind: FieldKind::Vector {
            range: [-10000.0, 10000.0],
            speed: 0.01,
        },
    },
    Field {
        name: "friction",
        label: "Friction",
        kind: FieldKind::Number {
            range: [0.0, 1.0],
            speed: 0.005,
        },
    },
    Field {
        name: "restitution",
        label: "Restitution",
        kind: FieldKind::Number {
            range: [0.0, 1.0],
            speed: 0.005,
        },
    },
    Field {
        name: "sensor",
        label: "Sensor",
        kind: FieldKind::Bool,
    },
    Field {
        name: "layer",
        label: "Layer",
        kind: FieldKind::Choice(LAYER_CHOICES),
    },
    Field {
        name: "mask",
        label: "Mask",
        kind: FieldKind::Bits {
            count: LAYER_COUNT as u8,
        },
    },
];

pub(crate) static SPEC: ComponentSpec = ComponentSpec {
    name: "collider",
    label: "Collider",
    required: false,
    fields: FIELDS,
    runtime_fields: &[],
    reach: Reach::None,
};

impl SceneComponent for ColliderRef {
    const SPEC: &'static ComponentSpec = &SPEC;

    fn read(&self, field: &str) -> Option<Value> {
        Some(match field {
            "shape" => Value::Text(String::from(self.shape.name())),
            "mesh" => Value::Text(
                self.mesh
                    .as_ref()
                    .map_or(String::new(), |mesh| mesh.as_str().to_owned()),
            ),
            "radius" => Value::Number(self.radius),
            "half_extents" => Value::Vector(self.half_extents),
            "half_height" => Value::Number(self.half_height),
            "center" => Value::Vector(self.center),
            "friction" => Value::Number(self.friction),
            "restitution" => Value::Number(self.restitution),
            "sensor" => Value::Bool(self.sensor),
            "layer" => Value::Number(f32::from(self.layer)),
            "mask" => Value::Number(f32::from(self.mask)),
            _ => return None,
        })
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        match field {
            "shape" => {
                no_axis(axis, "the collider shape")?;
                let name = value.text()?;
                self.shape = ShapeKind::of(name)
                    .ok_or_else(|| format!("the collider holds the shape {name}"))?;
                return Ok(if self.shape == ShapeKind::Mesh {
                    Effect::Structure
                } else {
                    Effect::Value
                });
            }
            "mesh" => {
                no_axis(axis, "the collider mesh")?;
                let id = value.text()?;
                self.mesh = if id.is_empty() {
                    None
                } else {
                    if !valid_id(id) {
                        return Err(format!("the collider mesh holds the id {id}"));
                    }
                    Some(MeshId::new(id))
                };
                return Ok(if self.shape == ShapeKind::Mesh {
                    Effect::Structure
                } else {
                    Effect::Value
                });
            }
            "radius" => {
                no_axis(axis, "the collider radius")?;
                self.radius =
                    positive(number(value, "the collider radius")?, "the collider radius")?;
            }
            "half_extents" => {
                let half_extents =
                    with_axis(self.half_extents, axis, value, "the collider half extents")?;
                for extent in half_extents.to_array() {
                    positive(extent, "the collider half extent")?;
                }
                self.half_extents = half_extents;
            }
            "half_height" => {
                no_axis(axis, "the collider half height")?;
                let half_height = number(value, "the collider half height")?;
                if half_height < 0.0 {
                    return Err(format!("the collider holds the half height {half_height}"));
                }
                self.half_height = half_height;
            }
            "center" => {
                let center = with_axis(self.center, axis, value, "the collider center")?;
                finite(center, "the collider center")?;
                self.center = center;
            }
            "friction" => {
                no_axis(axis, "the collider friction")?;
                let friction = number(value, "the collider friction")?;
                if !(0.0..=1.0).contains(&friction) {
                    return Err(format!("the collider holds the friction {friction}"));
                }
                self.friction = friction;
            }
            "restitution" => {
                no_axis(axis, "the collider restitution")?;
                let restitution = number(value, "the collider restitution")?;
                if !(0.0..=1.0).contains(&restitution) {
                    return Err(format!("the collider holds the restitution {restitution}"));
                }
                self.restitution = restitution;
            }
            "sensor" => {
                no_axis(axis, "the collider sensor")?;
                self.sensor = value.boolean()?;
            }
            "layer" => {
                no_axis(axis, "the collider layer")?;
                let layer = match value {
                    Value::Text(text) => text
                        .parse::<u16>()
                        .map_err(|_| format!("the collider holds the layer {text}"))?,
                    other => whole(other, LAYER_COUNT - 1, "the collider layer")?,
                };
                if layer >= LAYER_COUNT {
                    return Err(format!("the collider holds the layer {layer}"));
                }
                self.layer = layer;
            }
            "mask" => {
                no_axis(axis, "the collider mask")?;
                self.mask = whole(value, ALL_LAYERS, "the collider mask")?;
            }
            _ => return Err(format!("the collider holds no property {field}")),
        }
        Ok(Effect::Value)
    }

    fn authored(&self) -> Toml {
        Toml::object([
            ("shape", Toml::string(self.shape.name())),
            (
                "mesh",
                Toml::string(self.mesh.as_ref().map_or("", |mesh| mesh.as_str())),
            ),
            ("radius", Toml::from(self.radius)),
            ("half_extents", vector_toml(self.half_extents.to_array())),
            ("half_height", Toml::from(self.half_height)),
            ("center", vector_toml(self.center.to_array())),
            ("friction", Toml::from(self.friction)),
            ("restitution", Toml::from(self.restitution)),
            ("sensor", Toml::boolean(self.sensor)),
            ("layer", Toml::whole(u64::from(self.layer))),
            ("mask", Toml::whole(u64::from(self.mask))),
        ])
    }

    fn observed(&self) -> Toml {
        self.authored()
    }

    fn runtime(&self) -> Toml {
        let mut fields = vec![
            ("shape", Toml::string(self.shape.name())),
            ("center", vector_toml(self.center.to_array())),
            ("friction", Toml::from(self.friction)),
            ("restitution", Toml::from(self.restitution)),
            ("sensor", Toml::boolean(self.sensor)),
            ("layer", Toml::whole(u64::from(self.layer))),
            ("mask", Toml::whole(u64::from(self.mask))),
        ];
        match self.shape {
            ShapeKind::Sphere => fields.push(("radius", Toml::from(self.radius))),
            ShapeKind::Cuboid => {
                fields.push(("half_extents", vector_toml(self.half_extents.to_array())))
            }
            ShapeKind::Capsule => {
                fields.push(("radius", Toml::from(self.radius)));
                fields.push(("half_height", Toml::from(self.half_height)));
            }
            ShapeKind::Mesh => {}
        }
        Toml::object(fields)
    }

    fn install(
        &self,
        _scene: &dome_scene::Scene,
        _node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    ) {
        world.insert(entity, self.collider(assets));
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        let index = scene
            .node_index(node)
            .ok_or_else(|| format!("the scene holds no node {node}"))?;
        for scale in [
            scene.transform(index).scale,
            scene.world_transform(index).scale,
        ] {
            let tolerance = 1e-4 * scale.x.abs().max(scale.y.abs()).max(scale.z.abs());
            if !(scale.x > 0.0
                && (scale.x - scale.y).abs() <= tolerance
                && (scale.x - scale.z).abs() <= tolerance)
            {
                return Err(format!(
                    "the collider of node {node} holds the scale {scale:?}",
                ));
            }
        }
        if scene.nodes()[index].has_component("animation")
            || scene.ancestors_hold(index, "animation")
        {
            return Err(format!(
                "the collider of node {node} shares its transform with an animation",
            ));
        }
        if !(self.radius.is_finite() && self.radius > 0.0) {
            return Err(format!(
                "the collider of node {node} holds the radius {}",
                self.radius,
            ));
        }
        for extent in self.half_extents.to_array() {
            if !(extent.is_finite() && extent > 0.0) {
                return Err(format!(
                    "the collider of node {node} holds the half extent {extent}",
                ));
            }
        }
        if !(self.half_height.is_finite() && self.half_height >= 0.0) {
            return Err(format!(
                "the collider of node {node} holds the half height {}",
                self.half_height,
            ));
        }
        finite(self.center, &format!("the center of node {node}"))?;
        if self.layer >= LAYER_COUNT {
            return Err(format!(
                "the collider of node {node} holds the layer {}",
                self.layer,
            ));
        }
        for (name, value) in [
            ("friction", self.friction),
            ("restitution", self.restitution),
        ] {
            if !(value.is_finite() && (0.0..=1.0).contains(&value)) {
                return Err(format!(
                    "the collider of node {node} holds the {name} {value}",
                ));
            }
        }
        if self.shape == ShapeKind::Mesh {
            let mesh = self
                .mesh
                .as_ref()
                .ok_or_else(|| format!("the mesh collider of node {node} names no mesh"))?;
            if !scene.holds_reference(ReferenceKind::Mesh, mesh.as_str()) {
                return Err(format!(
                    "the collider of node {node} names the mesh {mesh}, which the scene does not hold",
                ));
            }
            if scene.nodes()[index].has_component("skin") || scene.ancestors_hold(index, "skin") {
                return Err(format!(
                    "the mesh collider of node {node} shares its triangles with a skin",
                ));
            }
            let mut at = Some(index);
            while let Some(current) = at {
                let held = &scene.nodes()[current];
                if held
                    .component::<BodyRef>()
                    .is_some_and(|body| body.kind == BodyKind::Dynamic)
                {
                    return Err(format!(
                        "the mesh collider of node {node} sits on the dynamic body of node {}",
                        held.id,
                    ));
                }
                at = scene.parent_index(current);
            }
        }
        Ok(())
    }

    fn create(_scene: &Scene) -> Result<Self, String> {
        Ok(Self::new())
    }

    fn from_toml(value: Toml, what: &str) -> Self {
        let mut fields = Fields::of(value, what);
        let shape = fields
            .need("shape")
            .string_value(&format!("the shape of {what}"));
        let shape =
            ShapeKind::of(&shape).unwrap_or_else(|| panic!("{what} holds the shape {shape}"));
        let mesh = fields
            .take("mesh")
            .map(|value| {
                let id = value.string_value(&format!("the mesh of {what}"));
                if id.is_empty() {
                    None
                } else {
                    assert!(valid_id(&id), "{what} holds the mesh id {id}");
                    Some(MeshId::new(id))
                }
            })
            .unwrap_or(None);
        let radius = fields
            .need("radius")
            .f32_value(&format!("the radius of {what}"));
        let half_extents = Vec3::from_array(floats::<3>(
            fields.need("half_extents"),
            &format!("the half extents of {what}"),
        ));
        let half_height = fields
            .need("half_height")
            .f32_value(&format!("the half height of {what}"));
        let center = Vec3::from_array(floats::<3>(
            fields.need("center"),
            &format!("the center of {what}"),
        ));
        let friction = fields
            .need("friction")
            .f32_value(&format!("the friction of {what}"));
        let restitution = fields
            .need("restitution")
            .f32_value(&format!("the restitution of {what}"));
        let sensor = fields
            .need("sensor")
            .boolean_value(&format!("the sensor of {what}"));
        let layer = u16::try_from(
            fields
                .need("layer")
                .u64_value(&format!("the layer of {what}")),
        )
        .unwrap_or_else(|_| panic!("{what} holds a layer beyond {LAYER_COUNT}"));
        let mask = u16::try_from(
            fields
                .need("mask")
                .u64_value(&format!("the mask of {what}")),
        )
        .unwrap_or_else(|_| panic!("{what} holds a mask beyond {ALL_LAYERS}"));
        fields.done();
        Self {
            shape,
            mesh,
            radius,
            half_extents,
            half_height,
            center,
            friction,
            restitution,
            sensor,
            layer,
            mask,
        }
    }

    fn observe(world: &World, entity: Entity) -> Option<Self> {
        world.get::<Collider>(entity).map(ColliderRef::observe)
    }

    fn remove(world: &mut World, entity: Entity) {
        world.remove::<Collider>(entity);
    }

    fn references(&self, kind: ReferenceKind, id: &str) -> bool {
        kind == ReferenceKind::Mesh && self.mesh.as_ref().is_some_and(|mesh| mesh.as_str() == id)
    }
}

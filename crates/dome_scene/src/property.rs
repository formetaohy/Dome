use crate::component::ReferenceKind;
use crate::edit::Edit;
use crate::field::{axis_value, no_axis, number, split, unit, with_axis};
use crate::folder::Folder;
use crate::image::{ImageSource, address_name, address_of, filter_name, filter_of};
use crate::material::MaterialAsset;
use crate::mesh::MeshAsset;
use crate::scene::Scene;
use crate::shape::Shape;
use crate::target::Target;
use crate::value::{Effect, Value};
use dome_render::AlphaMode;
use std::path::PathBuf;

impl Scene {
    pub fn property(&self, target: &Target, path: &str) -> Option<Value> {
        let (field, axis) = split(path);
        let value = match target {
            Target::Node(id) => {
                return self
                    .node_index(id)
                    .and_then(|at| self.node_property(at, path));
            }
            Target::Material(id) => self
                .material_index(id)
                .and_then(|at| material_property(&self.materials[at].asset, field)),
            Target::Mesh(id) => self
                .mesh_index(id)
                .and_then(|at| mesh_property(&self.meshes[at].asset, field)),
            Target::Image(id) => self
                .image_index(id)
                .and_then(|at| image_property(&self.images[at], field)),
            Target::Model(id) => self
                .model_index(id)
                .and_then(|at| model_property(&self.models[at].path, field)),
            Target::Sound(id) => self
                .sound_index(id)
                .and_then(|at| sound_property(&self.sounds[at], field)),
            Target::Environment(id) => self
                .environment_index(id)
                .and_then(|at| environment_property(&self.environments[at], field)),
            Target::Graph(id) => self
                .graph_index(id)
                .and_then(|at| self.graphs[at].asset.read(path)),
        }?;
        axis_value(value, axis)
    }

    fn node_property(&self, index: usize, path: &str) -> Option<Value> {
        let node = self.nodes.get(index)?;
        if path == "parent" {
            return Some(Value::Text(node.parent.clone().unwrap_or_default()));
        }
        let (component, rest) = path.split_once('.').unwrap_or((path, ""));
        if component == "transform" {
            if rest.is_empty() {
                return Some(Value::Bool(true));
            }
            let (field, axis) = split(rest);
            return axis_value(crate::transform::read(&node.transform(), field)?, axis);
        }
        if rest.is_empty() {
            return self
                .schema()
                .spec(component)
                .map(|_| Value::Bool(node.has_component(component)));
        }
        let (field, axis) = split(rest);
        let held = node.component_by_name(component)?;
        axis_value(held.read(field)?, axis)
    }
}

impl Edit<'_> {
    pub fn set_property(
        &mut self,
        target: &Target,
        path: &str,
        value: &Value,
    ) -> Result<Effect, String> {
        if !matches!(target, Target::Node(_))
            && let Some(prefab) = self.prefab_owner(target)
        {
            return Err(format!(
                "{target} belongs to the prefab {}",
                prefab.display(),
            ));
        }
        match target {
            Target::Node(id) => {
                let index = self
                    .node_index(id)
                    .ok_or_else(|| format!("the scene holds no node {id}"))?;
                if self.nodes[index].owner.is_some() {
                    self.write_override(index, path, value)
                } else {
                    self.set_node(index, path, value)
                }
            }
            Target::Material(id) => {
                let index = self
                    .material_index(id)
                    .ok_or_else(|| format!("the scene holds no material {id}"))?;
                let (field, axis) = split(path);
                self.change_asset::<crate::material::MaterialEntry, _>(index, |entry, _| {
                    set_material(&mut entry.asset, &entry.id, field, axis, value)
                })
            }
            Target::Mesh(id) => {
                let index = self
                    .mesh_index(id)
                    .ok_or_else(|| format!("the scene holds no mesh {id}"))?;
                let (field, axis) = split(path);
                self.change_asset::<crate::mesh::MeshEntry, _>(index, |entry, _| {
                    set_mesh(&mut entry.asset, &entry.id, field, axis, value)
                })
            }
            Target::Image(id) => {
                let index = self
                    .image_index(id)
                    .ok_or_else(|| format!("the scene holds no image {id}"))?;
                let (field, axis) = split(path);
                self.change_asset::<crate::image::ImageEntry, _>(index, |entry, scene| {
                    set_image(entry, scene, field, axis, value)
                })
            }
            Target::Model(id) => {
                let index = self
                    .model_index(id)
                    .ok_or_else(|| format!("the scene holds no model {id}"))?;
                let (field, axis) = split(path);
                self.change_asset::<crate::model::ModelEntry, _>(index, |entry, scene| {
                    set_model(entry, scene, field, axis, value)
                })
            }
            Target::Sound(id) => {
                let index = self
                    .sound_index(id)
                    .ok_or_else(|| format!("the scene holds no sound {id}"))?;
                let (field, axis) = split(path);
                self.change_asset::<crate::sound::SoundEntry, _>(index, |entry, scene| {
                    set_sound(entry, scene, field, axis, value)
                })
            }
            Target::Environment(id) => {
                let index = self
                    .environment_index(id)
                    .ok_or_else(|| format!("the scene holds no environment {id}"))?;
                let (field, axis) = split(path);
                self.change_asset::<crate::environment::EnvironmentEntry, _>(
                    index,
                    |entry, scene| set_environment(entry, scene, field, axis, value),
                )
            }
            Target::Graph(id) => {
                let index = self
                    .graph_index(id)
                    .ok_or_else(|| format!("the scene holds no graph {id}"))?;
                self.change_asset::<crate::graph::GraphEntry, _>(index, |entry, scene| {
                    entry
                        .asset
                        .write(path, value, |clip| scene.animation_index(clip).is_some())
                        .map(|()| Effect::Structure)
                })
            }
        }
    }

    pub(crate) fn set_node(
        &mut self,
        index: usize,
        path: &str,
        value: &Value,
    ) -> Result<Effect, String> {
        let id = self.nodes[index].id.clone();
        if self.nodes[index].prefab.is_some() && path != "parent" && !path.starts_with("transform.")
        {
            return Err(format!(
                "the prefab instance {id} holds its transform and parent alone",
            ));
        }
        if path == "parent" {
            let name = value.text()?;
            let parent = if name.is_empty() {
                None
            } else {
                let at = self
                    .node_index(name)
                    .ok_or_else(|| format!("the scene holds no node {name}"))?;
                if self.is_ancestor(index, at) {
                    return Err(format!("{id} parents itself through {name}"));
                }
                Some(at)
            };
            self.reparent(index, parent);
            return Ok(Effect::Structure);
        }
        let (component, rest) = path.split_once('.').unwrap_or((path, ""));
        if component == "transform" {
            let (field, axis) = split(rest);
            if rest.is_empty() {
                return Err(format!("the node {id} holds its transform"));
            }
            self.change_node(index, |node, _| {
                let mut transform = node.transform();
                crate::transform::write(&mut transform, field, axis, value)?;
                node.set_transform(transform);
                Ok(())
            })?;
            return Ok(Effect::Value);
        }
        let required = self
            .schema()
            .spec(component)
            .map(|spec| spec.required)
            .ok_or_else(|| format!("the node {id} holds no component {component}"))?;
        if rest.is_empty() {
            let wanted = value.boolean()?;
            if wanted == self.nodes[index].has_component(component) {
                return Ok(Effect::Structure);
            }
            if wanted {
                let created = self.schema().create(self.scene, component)?;
                created.validate(self.scene, &id)?;
                self.change_node(index, |node, _| {
                    node.insert_component(created);
                    Ok(())
                })?;
            } else {
                if required {
                    return Err(format!("the node {id} keeps its {component}"));
                }
                if component == "audio_bus" {
                    if let Some(effect) = ["audio_filter", "audio_compressor", "audio_reverb"]
                        .into_iter()
                        .find(|effect| self.nodes[index].has_component(effect))
                    {
                        return Err(format!("the node {id} keeps its {effect}"));
                    }
                    if self.referenced_by(ReferenceKind::AudioBus, &id) {
                        return Err(format!("the scene references the audio bus {id}"));
                    }
                }
                self.change_node(index, |node, _| {
                    node.remove_component(component);
                    Ok(())
                })?;
            }
            return self.checked_audio(component);
        }
        let (field, axis) = split(rest);
        let mark = self.mark();
        let effect = self.change_node(index, |node, scene| {
            let mut next = node
                .component_by_name(component)
                .ok_or_else(|| format!("the node {id} holds no {component}"))?
                .clone_box();
            let effect = next.write(field, axis, value)?;
            next.validate(scene, &id)?;
            node.insert_component(next);
            Ok(effect)
        });
        let effect = match effect {
            Ok(effect) => effect,
            Err(error) => {
                self.rollback(mark);
                return Err(error);
            }
        };
        if component == "audio_bus"
            && let Err(error) = self.check_audio_buses()
        {
            self.rollback(mark);
            return Err(error);
        }
        Ok(effect)
    }

    fn checked_audio(&mut self, component: &str) -> Result<Effect, String> {
        if component == "audio_bus" {
            self.check_audio_buses()?;
        }
        Ok(Effect::Structure)
    }
}

fn set_material(
    asset: &mut MaterialAsset,
    id: &str,
    field: &str,
    axis: Option<crate::field::Axis>,
    value: &Value,
) -> Result<Effect, String> {
    if matches!(
        field,
        "base_color_image"
            | "metallic_roughness_image"
            | "normal_image"
            | "occlusion_image"
            | "emissive_image"
    ) {
        no_axis(axis, "the material image")?;
        let image = String::from(value.text()?);
        let held = if image.is_empty() {
            None
        } else {
            Some(crate::reference::ImageId::new(image))
        };
        match field {
            "base_color_image" => asset.base_color_image = held,
            "metallic_roughness_image" => asset.metallic_roughness_image = held,
            "normal_image" => asset.normal_image = held,
            "occlusion_image" => asset.occlusion_image = held,
            _ => asset.emissive_image = held,
        }
        return Ok(Effect::Structure);
    }
    let material = asset;
    match field {
        "base_color" | "emissive" => {
            let subject = if field == "base_color" {
                "base color"
            } else {
                "emissive"
            };
            let held = if field == "base_color" {
                &mut material.base_color
            } else {
                &mut material.emissive
            };
            let color = with_axis(
                *held,
                axis,
                value,
                &format!("the {subject} of material {id}"),
            )?;
            unit(color, &format!("the {subject} of material {id}"))?;
            *held = color;
        }
        "metallic" => {
            no_axis(axis, "the material metallic")?;
            let metallic = number(value, &format!("the metallic of material {id}"))?;
            if !(0.0..=1.0).contains(&metallic) {
                return Err(format!("the material {id} holds metallic {metallic}"));
            }
            material.metallic = metallic;
        }
        "alpha" => {
            no_axis(axis, "the material alpha")?;
            let alpha = number(value, &format!("the alpha of material {id}"))?;
            if !(0.0..=1.0).contains(&alpha) {
                return Err(format!("the material {id} holds alpha {alpha}"));
            }
            material.alpha = alpha;
        }
        "alpha_mode" => {
            no_axis(axis, "the material alpha mode")?;
            let name = value.text()?;
            material.alpha_mode = AlphaMode::of(name)
                .ok_or_else(|| format!("the material {id} holds the alpha mode {name}"))?;
        }
        "alpha_cutoff" => {
            no_axis(axis, "the material alpha cutoff")?;
            let cutoff = number(value, &format!("the alpha cutoff of material {id}"))?;
            if !(0.0..=1.0).contains(&cutoff) {
                return Err(format!("the material {id} holds alpha cutoff {cutoff}"));
            }
            material.alpha_cutoff = cutoff;
        }
        "roughness" => {
            no_axis(axis, "the material roughness")?;
            let roughness = number(value, &format!("the roughness of material {id}"))?;
            if roughness <= 0.0 || roughness > 1.0 {
                return Err(format!("the material {id} holds roughness {roughness}"));
            }
            material.roughness = roughness;
        }
        "normal_scale" => {
            no_axis(axis, "the material normal scale")?;
            let scale = number(value, &format!("the normal scale of material {id}"))?;
            if !scale.is_finite() || scale < 0.0 {
                return Err(format!("the material {id} holds normal scale {scale}"));
            }
            material.normal_scale = scale;
        }
        "occlusion_strength" => {
            no_axis(axis, "the material occlusion strength")?;
            let strength = number(value, &format!("the occlusion strength of material {id}"))?;
            if !(0.0..=1.0).contains(&strength) {
                return Err(format!(
                    "the material {id} holds occlusion strength {strength}"
                ));
            }
            material.occlusion_strength = strength;
        }
        other => return Err(format!("the material {id} holds no property {other}")),
    }
    Ok(Effect::Structure)
}

fn set_mesh(
    asset: &mut MeshAsset,
    id: &str,
    field: &str,
    axis: Option<crate::field::Axis>,
    value: &Value,
) -> Result<Effect, String> {
    no_axis(axis, "the mesh shape")?;
    if field != "shape" {
        return Err(format!("the mesh {id} holds no property {field}"));
    }
    let name = value.text()?;
    let shape =
        Shape::from_name(name).ok_or_else(|| format!("the mesh {id} holds the shape {name}"))?;
    *asset = MeshAsset::shape(shape);
    Ok(Effect::Structure)
}

fn set_image(
    entry: &mut crate::image::ImageEntry,
    scene: &Scene,
    field: &str,
    axis: Option<crate::field::Axis>,
    value: &Value,
) -> Result<Effect, String> {
    let id = &entry.id;
    match field {
        "path" => {
            no_axis(axis, "the image path")?;
            let ImageSource::Path(held) = &mut entry.source else {
                return Err(format!("the image {id} reads its bytes from its model"));
            };
            let path = checked_asset(scene, "image", id, value)?;
            *held = path;
        }
        "min_filter" | "mag_filter" | "mip_filter" | "wrap_u" | "wrap_v" => {
            no_axis(axis, "the image sampler")?;
            let name = value.text()?;
            match field {
                "min_filter" => {
                    entry.sampler.min_filter = filter_of(name)
                        .ok_or_else(|| format!("the image {id} holds the filter {name}"))?;
                }
                "mag_filter" => {
                    entry.sampler.mag_filter = filter_of(name)
                        .ok_or_else(|| format!("the image {id} holds the filter {name}"))?;
                }
                "mip_filter" => {
                    entry.sampler.mip_filter = filter_of(name)
                        .ok_or_else(|| format!("the image {id} holds the filter {name}"))?;
                }
                "wrap_u" => {
                    entry.sampler.address_u = address_of(name)
                        .ok_or_else(|| format!("the image {id} holds the wrap {name}"))?;
                }
                _ => {
                    entry.sampler.address_v = address_of(name)
                        .ok_or_else(|| format!("the image {id} holds the wrap {name}"))?;
                }
            }
        }
        other => return Err(format!("the image {id} holds no property {other}")),
    }
    Ok(Effect::Structure)
}

fn set_environment(
    entry: &mut crate::environment::EnvironmentEntry,
    scene: &Scene,
    field: &str,
    axis: Option<crate::field::Axis>,
    value: &Value,
) -> Result<Effect, String> {
    let id = entry.id.clone();
    match &mut entry.source {
        crate::environment::EnvironmentSource::Panorama(path) => {
            no_axis(axis, "the environment path")?;
            if field != "path" {
                return Err(format!("the environment {id} holds no property {field}"));
            }
            let next = checked_asset(scene, "environment", &id, value)?;
            *path = next;
        }
        crate::environment::EnvironmentSource::Atmosphere(atmosphere) => {
            let what = |subject: &str| format!("the {subject} of environment {id}");
            let parameter = |axis: Option<crate::field::Axis>, value: &Value| {
                no_axis(axis, &format!("the {field} of environment {id}"))?;
                number(value, &what(field))
            };
            match field {
                "sun_elevation" => {
                    let held = parameter(axis, value)?;
                    if !(-90.0..=90.0).contains(&held) {
                        return Err(format!(
                            "the environment {id} holds the sun elevation {held}",
                        ));
                    }
                    atmosphere.sun_elevation = held;
                }
                "sun_azimuth" => {
                    let held = parameter(axis, value)?;
                    if !(0.0..=360.0).contains(&held) {
                        return Err(format!("the environment {id} holds the sun azimuth {held}",));
                    }
                    atmosphere.sun_azimuth = held;
                }
                "sun_intensity" => {
                    let held = parameter(axis, value)?;
                    if held <= 0.0 {
                        return Err(format!(
                            "the environment {id} holds the sun intensity {held}",
                        ));
                    }
                    atmosphere.sun_intensity = held;
                }
                "rayleigh" => {
                    let held = parameter(axis, value)?;
                    if held < 0.0 {
                        return Err(format!(
                            "the environment {id} holds the Rayleigh scale {held}",
                        ));
                    }
                    atmosphere.rayleigh = held;
                }
                "mie" => {
                    let held = parameter(axis, value)?;
                    if held < 0.0 {
                        return Err(format!("the environment {id} holds the Mie scale {held}",));
                    }
                    atmosphere.mie = held;
                }
                "mie_anisotropy" => {
                    let held = parameter(axis, value)?;
                    if !(-1.0..1.0).contains(&held) {
                        return Err(format!(
                            "the environment {id} holds the Mie anisotropy {held}",
                        ));
                    }
                    atmosphere.mie_anisotropy = held;
                }
                "ozone" => {
                    let held = parameter(axis, value)?;
                    if held < 0.0 {
                        return Err(format!("the environment {id} holds the ozone scale {held}",));
                    }
                    atmosphere.ozone = held;
                }
                "ground_albedo" => {
                    let held = with_axis(
                        atmosphere.ground_albedo,
                        axis,
                        value,
                        &what("ground albedo"),
                    )?;
                    unit(held, &what("ground albedo"))?;
                    atmosphere.ground_albedo = held;
                }
                "altitude" => {
                    let held = parameter(axis, value)?;
                    if held < 0.0 {
                        return Err(format!("the environment {id} holds the altitude {held}",));
                    }
                    atmosphere.altitude = held;
                }
                _ => {
                    return Err(format!("the environment {id} holds no property {field}",));
                }
            }
        }
        crate::environment::EnvironmentSource::Capture => {
            return Err(format!(
                "the captured environment {id} holds no authored property {field}",
            ));
        }
    }
    Ok(Effect::Structure)
}

fn set_sound(
    entry: &mut crate::sound::SoundEntry,
    scene: &Scene,
    field: &str,
    axis: Option<crate::field::Axis>,
    value: &Value,
) -> Result<Effect, String> {
    if field == "stream" {
        no_axis(axis, "the sound stream")?;
        entry.stream = value.boolean()?;
        return Ok(Effect::Structure);
    }
    no_axis(axis, "the sound path")?;
    if field != "path" {
        return Err(format!("the sound {} holds no property {field}", entry.id));
    }
    let path = checked_asset(scene, "sound", &entry.id, value)?;
    entry.path = path;
    Ok(Effect::Structure)
}

fn set_model(
    entry: &mut crate::model::ModelEntry,
    scene: &Scene,
    field: &str,
    axis: Option<crate::field::Axis>,
    value: &Value,
) -> Result<Effect, String> {
    no_axis(axis, "the model path")?;
    if field != "path" {
        return Err(format!("the model {} holds no property {field}", entry.id));
    }
    let path = checked_asset(scene, "model", &entry.id, value)?;
    entry.path = path;
    Ok(Effect::Structure)
}

fn material_property(asset: &MaterialAsset, field: &str) -> Option<Value> {
    let image = |held: &Option<crate::reference::ImageId>| {
        Some(Value::Text(
            held.as_ref()
                .map_or(String::new(), |id| id.as_str().to_owned()),
        ))
    };
    match field {
        "base_color" => Some(Value::Vector(asset.base_color)),
        "alpha" => Some(Value::Number(asset.alpha)),
        "alpha_mode" => Some(Value::Text(String::from(asset.alpha_mode.name()))),
        "alpha_cutoff" => Some(Value::Number(asset.alpha_cutoff)),
        "base_color_image" => image(&asset.base_color_image),
        "metallic" => Some(Value::Number(asset.metallic)),
        "roughness" => Some(Value::Number(asset.roughness)),
        "metallic_roughness_image" => image(&asset.metallic_roughness_image),
        "normal_image" => image(&asset.normal_image),
        "normal_scale" => Some(Value::Number(asset.normal_scale)),
        "occlusion_image" => image(&asset.occlusion_image),
        "occlusion_strength" => Some(Value::Number(asset.occlusion_strength)),
        "emissive" => Some(Value::Vector(asset.emissive)),
        "emissive_image" => image(&asset.emissive_image),
        _ => None,
    }
}

fn mesh_property(asset: &MeshAsset, field: &str) -> Option<Value> {
    match field {
        "shape" => asset
            .shape_of()
            .map(|shape| Value::Text(String::from(shape.name()))),
        "vertices" => match &asset.geometry {
            crate::mesh::Geometry::Vertices(vertices) => Some(Value::Number(vertices.len() as f32)),
            crate::mesh::Geometry::Shape(_) | crate::mesh::Geometry::Model { .. } => None,
        },
        _ => None,
    }
}

fn image_property(entry: &crate::image::ImageEntry, field: &str) -> Option<Value> {
    match field {
        "path" => entry
            .source
            .path()
            .map(|path| Value::Text(path.display().to_string())),
        "min_filter" => Some(Value::Text(String::from(filter_name(
            entry.sampler.min_filter,
        )))),
        "mag_filter" => Some(Value::Text(String::from(filter_name(
            entry.sampler.mag_filter,
        )))),
        "mip_filter" => Some(Value::Text(String::from(filter_name(
            entry.sampler.mip_filter,
        )))),
        "wrap_u" => Some(Value::Text(String::from(address_name(
            entry.sampler.address_u,
        )))),
        "wrap_v" => Some(Value::Text(String::from(address_name(
            entry.sampler.address_v,
        )))),
        _ => None,
    }
}

fn checked_asset(scene: &Scene, kind: &str, id: &str, value: &Value) -> Result<PathBuf, String> {
    let path = crate::folder::relative_path(value.text()?);
    if !Folder::Asset.holds(&path) {
        return Err(format!(
            "the {kind} {id} holds the path {} outside the assets folder",
            path.display(),
        ));
    }
    if !scene.asset(&path).is_file() {
        return Err(format!(
            "the {kind} {id} names {}, which the project {} does not hold",
            path.display(),
            scene.root().display(),
        ));
    }
    Ok(path)
}

fn model_property(path: &std::path::Path, field: &str) -> Option<Value> {
    match field {
        "path" => Some(Value::Text(path.display().to_string())),
        _ => None,
    }
}

fn environment_property(
    entry: &crate::environment::EnvironmentEntry,
    field: &str,
) -> Option<Value> {
    match &entry.source {
        crate::environment::EnvironmentSource::Panorama(path) => {
            (field == "path").then(|| Value::Text(path.display().to_string()))
        }
        crate::environment::EnvironmentSource::Atmosphere(atmosphere) => match field {
            "sun_elevation" => Some(Value::Number(atmosphere.sun_elevation)),
            "sun_azimuth" => Some(Value::Number(atmosphere.sun_azimuth)),
            "sun_intensity" => Some(Value::Number(atmosphere.sun_intensity)),
            "rayleigh" => Some(Value::Number(atmosphere.rayleigh)),
            "mie" => Some(Value::Number(atmosphere.mie)),
            "mie_anisotropy" => Some(Value::Number(atmosphere.mie_anisotropy)),
            "ozone" => Some(Value::Number(atmosphere.ozone)),
            "ground_albedo" => Some(Value::Vector(atmosphere.ground_albedo)),
            "altitude" => Some(Value::Number(atmosphere.altitude)),
            _ => None,
        },
        crate::environment::EnvironmentSource::Capture => None,
    }
}

fn sound_property(entry: &crate::sound::SoundEntry, field: &str) -> Option<Value> {
    match field {
        "path" => Some(Value::Text(entry.path.display().to_string())),
        "stream" => Some(Value::Bool(entry.stream)),
        _ => None,
    }
}

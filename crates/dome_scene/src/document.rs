use crate::animation::AnimationEntry;
use crate::environment::{EnvironmentEntry, EnvironmentSource};
use crate::graph::GraphEntry;
use crate::image::{ImageEntry, ImageSource, address_name, filter_name};
use crate::material::MaterialEntry;
use crate::mesh::{Geometry, MeshEntry};
use crate::model::ModelEntry;
use crate::node::Node;
use crate::registry::Registry;
use crate::scene::{FORMAT, Scene};
use crate::schema::ReferenceKind;
use crate::skin::SkinEntry;
use crate::sound::SoundEntry;
use dome_render::Vertex;
use dome_toml::Toml;
use std::path::Path;

#[derive(Clone, Copy, PartialEq)]
enum Presentation {
    Authored,
    Observed,
}

impl Scene {
    pub fn observe(&self) -> Toml {
        document(self, Presentation::Observed)
    }
}

pub(crate) fn write(scene: &Scene) -> String {
    document(scene, Presentation::Authored).write()
}

fn document(scene: &Scene, presentation: Presentation) -> Toml {
    let format = if scene.is_prefab() {
        crate::prefab::FORMAT
    } else {
        FORMAT
    };
    Toml::object([
        ("format", Toml::string(format)),
        (
            "models",
            assets(
                scene,
                ReferenceKind::Model,
                presentation,
                &scene.models,
                |entry| entry.id.as_str(),
                model,
            ),
        ),
        (
            "images",
            assets(
                scene,
                ReferenceKind::Image,
                presentation,
                &scene.images,
                |entry| entry.id.as_str(),
                image,
            ),
        ),
        (
            "sounds",
            assets(
                scene,
                ReferenceKind::Sound,
                presentation,
                &scene.sounds,
                |entry| entry.id.as_str(),
                sound,
            ),
        ),
        (
            "environments",
            assets(
                scene,
                ReferenceKind::Environment,
                presentation,
                &scene.environments,
                |entry| entry.id.as_str(),
                environment,
            ),
        ),
        (
            "meshes",
            assets(
                scene,
                ReferenceKind::Mesh,
                presentation,
                &scene.meshes,
                |entry| entry.id.as_str(),
                mesh,
            ),
        ),
        (
            "materials",
            assets(
                scene,
                ReferenceKind::Material,
                presentation,
                &scene.materials,
                |entry| entry.id.as_str(),
                material,
            ),
        ),
        (
            "skins",
            assets(
                scene,
                ReferenceKind::Skin,
                presentation,
                &scene.skins,
                |entry| entry.id.as_str(),
                skin,
            ),
        ),
        (
            "animations",
            assets(
                scene,
                ReferenceKind::Animation,
                presentation,
                &scene.animations,
                |entry| entry.id.as_str(),
                animation,
            ),
        ),
        (
            "graphs",
            assets(
                scene,
                ReferenceKind::Graph,
                presentation,
                &scene.graphs,
                |entry| entry.id.as_str(),
                graph,
            ),
        ),
        ("nodes", nodes(scene, presentation)),
    ])
}

fn assets<T>(
    scene: &Scene,
    kind: ReferenceKind,
    presentation: Presentation,
    entries: &[T],
    id: impl Fn(&T) -> &str,
    build: impl Fn(&T, Option<&Path>) -> Toml,
) -> Toml {
    Toml::array(
        entries
            .iter()
            .filter_map(|entry| {
                let owner = scene.asset_owner(kind, id(entry));
                if owner.is_some() && presentation == Presentation::Authored {
                    return None;
                }
                Some(build(entry, owner))
            })
            .collect(),
    )
}

fn mark_prefab(fields: &mut Vec<(String, Toml)>, owner: Option<&Path>) {
    let Some(owner) = owner else {
        return;
    };
    fields.push((String::from("prefab"), Toml::string(written_path(owner))));
}

fn written_path(path: &Path) -> String {
    path.to_str()
        .unwrap_or_else(|| panic!("the asset path {} holds no text", path.display()))
        .replace('\\', "/")
}

fn model(entry: &ModelEntry, owner: Option<&Path>) -> Toml {
    let mut fields = vec![
        (String::from("id"), Toml::string(entry.id.as_str())),
        (
            String::from("path"),
            Toml::string(written_path(&entry.path)),
        ),
    ];
    mark_prefab(&mut fields, owner);
    Toml::object(fields)
}

fn sound(entry: &SoundEntry, owner: Option<&Path>) -> Toml {
    let mut fields = vec![
        (String::from("id"), Toml::string(entry.id.as_str())),
        (
            String::from("path"),
            Toml::string(written_path(&entry.path)),
        ),
        (String::from("stream"), Toml::boolean(entry.stream)),
    ];
    mark_prefab(&mut fields, owner);
    Toml::object(fields)
}

fn environment(entry: &EnvironmentEntry, owner: Option<&Path>) -> Toml {
    let mut fields = vec![(String::from("id"), Toml::string(entry.id.as_str()))];
    match &entry.source {
        EnvironmentSource::Panorama(path) => {
            fields.push((String::from("path"), Toml::string(written_path(path))))
        }
        EnvironmentSource::Atmosphere(atmosphere) => fields.push((
            String::from("atmosphere"),
            Toml::object([
                ("sun_elevation", Toml::from(atmosphere.sun_elevation)),
                ("sun_azimuth", Toml::from(atmosphere.sun_azimuth)),
                ("sun_intensity", Toml::from(atmosphere.sun_intensity)),
                ("rayleigh", Toml::from(atmosphere.rayleigh)),
                ("mie", Toml::from(atmosphere.mie)),
                ("mie_anisotropy", Toml::from(atmosphere.mie_anisotropy)),
                ("ozone", Toml::from(atmosphere.ozone)),
                ("ground_albedo", vector(atmosphere.ground_albedo.to_array())),
                ("altitude", Toml::from(atmosphere.altitude)),
            ]),
        )),
        EnvironmentSource::Capture => fields.push((String::from("capture"), Toml::boolean(true))),
    }
    mark_prefab(&mut fields, owner);
    Toml::object(fields)
}

fn image(entry: &ImageEntry, owner: Option<&Path>) -> Toml {
    let mut fields = vec![(String::from("id"), Toml::string(entry.id.as_str()))];
    match &entry.source {
        ImageSource::Path(path) => {
            fields.push((String::from("path"), Toml::string(written_path(path))))
        }
        ImageSource::Model { model, image } => {
            fields.push((String::from("model"), Toml::string(model.as_str())));
            fields.push((String::from("image"), Toml::from(*image)));
        }
    }
    fields.push((
        String::from("min_filter"),
        Toml::string(filter_name(entry.sampler.min_filter)),
    ));
    fields.push((
        String::from("mag_filter"),
        Toml::string(filter_name(entry.sampler.mag_filter)),
    ));
    fields.push((
        String::from("mip_filter"),
        Toml::string(filter_name(entry.sampler.mip_filter)),
    ));
    fields.push((
        String::from("wrap_u"),
        Toml::string(address_name(entry.sampler.address_u)),
    ));
    fields.push((
        String::from("wrap_v"),
        Toml::string(address_name(entry.sampler.address_v)),
    ));
    mark_prefab(&mut fields, owner);
    Toml::object(fields)
}

fn mesh(entry: &MeshEntry, owner: Option<&Path>) -> Toml {
    let mut fields = vec![(String::from("id"), Toml::string(entry.id.as_str()))];
    match &entry.asset.geometry {
        Geometry::Shape(shape) => {
            fields.push((String::from("shape"), Toml::string(shape.name())));
        }
        Geometry::Vertices(vertices) => {
            fields.push((
                String::from("vertices"),
                Toml::array(vertices.iter().map(vertex).collect()),
            ));
        }
        Geometry::Model {
            model,
            mesh,
            primitive,
        } => {
            fields.push((String::from("model"), Toml::string(model.as_str())));
            fields.push((String::from("mesh"), Toml::from(*mesh)));
            fields.push((String::from("primitive"), Toml::from(*primitive)));
        }
    }
    mark_prefab(&mut fields, owner);
    Toml::object(fields)
}

fn vertex(vertex: &Vertex) -> Toml {
    Toml::object([
        ("position", vector(vertex.position)),
        ("normal", vector(vertex.normal)),
        ("uv", vector(vertex.uv)),
    ])
}

fn vector<const N: usize>(values: [f32; N]) -> Toml {
    Toml::array(values.into_iter().map(Toml::from).collect())
}

fn material(entry: &MaterialEntry, owner: Option<&Path>) -> Toml {
    let asset = &entry.asset;
    let mut fields = vec![
        (String::from("id"), Toml::string(entry.id.as_str())),
        (
            String::from("base_color"),
            vector(asset.base_color.to_array()),
        ),
        (String::from("alpha"), Toml::from(asset.alpha)),
        (
            String::from("alpha_mode"),
            Toml::string(asset.alpha_mode.name()),
        ),
        (String::from("alpha_cutoff"), Toml::from(asset.alpha_cutoff)),
        (String::from("metallic"), Toml::from(asset.metallic)),
        (String::from("roughness"), Toml::from(asset.roughness)),
        (String::from("normal_scale"), Toml::from(asset.normal_scale)),
        (
            String::from("occlusion_strength"),
            Toml::from(asset.occlusion_strength),
        ),
        (String::from("emissive"), vector(asset.emissive.to_array())),
    ];
    for (name, image) in [
        ("base_color_image", &asset.base_color_image),
        ("metallic_roughness_image", &asset.metallic_roughness_image),
        ("normal_image", &asset.normal_image),
        ("occlusion_image", &asset.occlusion_image),
        ("emissive_image", &asset.emissive_image),
    ] {
        if let Some(image) = image {
            fields.push((String::from(name), Toml::string(image.as_str())));
        }
    }
    mark_prefab(&mut fields, owner);
    Toml::object(fields)
}

fn skin(entry: &SkinEntry, owner: Option<&Path>) -> Toml {
    let mut fields = vec![
        (String::from("id"), Toml::string(entry.id.as_str())),
        (
            String::from("model"),
            Toml::string(entry.asset.model_of().as_str()),
        ),
        (String::from("skin"), Toml::from(entry.asset.index())),
        (
            String::from("joints"),
            Toml::array(
                entry
                    .asset
                    .joints()
                    .iter()
                    .map(|joint| Toml::string(joint.as_str()))
                    .collect(),
            ),
        ),
    ];
    mark_prefab(&mut fields, owner);
    Toml::object(fields)
}

fn graph(entry: &GraphEntry, owner: Option<&Path>) -> Toml {
    let mut fields = entry.authored().object_fields("an authored graph");
    mark_prefab(&mut fields, owner);
    Toml::object(fields)
}

fn animation(entry: &AnimationEntry, owner: Option<&Path>) -> Toml {
    let mut fields = vec![
        (String::from("id"), Toml::string(entry.id.as_str())),
        (
            String::from("model"),
            Toml::string(entry.asset.model_of().as_str()),
        ),
        (String::from("animation"), Toml::from(entry.asset.index())),
        (
            String::from("targets"),
            Toml::array(
                entry
                    .asset
                    .targets()
                    .iter()
                    .map(|target| Toml::string(target.as_str()))
                    .collect(),
            ),
        ),
    ];
    mark_prefab(&mut fields, owner);
    Toml::object(fields)
}

fn nodes(scene: &Scene, presentation: Presentation) -> Toml {
    Toml::array(
        scene
            .nodes
            .iter()
            .filter(|node| node.owner().is_none() || presentation == Presentation::Observed)
            .map(|node| self::node(node, scene.registry(), presentation))
            .collect(),
    )
}

fn node(node: &Node, registry: &Registry, presentation: Presentation) -> Toml {
    let mut fields = vec![(String::from("id"), Toml::string(&node.id))];
    if let Some(parent) = &node.parent {
        fields.push((String::from("parent"), Toml::string(parent)));
    }
    if let Some(prefab) = node.prefab() {
        fields.push((String::from("prefab"), Toml::string(written_path(prefab))));
    }
    if presentation == Presentation::Observed
        && let Some(owner) = node.owner()
    {
        fields.push((String::from("owner"), Toml::string(owner)));
    }
    if !node.overrides().is_empty() {
        fields.push((
            String::from("overrides"),
            Toml::array(
                node.overrides()
                    .iter()
                    .map(|entry| {
                        Toml::object([
                            ("node", Toml::string(entry.node.as_str())),
                            ("path", Toml::string(entry.path.as_str())),
                            ("value", entry.value.to_toml()),
                        ])
                    })
                    .collect(),
            ),
        ));
    }
    if !node.additions().is_empty() {
        fields.push((
            String::from("additions"),
            Toml::array(
                node.additions()
                    .iter()
                    .map(|addition| self::node(addition, registry, presentation))
                    .collect(),
            ),
        ));
    }
    if !node.removals().is_empty() {
        fields.push((
            String::from("removals"),
            Toml::array(
                node.removals()
                    .iter()
                    .map(|removal| Toml::string(removal.as_str()))
                    .collect(),
            ),
        ));
    }
    for component in registry.components() {
        if let Some(held) = node.component_by_name(component.name) {
            let value = match presentation {
                Presentation::Authored => held.authored(),
                Presentation::Observed => held.observed(),
            };
            fields.push((String::from(component.name), value));
        }
    }
    Toml::object(fields)
}

use super::AssetError;
use super::placeholder;
use super::prepare::{
    ImageBytes, Prepare, animation_data, animation_fingerprint, atmosphere_fingerprint,
    environment_data, environment_fingerprint, image_data_of, image_fingerprint, image_source,
    material_fingerprint, mesh_data, mesh_fingerprint, skin_data, skin_fingerprint, sound_data,
    sound_fingerprint, validate_material,
};
use super::work::{
    ImageMips, Job, Prepared, PreparedAnimation, PreparedEnvironment, PreparedImage,
    PreparedMaterial, PreparedMesh, PreparedSkin, PreparedSound, Removed, format_of, image_modes,
};
use crate::library::TextureKey;
use dome_image::{ColorSpace, mip_chain};
use dome_scene::EnvironmentSource;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

pub(super) fn plan(job: Job) -> Prepared {
    let Job {
        scene,
        fingerprints,
        textures: cached_textures,
        sources,
    } = job;
    let mut errors = Vec::new();
    let mut files = BTreeSet::new();
    let mut prepare = Prepare::new(&scene, sources);

    let mut effective_images = HashMap::new();
    let mut pending_images = Vec::new();
    let mut changed_images = HashSet::new();
    for entry in scene.images() {
        let source = image_source(
            entry,
            &scene,
            &mut prepare.sources,
            &mut files,
            &mut prepare.models,
        );
        let fingerprint = image_fingerprint(entry, source.as_ref().ok().copied());
        let previous = fingerprints.images.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            effective_images.insert(entry.id.clone(), fingerprint);
            continue;
        }
        let prepared = source.and_then(|_| {
            image_data_of(
                &scene,
                &entry.id,
                &mut prepare.images,
                &mut prepare.models,
                &mut prepare.sources,
            )
        });
        if let Err(message) = prepared {
            fail(&mut errors, "image", &entry.id, message);
            prepare
                .images
                .insert(entry.id.clone(), Arc::new(placeholder::image()));
        }
        effective_images.insert(entry.id.clone(), fingerprint);
        changed_images.insert(entry.id.clone());
        pending_images.push((entry.clone(), fingerprint));
    }

    let mut pending_materials = Vec::new();
    for entry in scene.materials() {
        let mut dependencies = Vec::new();
        let mut missing = None;
        for image in entry.asset.images().into_iter().flatten() {
            match effective_images.get(image.as_str()) {
                Some(fingerprint) => dependencies.push(*fingerprint),
                None => {
                    missing = Some(String::from(image.as_str()));
                    break;
                }
            }
        }
        let fingerprint = material_fingerprint(entry, &dependencies);
        let previous = fingerprints.materials.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            continue;
        }
        let rejected = match missing {
            Some(image) => Some(format!(
                "the material {} samples the image {image}, which the scene builds none",
                entry.id,
            )),
            None => validate_material(entry).err(),
        };
        match rejected {
            Some(message) => {
                fail(&mut errors, "material", &entry.id, message);
                pending_materials.push((placeholder::material(entry), fingerprint));
            }
            None => pending_materials.push((entry.clone(), fingerprint)),
        }
    }

    let mut modes: HashMap<String, HashSet<ColorSpace>> = HashMap::new();
    for id in &changed_images {
        modes
            .entry(id.clone())
            .or_default()
            .insert(ColorSpace::Srgb);
    }
    for (entry, _) in &pending_materials {
        for (id, color) in image_modes(entry) {
            let Some(id) = id else {
                continue;
            };
            let key: TextureKey = (String::from(id), format_of(color), color);
            if changed_images.contains(id) || !cached_textures.contains(&key) {
                modes.entry(String::from(id)).or_default().insert(color);
            }
        }
    }
    let mut textures = HashMap::new();
    for (id, colors) in modes {
        let data = match prepare.images.get(&id) {
            Some(data) => Arc::clone(data),
            None => match image_data_of(
                &scene,
                &id,
                &mut prepare.images,
                &mut prepare.models,
                &mut prepare.sources,
            ) {
                Ok(data) => data,
                Err(message) => {
                    fail(&mut errors, "image", &id, message);
                    Arc::new(placeholder::image())
                }
            },
        };
        textures.insert(id, Arc::new(mips(&data, &colors)));
    }
    prepare.images.clear();

    let mut images = Vec::new();
    for (entry, fingerprint) in pending_images {
        let mips = textures
            .get(&entry.id)
            .cloned()
            .expect("a pending image holds the levels its colour modes build");
        images.push(PreparedImage {
            entry,
            fingerprint,
            mips,
        });
    }

    let materials = pending_materials
        .into_iter()
        .map(|(entry, fingerprint)| PreparedMaterial { entry, fingerprint })
        .collect();

    let mut meshes = Vec::new();
    for entry in scene.meshes() {
        let model = match entry.asset.geometry() {
            dome_scene::Geometry::Model { model, .. } => prepare.models.digest(
                model.as_str(),
                super::prepare::Scope::Geometry,
                &mut prepare.sources,
                &mut files,
            ),
            dome_scene::Geometry::Shape(_) | dome_scene::Geometry::Vertices(_) => {
                Ok(crate::digest::Digest::default())
            }
        };
        let fingerprint = mesh_fingerprint(entry, model.as_ref().ok().copied());
        let previous = fingerprints.meshes.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            continue;
        }
        let prepared =
            model.and_then(|_| mesh_data(entry, &mut prepare.models, &mut prepare.sources));
        let data = match prepared {
            Ok(data) => data,
            Err(message) => {
                fail(&mut errors, "mesh", &entry.id, message);
                placeholder::mesh()
            }
        };
        meshes.push(PreparedMesh {
            entry: entry.clone(),
            fingerprint,
            data,
        });
    }

    let mut skins = Vec::new();
    for entry in scene.skins() {
        let model = prepare.models.digest(
            entry.asset.model_of().as_str(),
            super::prepare::Scope::Geometry,
            &mut prepare.sources,
            &mut files,
        );
        let fingerprint = skin_fingerprint(entry, model.as_ref().ok().copied());
        let previous = fingerprints.skins.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            continue;
        }
        let prepared = model.and_then(|_| {
            let model = prepare
                .models
                .load(entry.asset.model_of().as_str(), &mut prepare.sources)?;
            skin_data(entry, model)
        });
        let data = match prepared {
            Ok(data) => data,
            Err(message) => {
                fail(&mut errors, "skin", &entry.id, message);
                placeholder::skin(entry)
            }
        };
        skins.push(PreparedSkin {
            entry: entry.clone(),
            fingerprint,
            data,
        });
    }

    let mut animations = Vec::new();
    for entry in scene.animations() {
        let model = prepare.models.digest(
            entry.asset.model_of().as_str(),
            super::prepare::Scope::Geometry,
            &mut prepare.sources,
            &mut files,
        );
        let fingerprint = animation_fingerprint(entry, model.as_ref().ok().copied());
        let previous = fingerprints.animations.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            continue;
        }
        let prepared = model.and_then(|_| {
            let model = prepare
                .models
                .load(entry.asset.model_of().as_str(), &mut prepare.sources)?;
            animation_data(entry, model)
        });
        let clip = match prepared {
            Ok(clip) => clip,
            Err(message) => {
                fail(&mut errors, "animation", &entry.id, message);
                placeholder::animation(&scene, entry)
            }
        };
        animations.push(PreparedAnimation {
            entry: entry.clone(),
            fingerprint,
            clip,
        });
    }

    let mut sounds = Vec::new();
    for entry in scene.sounds() {
        let file = scene.asset(&entry.path);
        files.insert(file.clone());
        let digest = prepare.sources.digest(&file);
        let fingerprint = sound_fingerprint(entry, digest.as_ref().ok().copied());
        let previous = fingerprints.sounds.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            continue;
        }
        let prepared = digest.and_then(|_| sound_data(&scene, entry, &mut prepare.sources));
        let source = match prepared {
            Ok(source) => source,
            Err(message) => {
                fail(&mut errors, "sound", &entry.id, message);
                placeholder::sound()
            }
        };
        sounds.push(PreparedSound {
            entry: entry.clone(),
            fingerprint,
            source,
        });
    }

    let mut environments = Vec::new();
    for entry in scene.environments() {
        let digest = match &entry.source {
            EnvironmentSource::Panorama(path) => {
                let file = scene.asset(path);
                files.insert(file.clone());
                prepare.sources.digest(&file)
            }
            EnvironmentSource::Atmosphere(atmosphere) => Ok(atmosphere_fingerprint(atmosphere)),
            EnvironmentSource::Capture => Ok(crate::digest::Digest::of(entry.id.as_bytes())),
        };
        let fingerprint = environment_fingerprint(entry, digest.as_ref().ok().copied());
        let previous = fingerprints.environments.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            continue;
        }
        let prepared = digest
            .as_ref()
            .map_err(|message| message.clone())
            .and_then(|_| environment_data(&scene, entry, &mut prepare.sources));
        let data = match prepared {
            Ok(data) => data,
            Err(message) => {
                fail(&mut errors, "environment", &entry.id, message);
                placeholder::environment()
            }
        };
        environments.push(PreparedEnvironment {
            entry: entry.clone(),
            fingerprint,
            data,
        });
    }

    let removed = Removed {
        images: absent(&fingerprints.images, scene.images(), |entry| {
            entry.id.as_str()
        }),
        materials: absent(&fingerprints.materials, scene.materials(), |entry| {
            entry.id.as_str()
        }),
        meshes: absent(&fingerprints.meshes, scene.meshes(), |entry| {
            entry.id.as_str()
        }),
        skins: absent(&fingerprints.skins, scene.skins(), |entry| {
            entry.id.as_str()
        }),
        animations: absent(&fingerprints.animations, scene.animations(), |entry| {
            entry.id.as_str()
        }),
        sounds: absent(&fingerprints.sounds, scene.sounds(), |entry| {
            entry.id.as_str()
        }),
        environments: absent(&fingerprints.environments, scene.environments(), |entry| {
            entry.id.as_str()
        }),
    };

    Prepared {
        errors,
        images,
        materials,
        meshes,
        skins,
        animations,
        sounds,
        environments,
        textures,
        changed_images,
        removed,
        sources: prepare.sources,
        files,
    }
}

fn mips(data: &ImageBytes, colors: &HashSet<ColorSpace>) -> ImageMips {
    ImageMips {
        width: data.width,
        height: data.height,
        srgb: mip_chain(data.width, data.height, ColorSpace::Srgb, &data.rgba),
        linear: colors
            .contains(&ColorSpace::Linear)
            .then(|| mip_chain(data.width, data.height, ColorSpace::Linear, &data.rgba)),
    }
}

fn fail(errors: &mut Vec<AssetError>, kind: &'static str, id: &str, message: String) {
    errors.push(AssetError {
        kind,
        id: String::from(id),
        message,
    });
}

fn absent<T>(
    held: &HashMap<String, crate::digest::Digest>,
    entries: &[T],
    id: impl Fn(&T) -> &str,
) -> Vec<String> {
    let present = entries
        .iter()
        .map(|entry| String::from(id(entry)))
        .collect::<HashSet<_>>();
    held.keys()
        .filter(|id| !present.contains(*id))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::work::Fingerprints;
    use super::*;
    use crate::source::Sources;
    use dome_scene::{ReferenceKind, Scene, Shape};

    const SCENE: &str = r#"format = "dome_scene"

images = [
    { id = "broken_image", path = "assets/missing.png", min_filter = "linear", mag_filter = "linear", mip_filter = "linear", wrap_u = "repeat", wrap_v = "repeat" },
]

[[models]]
id = "missing_model"
path = "assets/missing.gltf"

[[meshes]]
id = "broken_mesh"
model = "missing_model"
mesh = 0
primitive = 0

[[meshes]]
id = "cube"
shape = "cube"

[[materials]]
id = "grey"
base_color = [0.8, 0.8, 0.8]
alpha = 1.0
alpha_mode = "opaque"
alpha_cutoff = 0.5
metallic = 0.0
roughness = 0.5
normal_scale = 1.0
occlusion_strength = 1.0
emissive = [0.0, 0.0, 0.0]
base_color_image = "broken_image"

[[nodes]]
id = "camera"
"#;

    #[test]
    fn a_broken_asset_lands_a_placeholder_where_its_siblings_commit() {
        let root = std::env::temp_dir().join(format!("dome_plan_absent_{}", std::process::id()));
        let scene = Scene::parse(
            SCENE,
            root,
            "the plan test scene",
            Arc::new(dome_schema::Registry::default()),
        )
        .expect("the plan test scene parses");
        let prepared = plan(Job {
            scene: scene.clone(),
            fingerprints: Fingerprints::default(),
            textures: HashSet::new(),
            sources: Sources::new(),
        });

        let failed = |kind: &str, id: &str| {
            prepared
                .errors
                .iter()
                .any(|error| error.kind == kind && error.id == id)
        };
        assert!(
            failed("image", "broken_image"),
            "a broken image reports itself"
        );
        assert!(
            failed("mesh", "broken_mesh"),
            "a broken mesh reports itself"
        );

        let image = prepared
            .images
            .iter()
            .find(|item| item.entry.id == "broken_image")
            .expect("a broken image commits its placeholder");
        assert_eq!((image.mips.width, image.mips.height), (1, 1));
        assert_eq!(image.mips.srgb, vec![vec![255, 0, 255, 255]]);

        let mesh = prepared
            .meshes
            .iter()
            .find(|item| item.entry.id == "broken_mesh")
            .expect("a broken mesh commits its placeholder");
        assert_eq!(
            mesh.data.vertices.len(),
            Shape::Cube.vertices().len(),
            "a broken mesh commits the unit cube",
        );
        assert_eq!(
            mesh.data.skin.as_ref().map(Vec::len),
            Some(mesh.data.vertices.len()),
            "a placeholder cube carries one weight record per vertex",
        );

        assert!(
            prepared.meshes.iter().any(|item| item.entry.id == "cube"),
            "the mesh of the scene still commits",
        );
        assert!(
            prepared
                .materials
                .iter()
                .any(|item| item.entry.id == "grey"),
            "a material sampling a broken image still commits",
        );

        let changed = prepared.changed();
        for (kind, id) in [
            (ReferenceKind::Image, "broken_image"),
            (ReferenceKind::Mesh, "broken_mesh"),
            (ReferenceKind::Mesh, "cube"),
            (ReferenceKind::Material, "grey"),
        ] {
            assert!(changed.touches(kind, id), "the batch changes {kind:?}.{id}");
        }

        let fingerprints = Fingerprints {
            images: prepared
                .images
                .iter()
                .map(|item| (item.entry.id.clone(), item.fingerprint))
                .collect(),
            materials: prepared
                .materials
                .iter()
                .map(|item| (item.entry.id.clone(), item.fingerprint))
                .collect(),
            meshes: prepared
                .meshes
                .iter()
                .map(|item| (item.entry.id.clone(), item.fingerprint))
                .collect(),
            skins: prepared
                .skins
                .iter()
                .map(|item| (item.entry.id.clone(), item.fingerprint))
                .collect(),
            animations: prepared
                .animations
                .iter()
                .map(|item| (item.entry.id.clone(), item.fingerprint))
                .collect(),
            sounds: prepared
                .sounds
                .iter()
                .map(|item| (item.entry.id.clone(), item.fingerprint))
                .collect(),
            environments: prepared
                .environments
                .iter()
                .map(|item| (item.entry.id.clone(), item.fingerprint))
                .collect(),
        };

        let retried = plan(Job {
            scene,
            fingerprints,
            textures: HashSet::new(),
            sources: Sources::new(),
        });
        assert!(
            retried.errors.is_empty() && !retried.changed().any(),
            "an unchanged broken input stays on its committed placeholder",
        );
    }
}

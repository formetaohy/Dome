use dome_math::{Quat, Transform, Vec3};
use dome_render::{Camera, DirectionalLight, PointLight};
use dome_scene::{EnvironmentId, Node, Scene, Shape};
use dome_schema::{
    AudioBusRef, AudioListenerRef, EnvironmentRef, MeshRef, NodeComponents, Registry,
};
use std::path::Path;
use std::sync::Arc;

pub(crate) fn scene(path: &Path, registry: Arc<Registry>) -> Scene {
    let root = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut scene = Scene::with_schema(root, registry);
    scene
        .edit(|edit| {
            edit.add_mesh("cube", Shape::Cube)?;
            edit.add_material("grey")?;
            edit.insert_node(
                placed(
                    Node::new("camera"),
                    Transform::from_translation(Vec3::new(0.0, 1.0, 4.0)),
                )
                .with(Camera::new(1.0, 0.1, 1000.0))
                .with(AudioListenerRef),
            )?;
            edit.insert_node(Node::new("master").with(AudioBusRef::default()))?;
            edit.insert_node(
                placed(
                    Node::new("sun"),
                    Transform::from_rotation(
                        Quat::from_rotation_y(-0.55) * Quat::from_rotation_x(-0.75),
                    ),
                )
                .with(DirectionalLight::new(
                    Vec3::ONE,
                    std::f32::consts::PI,
                    100.0,
                )),
            )?;
            let mut lamp = lamp();
            lamp.id = String::from("lamp");
            edit.insert_node(lamp)?;
            let mut cube = mesh("cube", "grey");
            cube.id = String::from("cube");
            edit.insert_node(cube)?;
            Ok(())
        })
        .expect("the editor builds its first scene");
    scene
}

fn placed(mut node: Node, transform: Transform) -> Node {
    node.set_transform(transform);
    node
}

pub(crate) fn mesh(mesh: &str, material: &str) -> Node {
    Node::new("").with(MeshRef::new(mesh, material, Vec3::ONE))
}

pub(crate) fn camera() -> Node {
    Node::new("")
        .with(Camera::new(1.0, 0.1, 1000.0))
        .with(AudioListenerRef)
}

pub(crate) fn sun() -> Node {
    placed(
        Node::new(""),
        Transform::from_rotation(Quat::from_rotation_y(-0.55) * Quat::from_rotation_x(-0.75)),
    )
    .with(DirectionalLight::new(
        Vec3::ONE,
        std::f32::consts::PI,
        100.0,
    ))
}

pub(crate) fn lamp() -> Node {
    placed(
        Node::new(""),
        Transform::from_translation(Vec3::new(0.0, 2.0, 0.0)),
    )
    .with(PointLight::new(Vec3::ONE, 20.0, 10.0))
}

pub(crate) fn probe(environment: &str) -> Node {
    placed(
        Node::new(""),
        Transform::from_scale_rotation_translation(
            Vec3::splat(2.0),
            Quat::IDENTITY,
            Vec3::new(0.0, 1.0, 0.0),
        ),
    )
    .with(EnvironmentRef::new(EnvironmentId::new(environment)).probe(0.5))
}

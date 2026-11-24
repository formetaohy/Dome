use dome_math::{Quat, Transform, Vec3};
use dome_render::{Camera, DirectionalLight, PointLight};
use dome_scene::{EnvironmentId, Node};
use dome_schema::{AudioListenerRef, EnvironmentRef, MeshRef, NodeComponents};

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

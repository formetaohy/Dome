use crate::project::ENTRY;
use dome_math::{Quat, Transform, Vec3};
use dome_render::{Camera, DirectionalLight, PointLight};
use dome_scene::{Folder, Node, Scene, Shape};
use dome_schema::{AudioBusRef, AudioListenerRef, MeshRef, NodeComponents, Registry};
use std::path::Path;
use std::sync::Arc;

pub const STARTER_SCRIPT: &str = r#"component Spin {
    speed: float = 1.0,
}

boot {
    let cube = find("cube");
    add(cube, Spin {});
}

system turn(spin: read Spin) {
    rotate_y(self, spin.speed * delta());
}
"#;

pub(crate) fn write(root: &Path) -> Result<(), String> {
    let mut scene = Scene::with_schema(root.to_path_buf(), Arc::new(Registry::default()));
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
            edit.insert_node(
                placed(
                    Node::new("lamp"),
                    Transform::from_translation(Vec3::new(0.0, 2.0, 0.0)),
                )
                .with(PointLight::new(Vec3::ONE, 20.0, 10.0)),
            )?;
            edit.insert_node(Node::new("cube").with(MeshRef::new("cube", "grey", Vec3::ONE)))?;
            Ok(())
        })
        .map_err(|error| format!("a new project builds its entry scene: {error}"))?;
    scene.save(root.join(Folder::Scene.name()).join(ENTRY))?;
    write_script(root)
}

fn write_script(root: &Path) -> Result<(), String> {
    let path = root.join(Folder::Script.name()).join("main.amp");
    std::fs::write(&path, STARTER_SCRIPT)
        .map_err(|error| format!("the project writes {}: {error}", path.display()))
}

fn placed(mut node: Node, transform: Transform) -> Node {
    node.set_transform(transform);
    node
}

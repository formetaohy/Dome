use dome_math::{Quat, Transform, Vec3};

fn close(left: Vec3, right: Vec3) -> bool {
    (left - right).length() < 1e-4
}

fn turned() -> Transform {
    Transform::from_scale_rotation_translation(
        Vec3::new(2.0, 1.0, 0.5),
        Quat::from_axis_angle(Vec3::Z, std::f32::consts::FRAC_PI_2),
        Vec3::new(3.0, -1.0, 2.0),
    )
}

fn local() -> Transform {
    Transform::from_scale_rotation_translation(
        Vec3::new(0.5, 2.0, 1.5),
        Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_3),
        Vec3::new(1.0, 2.0, 3.0),
    )
}

#[test]
fn a_world_transform_decomposes_into_the_local_transform_it_composed_from() {
    let parent = turned();
    let held = local();
    let decomposed = parent.decompose(parent.compose(held));
    assert!(
        close(decomposed.translation, held.translation),
        "a parent of non-uniform scale and rotation recovers the local translation it composed",
    );
    assert!(
        decomposed.rotation.dot(held.rotation).abs() > 0.999_99,
        "the recovered local rotation turns as the one it composed",
    );
    assert!(
        close(decomposed.scale, held.scale),
        "the recovered local scale spans what it composed",
    );
}

#[test]
fn a_local_point_survives_the_map_out_of_and_back_into_its_transform() {
    let transform = turned();
    let point = Vec3::new(-0.5, 4.0, 1.25);
    assert!(
        close(
            transform.inverse_transform_point(transform.transform_point(point)),
            point,
        ),
        "a point maps out of a non-uniformly scaled, turned transform and back",
    );
}

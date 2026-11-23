use super::*;

pub(super) fn joint_world(world: &World, entity: Entity) -> Mat4 {
    world
        .get::<Transform>(entity)
        .unwrap_or_else(|| {
            panic!(
                "a skin joint names entity {}, which holds no world transform",
                entity.index(),
            )
        })
        .matrix()
}

pub(super) fn static_bounds(geometry: &crate::mesh::Geometry, transform: Transform) -> (Vec3, f32) {
    let reach = transform
        .scale
        .abs()
        .x
        .max(transform.scale.abs().y)
        .max(transform.scale.abs().z);
    (
        transform.transform_point(geometry.center),
        geometry.radius * reach,
    )
}

pub(super) fn skinned_bounds(
    geometry: &crate::mesh::Geometry,
    transform: Transform,
    palette: &[Mat4],
) -> (Vec3, f32) {
    let mut low = Vec3::splat(f32::INFINITY);
    let mut high = Vec3::splat(f32::NEG_INFINITY);
    let mut spheres = Vec::with_capacity(palette.len());
    for matrix in palette {
        let center = matrix.transform_point(geometry.center);
        let radius = geometry.radius * matrix_reach(*matrix);
        low = low.min(center - Vec3::splat(radius));
        high = high.max(center + Vec3::splat(radius));
        spheres.push((center, radius));
    }
    let center = (low + high) * 0.5;
    let radius = spheres
        .iter()
        .map(|(held, reach)| held.distance(center) + reach)
        .fold(0.0, f32::max);
    let reach = transform
        .scale
        .abs()
        .x
        .max(transform.scale.abs().y)
        .max(transform.scale.abs().z);
    (transform.transform_point(center), radius * reach)
}

pub(super) fn matrix_reach(matrix: Mat4) -> f32 {
    let matrix = matrix.to_mat3();
    (matrix.col(0).length_squared()
        + matrix.col(1).length_squared()
        + matrix.col(2).length_squared())
    .sqrt()
}

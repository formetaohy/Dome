use crate::contact::{CONTACT_GAP, Manifold};
use crate::shape::Shape;
use crate::shape::capsule_segment;
use crate::solid::Solid;
use crate::triangle_contact::{self, Contact};
use dome_math::{Aabb, Vec3};

pub(crate) fn collide(first: &Solid, second: &Solid, gap: f32, contacts: &mut Vec<Manifold>) {
    match (&first.shape, &second.shape) {
        (Shape::Mesh { .. }, Shape::Mesh { .. }) => {}
        (Shape::Mesh { .. }, _) => mesh_convex(first, second, false, gap, contacts),
        (_, Shape::Mesh { .. }) => mesh_convex(second, first, true, gap, contacts),
        _ => panic!("a convex pair dispatches no mesh contact"),
    }
}

pub(crate) fn intersects(mesh: &Solid, other: &Solid) -> bool {
    if matches!(&other.shape, Shape::Mesh { .. }) {
        return false;
    }
    let mut contacts = Vec::new();
    mesh_convex(mesh, other, false, CONTACT_GAP, &mut contacts);
    contacts
        .iter()
        .any(|manifold| manifold.points.iter().any(|point| point.separation <= 0.0))
}

fn mesh_convex(mesh: &Solid, convex: &Solid, flip: bool, gap: f32, contacts: &mut Vec<Manifold>) {
    let Shape::Mesh { mesh: held, scale } = &mesh.shape else {
        panic!("a mesh contact holds a mesh collider")
    };
    assert!(
        *scale > 0.0 && scale.is_finite(),
        "a mesh collider of scale {scale} encloses no volume",
    );
    let rotation = mesh.rotation.inverse();
    let local = Solid::new(
        rotation.rotate_vec3(convex.position - mesh.position) / *scale,
        (rotation * convex.rotation).normalize(),
        convex.shape.scaled(1.0 / *scale),
    );
    let local_gap = gap / *scale;
    let margin = Vec3::splat(local_gap);
    let bounds = local.shape.aabb(local.position, local.rotation);
    let bounds = Aabb::new(bounds.min - margin, bounds.max + margin);
    for index in held.candidates(bounds) {
        let triangle = held.triangle(index);
        let Some(contact) = contact_of(&local, triangle, local_gap) else {
            continue;
        };
        let Contact {
            mut manifold,
            edges,
        } = contact;
        if let Some(feature) = held.feature(index, edges) {
            manifold.normal = if manifold.normal.dot(feature) >= 0.0 {
                feature
            } else {
                -feature
            };
        }
        for point in &mut manifold.points {
            point.position = mesh.position + mesh.rotation.rotate_vec3(point.position * *scale);
            point.separation *= *scale;
        }
        manifold.points.retain(|point| point.separation <= gap);
        if manifold.points.is_empty() {
            continue;
        }
        let normal = mesh.rotation.rotate_vec3(manifold.normal);
        manifold.normal = if flip { -normal } else { normal };
        for point in &mut manifold.points {
            point.feature |= (index as u64) << 32;
        }
        contacts.push(manifold);
    }
}

fn contact_of(local: &Solid, triangle: [Vec3; 3], gap: f32) -> Option<Contact> {
    match &local.shape {
        Shape::Sphere { radius } => {
            triangle_contact::sphere(triangle, local.position, *radius, gap)
        }
        Shape::Capsule {
            radius,
            half_height,
        } => {
            let (start, end) = capsule_segment(local.position, local.rotation, *half_height);
            triangle_contact::capsule(triangle, start, end, *radius, gap)
        }
        Shape::Cuboid { half_extents } => {
            let rotation = local.rotation.inverse();
            let held = triangle.map(|vertex| rotation.rotate_vec3(vertex - local.position));
            triangle_contact::cuboid(held, *half_extents, gap).map(|mut contact| {
                contact.manifold.normal = local.rotation.rotate_vec3(contact.manifold.normal);
                for point in &mut contact.manifold.points {
                    point.position = local.position + local.rotation.rotate_vec3(point.position);
                }
                contact
            })
        }
        Shape::Mesh { .. } => panic!("a mesh collider holds no mesh inside a mesh contact"),
    }
}

use crate::document::Node as NodeDef;
use dome_math::{Quat, Transform, Vec3};

pub struct Node {
    pub name: Option<String>,
    pub transform: Transform,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
    pub children: Vec<usize>,
}

pub(crate) fn build(document: &crate::document::Document, what: &str) -> Vec<Node> {
    document
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| Node {
            name: node.name.clone(),
            transform: transform(node, &format!("node {index} of {what}")),
            mesh: node.mesh,
            skin: node.skin,
            children: node.children.clone().unwrap_or_default(),
        })
        .collect()
}

fn transform(node: &NodeDef, what: &str) -> Transform {
    if let Some(matrix) = node.matrix {
        return decomposed(matrix, what);
    }
    let translation = Vec3::from_array(node.translation.unwrap_or([0.0; 3]));
    let rotation = node.rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let rotation = Quat::new(rotation[0], rotation[1], rotation[2], rotation[3]);
    let length = rotation.length();
    assert!(
        length > 0.0,
        "{what} holds the rotation {rotation:?} of no length",
    );
    let scale = Vec3::from_array(node.scale.unwrap_or([1.0; 3]));
    assert!(
        scale.x != 0.0 && scale.y != 0.0 && scale.z != 0.0,
        "{what} holds the scale {scale:?}",
    );
    Transform::from_scale_rotation_translation(scale, rotation.normalize(), translation)
}

fn decomposed(matrix: [f32; 16], what: &str) -> Transform {
    assert!(
        (matrix[3].abs() < 1e-6
            && matrix[7].abs() < 1e-6
            && matrix[11].abs() < 1e-6
            && (matrix[15] - 1.0).abs() < 1e-6),
        "{what} holds a matrix that is no affine transform",
    );
    let x = Vec3::new(matrix[0], matrix[1], matrix[2]);
    let y = Vec3::new(matrix[4], matrix[5], matrix[6]);
    let z = Vec3::new(matrix[8], matrix[9], matrix[10]);
    let mut sx = x.length();
    let sy = y.length();
    let sz = z.length();
    assert!(
        sx > 0.000001 && sy > 0.000001 && sz > 0.000001,
        "{what} holds a matrix of no scale",
    );
    if x.cross(y).dot(z) < 0.0 {
        sx = -sx;
    }
    let first = x * (1.0 / sx);
    let second = y * (1.0 / sy);
    let third = z * (1.0 / sz);
    assert!(
        first.dot(second).abs() < 1e-3
            && first.dot(third).abs() < 1e-3
            && second.dot(third).abs() < 1e-3,
        "{what} holds a matrix of shear",
    );
    Transform::from_scale_rotation_translation(
        Vec3::new(sx, sy, sz),
        rotated(first, second, third),
        Vec3::new(matrix[12], matrix[13], matrix[14]),
    )
}

fn rotated(first: Vec3, second: Vec3, third: Vec3) -> Quat {
    let m00 = first.x;
    let m10 = first.y;
    let m20 = first.z;
    let m01 = second.x;
    let m11 = second.y;
    let m21 = second.z;
    let m02 = third.x;
    let m12 = third.y;
    let m22 = third.z;
    let trace = m00 + m11 + m22;
    let held = if trace > 0.0 {
        let scale = (trace + 1.0).sqrt() * 2.0;
        Quat::new(
            (m21 - m12) / scale,
            (m02 - m20) / scale,
            (m10 - m01) / scale,
            0.25 * scale,
        )
    } else if m00 > m11 && m00 > m22 {
        let scale = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        Quat::new(
            0.25 * scale,
            (m01 + m10) / scale,
            (m02 + m20) / scale,
            (m21 - m12) / scale,
        )
    } else if m11 > m22 {
        let scale = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        Quat::new(
            (m01 + m10) / scale,
            0.25 * scale,
            (m12 + m21) / scale,
            (m02 - m20) / scale,
        )
    } else {
        let scale = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        Quat::new(
            (m02 + m20) / scale,
            (m12 + m21) / scale,
            0.25 * scale,
            (m10 - m01) / scale,
        )
    };
    held.normalize()
}

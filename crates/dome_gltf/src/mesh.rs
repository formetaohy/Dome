use crate::accessor::Store;
use crate::document::Primitive;
use crate::vertex::{SkinVertex, Vertex};
use dome_math::Vec3;

pub(crate) fn build(
    primitive: &Primitive,
    store: &Store<'_>,
    material_uses_image: bool,
    what: &str,
) -> (Vec<Vertex>, Option<Vec<SkinVertex>>) {
    assert_eq!(
        primitive.mode.unwrap_or(4),
        4,
        "{what} holds the primitive mode {}",
        primitive.mode.unwrap_or(4),
    );
    assert!(
        primitive.targets.as_ref().is_none_or(Vec::is_empty),
        "{what} holds morph targets",
    );
    assert!(
        !primitive.attributes.contains_key("JOINTS_1")
            && !primitive.attributes.contains_key("WEIGHTS_1"),
        "{what} holds more than four skin weights",
    );
    let joints = primitive
        .attributes
        .get("JOINTS_0")
        .map(|index| store.accessor(*index));
    let weights = primitive
        .attributes
        .get("WEIGHTS_0")
        .map(|index| store.accessor(*index));
    assert_eq!(
        joints.is_some(),
        weights.is_some(),
        "{what} holds skin joints without skin weights",
    );
    let positions = attribute(primitive, "POSITION", store, what);
    let normals = primitive
        .attributes
        .get("NORMAL")
        .map(|index| store.accessor(*index));
    let uvs = primitive
        .attributes
        .get("TEXCOORD_0")
        .map(|index| store.accessor(*index));
    assert!(
        !material_uses_image || uvs.is_some(),
        "{what} samples a texture without texture coordinates",
    );
    let indices = primitive.indices.map(|index| store.accessor(index));
    let count = indices
        .as_ref()
        .map_or(positions.count(), |held| held.count());
    assert!(
        count > 0 && count.is_multiple_of(3),
        "{what} holds {count} triangle indices",
    );
    if let (Some(joints), Some(weights)) = (&joints, &weights) {
        assert_eq!(
            joints.count(),
            positions.count(),
            "{what} holds {} skin joints for {} positions",
            joints.count(),
            positions.count(),
        );
        assert_eq!(
            weights.count(),
            positions.count(),
            "{what} holds {} skin weights for {} positions",
            weights.count(),
            positions.count(),
        );
    }
    let mut vertices = Vec::with_capacity(count);
    let mut skins = joints.as_ref().map(|_| Vec::with_capacity(count));
    for at in 0..count {
        let index = indices.as_ref().map_or(at as u32, |held| held.index(at));
        let index = index as usize;
        assert!(
            index < positions.count(),
            "{what} indexes the position {index} beyond {}",
            positions.count(),
        );
        let position = positions.vector3(index);
        let normal = normals
            .as_ref()
            .map_or([0.0, 0.0, 1.0], |held| held.vector3(index));
        let uv = uvs.as_ref().map_or([0.0, 0.0], |held| held.vector2(index));
        assert!(
            position.iter().all(|value| value.is_finite())
                && normal.iter().all(|value| value.is_finite())
                && uv.iter().all(|value| value.is_finite()),
            "{what} holds a vertex of no finite number",
        );
        vertices.push(Vertex {
            position,
            normal,
            uv,
        });
        if let (Some(joints), Some(weights)) = (&joints, &weights) {
            let mut joint = joints.joints(index);
            let mut weight = weights.vector4(index);
            assert!(
                weight
                    .iter()
                    .all(|value| value.is_finite() && *value >= 0.0),
                "{what} holds the skin weights {weight:?}",
            );
            let total = weight.iter().sum::<f32>();
            assert!(total > 0.0, "{what} holds a vertex of no skin weight",);
            for at in 0..4 {
                if weight[at] == 0.0 {
                    joint[at] = 0;
                }
                weight[at] /= total;
            }
            skins
                .as_mut()
                .expect("skin weights collect beside their joints")
                .push(SkinVertex::new(joint, weight));
        }
    }
    if normals.is_none() {
        for triangle in vertices.as_chunks_mut::<3>().0 {
            let normal = face(triangle);
            for vertex in triangle {
                vertex.normal = normal;
            }
        }
    } else {
        for triangle in vertices.as_chunks_mut::<3>().0 {
            let fallback = face(triangle);
            for vertex in triangle {
                vertex.normal = normalized(vertex.normal).unwrap_or(fallback);
            }
        }
    }
    (vertices, skins)
}

fn attribute<'a>(
    primitive: &Primitive,
    name: &str,
    store: &'a Store<'_>,
    what: &str,
) -> crate::accessor::Accessor<'a> {
    let index = primitive
        .attributes
        .get(name)
        .unwrap_or_else(|| panic!("{what} holds no {name} attribute"));
    store.accessor(*index)
}

fn face(triangle: &[Vertex]) -> [f32; 3] {
    let first = Vec3::from_array(triangle[0].position);
    let second = Vec3::from_array(triangle[1].position);
    let third = Vec3::from_array(triangle[2].position);
    normalized((second - first).cross(third - first).to_array()).unwrap_or([0.0, 0.0, 1.0])
}

fn normalized(vector: [f32; 3]) -> Option<[f32; 3]> {
    let held = Vec3::from_array(vector);
    let length = held.length();
    if length > 0.000001 {
        Some((held * (1.0 / length)).to_array())
    } else {
        None
    }
}

use crate::accessor::Store;
use crate::document::Primitive;
use crate::vertex::{SkinVertex, Vertex};
use dome_math::Vec3;

pub(crate) fn build(
    primitive: &Primitive,
    store: &Store<'_>,
    material_uses_image: bool,
    what: &str,
) -> Result<(Vec<Vertex>, Option<Vec<SkinVertex>>), String> {
    if primitive.mode.unwrap_or(4) != 4 {
        return Err(format!(
            "{what} holds the primitive mode {}",
            primitive.mode.unwrap_or(4),
        ));
    }
    if primitive
        .targets
        .as_ref()
        .is_some_and(|targets| !targets.is_empty())
    {
        return Err(format!("{what} holds morph targets"));
    }
    if primitive.attributes.contains_key("JOINTS_1")
        || primitive.attributes.contains_key("WEIGHTS_1")
    {
        return Err(format!("{what} holds more than four skin weights"));
    }
    let joints = primitive
        .attributes
        .get("JOINTS_0")
        .map(|index| store.accessor(*index))
        .transpose()?;
    let weights = primitive
        .attributes
        .get("WEIGHTS_0")
        .map(|index| store.accessor(*index))
        .transpose()?;
    if joints.is_some() != weights.is_some() {
        return Err(format!("{what} holds skin joints without skin weights"));
    }
    let positions = attribute(primitive, "POSITION", store, what)?;
    let normals = primitive
        .attributes
        .get("NORMAL")
        .map(|index| store.accessor(*index))
        .transpose()?;
    let uvs = primitive
        .attributes
        .get("TEXCOORD_0")
        .map(|index| store.accessor(*index))
        .transpose()?;
    if material_uses_image && uvs.is_none() {
        return Err(format!(
            "{what} samples a texture without texture coordinates"
        ));
    }
    let indices = primitive
        .indices
        .map(|index| store.accessor(index))
        .transpose()?;
    let count = indices
        .as_ref()
        .map_or(positions.count(), |held| held.count());
    if count == 0 || !count.is_multiple_of(3) {
        return Err(format!("{what} holds {count} triangle indices"));
    }
    if let (Some(joints), Some(weights)) = (&joints, &weights) {
        if joints.count() != positions.count() {
            return Err(format!(
                "{what} holds {} skin joints for {} positions",
                joints.count(),
                positions.count(),
            ));
        }
        if weights.count() != positions.count() {
            return Err(format!(
                "{what} holds {} skin weights for {} positions",
                weights.count(),
                positions.count(),
            ));
        }
    }
    let mut vertices = Vec::with_capacity(count);
    let mut skins = joints.as_ref().map(|_| Vec::with_capacity(count));
    for at in 0..count {
        let index = match indices.as_ref() {
            Some(indices) => indices.index(at)?,
            None => at as u32,
        } as usize;
        if index >= positions.count() {
            return Err(format!(
                "{what} indexes the position {index} beyond {}",
                positions.count(),
            ));
        }
        let position = positions.vector3(index)?;
        let normal = match normals.as_ref() {
            Some(normals) => normals.vector3(index)?,
            None => [0.0, 0.0, 1.0],
        };
        let uv = match uvs.as_ref() {
            Some(uvs) => uvs.vector2(index)?,
            None => [0.0, 0.0],
        };
        if !position.iter().all(|value| value.is_finite())
            || !normal.iter().all(|value| value.is_finite())
            || !uv.iter().all(|value| value.is_finite())
        {
            return Err(format!("{what} holds a vertex of no finite number"));
        }
        vertices.push(Vertex {
            position,
            normal,
            uv,
        });
        if let (Some(joints), Some(weights)) = (&joints, &weights) {
            let mut joint = joints.joints(index)?;
            let mut weight = weights.vector4(index)?;
            if !weight
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0)
            {
                return Err(format!("{what} holds the skin weights {weight:?}"));
            }
            let total = weight.iter().sum::<f32>();
            if total <= 0.0 {
                return Err(format!("{what} holds a vertex of no skin weight"));
            }
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
    Ok((vertices, skins))
}

fn attribute<'a>(
    primitive: &Primitive,
    name: &str,
    store: &'a Store<'_>,
    what: &str,
) -> Result<crate::accessor::Accessor<'a>, String> {
    let index = primitive
        .attributes
        .get(name)
        .ok_or_else(|| format!("{what} holds no {name} attribute"))?;
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

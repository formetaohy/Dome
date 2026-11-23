use crate::reference::ModelId;
use crate::shape::Shape;
use dome_render::Vertex;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct MeshAsset {
    pub(crate) geometry: Geometry,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    Shape(Shape),
    Vertices(Arc<[Vertex]>),
    Model {
        model: ModelId,
        mesh: u32,
        primitive: u32,
    },
}

impl MeshAsset {
    pub fn shape(shape: Shape) -> Self {
        Self {
            geometry: Geometry::Shape(shape),
        }
    }

    pub fn vertices(vertices: Vec<Vertex>) -> Self {
        Self {
            geometry: Geometry::Vertices(Arc::from(vertices)),
        }
    }

    pub fn model(model: ModelId, mesh: u32, primitive: u32) -> Self {
        Self {
            geometry: Geometry::Model {
                model,
                mesh,
                primitive,
            },
        }
    }

    pub fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    pub fn shape_of(&self) -> Option<Shape> {
        match &self.geometry {
            Geometry::Shape(shape) => Some(*shape),
            Geometry::Vertices(_) | Geometry::Model { .. } => None,
        }
    }

    pub fn model_of(&self) -> Option<(&ModelId, u32, u32)> {
        match &self.geometry {
            Geometry::Model {
                model,
                mesh,
                primitive,
            } => Some((model, *mesh, *primitive)),
            Geometry::Shape(_) | Geometry::Vertices(_) => None,
        }
    }

    pub fn is_imported(&self) -> bool {
        matches!(self.geometry, Geometry::Model { .. })
    }

    pub fn is_empty(&self) -> bool {
        match &self.geometry {
            Geometry::Shape(_) | Geometry::Model { .. } => false,
            Geometry::Vertices(vertices) => vertices.is_empty(),
        }
    }

    pub fn vertex_count(&self) -> usize {
        match &self.geometry {
            Geometry::Shape(_) | Geometry::Model { .. } => 0,
            Geometry::Vertices(vertices) => vertices.len(),
        }
    }

    pub fn vertices_of(&self) -> Vec<Vertex> {
        match &self.geometry {
            Geometry::Shape(shape) => shape.vertices(),
            Geometry::Vertices(vertices) => vertices.to_vec(),
            Geometry::Model { .. } => {
                panic!("an imported mesh reads its vertices from its model")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshEntry {
    pub id: String,
    pub asset: MeshAsset,
}

impl MeshEntry {
    pub fn new(id: impl Into<String>, asset: MeshAsset) -> Self {
        Self {
            id: id.into(),
            asset,
        }
    }
}

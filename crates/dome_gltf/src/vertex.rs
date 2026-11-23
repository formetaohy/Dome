#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinVertex {
    pub joints: [u32; 4],
    pub weights: [f32; 4],
}

impl SkinVertex {
    pub fn new(joints: [u32; 4], weights: [f32; 4]) -> Self {
        Self { joints, weights }
    }
}

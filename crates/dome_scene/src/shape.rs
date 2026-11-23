use dome_render::Vertex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Plane,
    Cube,
    Sphere,
}

const CUBE_FACES: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
    ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
    ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
    ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
];

const SEGMENTS: u32 = 32;
const RINGS: u32 = 16;

impl Shape {
    pub(crate) fn of(name: &str, what: &str) -> Self {
        Self::from_name(name).unwrap_or_else(|| panic!("{what} holds the shape {name}"))
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "plane" => Some(Self::Plane),
            "cube" => Some(Self::Cube),
            "sphere" => Some(Self::Sphere),
            _ => None,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Plane => "plane",
            Self::Cube => "cube",
            Self::Sphere => "sphere",
        }
    }

    pub(crate) fn vertices(self) -> Vec<Vertex> {
        match self {
            Self::Plane => plane(),
            Self::Cube => cube(),
            Self::Sphere => sphere(),
        }
    }
}

fn plane() -> Vec<Vertex> {
    let corner = |x: f32, z: f32, u: f32, v: f32| Vertex {
        position: [x, 0.0, z],
        normal: [0.0, 1.0, 0.0],
        uv: [u, v],
    };
    let first = corner(-0.5, -0.5, 0.0, 0.0);
    let second = corner(-0.5, 0.5, 0.0, 1.0);
    let third = corner(0.5, 0.5, 1.0, 1.0);
    let fourth = corner(0.5, -0.5, 1.0, 0.0);
    vec![first, second, third, first, third, fourth]
}

fn cube() -> Vec<Vertex> {
    let mut vertices = Vec::with_capacity(CUBE_FACES.len() * 6);
    for (normal, right, up) in CUBE_FACES {
        let corner = |side_right: f32, side_up: f32| Vertex {
            position: [
                normal[0] * 0.5 + right[0] * side_right * 0.5 + up[0] * side_up * 0.5,
                normal[1] * 0.5 + right[1] * side_right * 0.5 + up[1] * side_up * 0.5,
                normal[2] * 0.5 + right[2] * side_right * 0.5 + up[2] * side_up * 0.5,
            ],
            normal,
            uv: [(side_right + 1.0) * 0.5, 0.5 - side_up * 0.5],
        };
        let first = corner(-1.0, -1.0);
        let second = corner(1.0, -1.0);
        let third = corner(1.0, 1.0);
        let fourth = corner(-1.0, 1.0);
        vertices.extend([first, second, third, first, third, fourth]);
    }
    vertices
}

fn sphere() -> Vec<Vertex> {
    let point = |theta: f32, phi: f32| {
        let (sin_phi, cos_phi) = phi.sin_cos();
        let (sin_theta, cos_theta) = theta.sin_cos();
        [
            sin_phi * cos_theta * 0.5,
            cos_phi * 0.5,
            sin_phi * sin_theta * 0.5,
        ]
    };
    let vertex = |theta: f32, phi: f32| {
        let position = point(theta, phi);
        let normal = [position[0] * 2.0, position[1] * 2.0, position[2] * 2.0];
        Vertex {
            position,
            normal,
            uv: [theta / std::f32::consts::TAU, phi / std::f32::consts::PI],
        }
    };
    let theta = |segment: u32| segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
    let phi = |ring: u32| ring as f32 / RINGS as f32 * std::f32::consts::PI;
    let mut vertices = Vec::with_capacity((SEGMENTS * RINGS * 6) as usize);
    for ring in 0..RINGS {
        for segment in 0..SEGMENTS {
            let first = vertex(theta(segment), phi(ring));
            let second = vertex(theta(segment + 1), phi(ring));
            let third = vertex(theta(segment + 1), phi(ring + 1));
            let fourth = vertex(theta(segment), phi(ring + 1));
            vertices.extend([first, second, third, first, third, fourth]);
        }
    }
    vertices
}

use dome_math::{Aabb, Vec3};
use std::collections::{HashMap, HashSet};

const LEAF_TRIANGLES: usize = 4;
const DEGENERATE_AREA: f32 = 1e-12;
const PARALLEL_RAY: f32 = 1e-12;
const SMOOTH_EDGE: f32 = 0.984_807_7;

type EdgeKey = ((u32, u32, u32), (u32, u32, u32));

#[derive(Clone, Debug)]
pub struct TriangleMesh {
    triangles: Vec<Triangle>,
    edges: Vec<[Vec3; 2]>,
    features: Vec<[Option<Vec3>; 3]>,
    nodes: Vec<Node>,
    order: Vec<u32>,
    bounds: Aabb,
}

#[derive(Clone, Debug)]
struct Triangle {
    vertices: [Vec3; 3],
    bounds: Aabb,
}

#[derive(Clone, Debug)]
struct Node {
    bounds: Aabb,
    first: u32,
    count: u32,
    left: u32,
    right: u32,
}

impl TriangleMesh {
    pub fn from_positions(positions: &[[f32; 3]]) -> Self {
        assert!(
            !positions.is_empty() && positions.len().is_multiple_of(3),
            "a triangle mesh of {} positions holds no triangle",
            positions.len(),
        );
        let mut triangles = Vec::with_capacity(positions.len() / 3);
        for vertices in positions.as_chunks::<3>().0 {
            let vertices = [
                Vec3::from_array(vertices[0]),
                Vec3::from_array(vertices[1]),
                Vec3::from_array(vertices[2]),
            ];
            for vertex in vertices {
                assert!(
                    vertex.x.is_finite() && vertex.y.is_finite() && vertex.z.is_finite(),
                    "a triangle mesh holds the vertex {vertex:?}",
                );
            }
            let normal = (vertices[1] - vertices[0]).cross(vertices[2] - vertices[0]);
            if normal.length_squared() <= DEGENERATE_AREA {
                continue;
            }
            triangles.push(Triangle {
                vertices,
                bounds: Aabb::from_points(&vertices),
            });
        }
        assert!(
            !triangles.is_empty(),
            "a triangle mesh of {} positions holds no triangle of area",
            positions.len(),
        );
        let bounds = triangles
            .iter()
            .map(|triangle| triangle.bounds)
            .reduce(Aabb::union)
            .expect("a triangle mesh holds a triangle");
        let internal = feature_normals(&triangles);
        let edges = unique_edges(&triangles);
        let mut order = (0..triangles.len() as u32).collect::<Vec<_>>();
        let mut nodes = Vec::with_capacity(triangles.len() * 2);
        build(&mut nodes, &triangles, &mut order, 0);
        Self {
            triangles,
            edges,
            features: internal,
            nodes,
            order,
            bounds,
        }
    }

    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    pub fn bounds(&self) -> Aabb {
        self.bounds
    }

    pub(crate) fn edges(&self) -> &[[Vec3; 2]] {
        &self.edges
    }

    pub(crate) fn triangle(&self, index: u32) -> [Vec3; 3] {
        self.triangles[index as usize].vertices
    }

    pub(crate) fn feature(&self, index: u32, edges: u8) -> Option<Vec3> {
        let held = &self.features[index as usize];
        let mut sum = Vec3::ZERO;
        for (edge, &normal) in held.iter().enumerate() {
            if let Some(normal) = normal
                && edges & (1 << edge) != 0
            {
                sum += normal;
            }
        }
        (sum.length_squared() > 0.0).then(|| sum.normalize())
    }

    pub(crate) fn candidates(&self, bounds: Aabb) -> Vec<u32> {
        let mut found = Vec::new();
        if self.bounds.intersects(bounds) {
            self.collect(0, bounds, &mut found);
        }
        found.sort_unstable();
        found.dedup();
        found
    }

    pub(crate) fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(f32, Vec3)> {
        let mut best: Option<(f32, Vec3)> = None;
        self.trace(0, origin, direction, max_distance, &mut best);
        best
    }

    fn collect(&self, at: u32, bounds: Aabb, found: &mut Vec<u32>) {
        let node = &self.nodes[at as usize];
        if !node.bounds.intersects(bounds) {
            return;
        }
        if node.count > 0 {
            let first = node.first as usize;
            let count = node.count as usize;
            found.extend_from_slice(&self.order[first..first + count]);
            return;
        }
        self.collect(node.left, bounds, found);
        self.collect(node.right, bounds, found);
    }

    fn trace(
        &self,
        at: u32,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        best: &mut Option<(f32, Vec3)>,
    ) {
        let node = &self.nodes[at as usize];
        let reach = best.map_or(max_distance, |(distance, _)| distance.min(max_distance));
        if !reached(node.bounds, origin, direction, reach) {
            return;
        }
        if node.count > 0 {
            let first = node.first as usize;
            let count = node.count as usize;
            for &index in &self.order[first..first + count] {
                let triangle = &self.triangles[index as usize];
                let Some(distance) = cross(triangle, origin, direction, reach) else {
                    continue;
                };
                if best.is_some_and(|(held, _)| held <= distance) {
                    continue;
                }
                let normal = face(triangle.vertices);
                let normal = if normal.dot(direction) > 0.0 {
                    -normal
                } else {
                    normal
                };
                *best = Some((distance, normal));
            }
            return;
        }
        self.trace(node.left, origin, direction, max_distance, best);
        self.trace(node.right, origin, direction, max_distance, best);
    }
}

fn feature_normals(triangles: &[Triangle]) -> Vec<[Option<Vec3>; 3]> {
    let mut shared = HashMap::<EdgeKey, (u32, Vec3)>::with_capacity(triangles.len() * 2);
    for triangle in triangles {
        let normal = area_normal(triangle.vertices);
        for edge in 0..3 {
            let key = edge_key(triangle.vertices[edge], triangle.vertices[(edge + 1) % 3]);
            shared
                .entry(key)
                .and_modify(|(count, held)| {
                    *count += 1;
                    *held += normal;
                })
                .or_insert((1, normal));
        }
    }
    triangles
        .iter()
        .map(|triangle| {
            let normal = face(triangle.vertices);
            std::array::from_fn(|edge| {
                let key = edge_key(triangle.vertices[edge], triangle.vertices[(edge + 1) % 3]);
                let (count, held) = shared[&key];
                (count > 1 && normal.dot(held.normalize()) > SMOOTH_EDGE).then(|| held.normalize())
            })
        })
        .collect()
}

fn edge_key(first: Vec3, second: Vec3) -> EdgeKey {
    let first = bits(first);
    let second = bits(second);
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

fn unique_edges(triangles: &[Triangle]) -> Vec<[Vec3; 2]> {
    let mut seen = HashSet::<EdgeKey>::new();
    let mut edges = Vec::new();
    for triangle in triangles {
        for edge in 0..3 {
            let first = triangle.vertices[edge];
            let second = triangle.vertices[(edge + 1) % 3];
            if seen.insert(edge_key(first, second)) {
                edges.push([first, second]);
            }
        }
    }
    edges
}

fn bits(vector: Vec3) -> (u32, u32, u32) {
    (vector.x.to_bits(), vector.y.to_bits(), vector.z.to_bits())
}

fn build(nodes: &mut Vec<Node>, triangles: &[Triangle], order: &mut [u32], offset: u32) -> u32 {
    let bounds = order
        .iter()
        .map(|index| triangles[*index as usize].bounds)
        .reduce(Aabb::union)
        .expect("a branch of a triangle mesh holds a triangle");
    let at = nodes.len() as u32;
    nodes.push(Node {
        bounds,
        first: 0,
        count: 0,
        left: 0,
        right: 0,
    });
    if order.len() <= LEAF_TRIANGLES {
        nodes[at as usize] = Node {
            bounds,
            first: offset,
            count: order.len() as u32,
            left: 0,
            right: 0,
        };
        return at;
    }
    let centers = order
        .iter()
        .map(|index| triangles[*index as usize].bounds.center())
        .collect::<Vec<_>>();
    let extent = centers
        .iter()
        .fold(Vec3::splat(f32::NEG_INFINITY), |held, center| {
            held.max(*center)
        })
        - centers
            .iter()
            .fold(Vec3::splat(f32::INFINITY), |held, center| held.min(*center));
    let axis = if extent.x >= extent.y && extent.x >= extent.z {
        0
    } else if extent.y >= extent.z {
        1
    } else {
        2
    };
    order.sort_by(|left, right| {
        let left = triangles[*left as usize].bounds.center().to_array()[axis];
        let right = triangles[*right as usize].bounds.center().to_array()[axis];
        left.total_cmp(&right)
    });
    let middle = order.len() / 2;
    let (low, high) = order.split_at_mut(middle);
    let left = build(nodes, triangles, low, offset);
    let right = build(nodes, triangles, high, offset + middle as u32);
    nodes[at as usize].left = left;
    nodes[at as usize].right = right;
    at
}

fn reached(bounds: Aabb, origin: Vec3, direction: Vec3, reach: f32) -> bool {
    let mut near = 0.0f32;
    let mut far = reach;
    for axis in 0..3 {
        let origin = origin.to_array()[axis];
        let direction = direction.to_array()[axis];
        let low = bounds.min.to_array()[axis];
        let high = bounds.max.to_array()[axis];
        if direction.abs() <= PARALLEL_RAY {
            if origin < low || origin > high {
                return false;
            }
            continue;
        }
        let first = (low - origin) / direction;
        let second = (high - origin) / direction;
        let (first, second) = if first <= second {
            (first, second)
        } else {
            (second, first)
        };
        near = near.max(first);
        far = far.min(second);
        if near > far {
            return false;
        }
    }
    far >= 0.0
}

fn face(vertices: [Vec3; 3]) -> Vec3 {
    area_normal(vertices).normalize()
}

fn area_normal(vertices: [Vec3; 3]) -> Vec3 {
    (vertices[1] - vertices[0]).cross(vertices[2] - vertices[0])
}

fn cross(triangle: &Triangle, origin: Vec3, direction: Vec3, reach: f32) -> Option<f32> {
    let [first, second, third] = triangle.vertices;
    let edge_first = second - first;
    let edge_second = third - first;
    let p = direction.cross(edge_second);
    let determinant = edge_first.dot(p);
    if determinant.abs() <= PARALLEL_RAY {
        return None;
    }
    let inverse = 1.0 / determinant;
    let offset = origin - first;
    let u = offset.dot(p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = offset.cross(edge_first);
    let v = direction.dot(q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = edge_second.dot(q) * inverse;
    (0.0..=reach).contains(&distance).then_some(distance)
}

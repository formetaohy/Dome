use dome_math::{Aabb, Vec3};

const LEAF: u32 = u32::MAX;

struct Node {
    aabb: Aabb,
    left: u32,
    right: u32,
    body: u32,
}

pub(crate) fn pairs(boxes: &[Aabb]) -> Vec<(usize, usize)> {
    if boxes.len() < 2 {
        return Vec::new();
    }
    let mut indices = (0..boxes.len()).collect::<Vec<_>>();
    let mut nodes = Vec::with_capacity(boxes.len() * 2);
    let root = build(&mut nodes, boxes, &mut indices);
    let mut found = Vec::new();
    for (index, &aabb) in boxes.iter().enumerate() {
        query(&nodes, root, aabb, index, &mut found);
    }
    found.sort_unstable();
    found.dedup();
    found
}

fn build(nodes: &mut Vec<Node>, boxes: &[Aabb], indices: &mut [usize]) -> u32 {
    let aabb = indices
        .iter()
        .map(|index| boxes[*index])
        .reduce(Aabb::union)
        .expect("a branch of the broadphase spans at least one collider");
    let at = nodes.len() as u32;
    nodes.push(Node {
        aabb,
        left: LEAF,
        right: LEAF,
        body: LEAF,
    });
    if indices.len() == 1 {
        nodes[at as usize].body = indices[0] as u32;
        return at;
    }
    let centers = indices
        .iter()
        .map(|index| boxes[*index].center())
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
    indices.sort_by(|left, right| {
        let left = boxes[*left].center().to_array()[axis];
        let right = boxes[*right].center().to_array()[axis];
        left.total_cmp(&right)
    });
    let middle = indices.len() / 2;
    let (low, high) = indices.split_at_mut(middle);
    let left = build(nodes, boxes, low);
    let right = build(nodes, boxes, high);
    nodes[at as usize].left = left;
    nodes[at as usize].right = right;
    at
}

fn query(nodes: &[Node], at: u32, target: Aabb, body: usize, found: &mut Vec<(usize, usize)>) {
    let node = &nodes[at as usize];
    if !node.aabb.intersects(target) {
        return;
    }
    if node.body != LEAF {
        if node.body as usize > body {
            found.push((body, node.body as usize));
        }
        return;
    }
    query(nodes, node.left, target, body, found);
    query(nodes, node.right, target, body, found);
}

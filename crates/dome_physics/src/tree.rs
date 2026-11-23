use dome_math::{Aabb, Vec3};

const NONE: usize = usize::MAX;
const FAT: f32 = 0.04;

#[derive(Clone, Copy)]
struct Node {
    aabb: Aabb,
    parent: usize,
    left: usize,
    right: usize,
    height: usize,
}

pub(crate) struct Tree {
    nodes: Vec<Node>,
    free: Vec<usize>,
    root: usize,
    height: usize,
}

impl Tree {
    pub(crate) fn new() -> Self {
        Self {
            nodes: Vec::new(),
            free: Vec::new(),
            root: NONE,
            height: 0,
        }
    }

    pub(crate) fn insert(&mut self, aabb: Aabb, proxy: u32) -> usize {
        let leaf = self.allocate(Node {
            aabb: fat(aabb),
            parent: NONE,
            left: proxy as usize,
            right: NONE,
            height: 0,
        });
        if self.root == NONE {
            self.root = leaf;
            self.height = 0;
            return leaf;
        }
        let sibling = self.sibling(self.root, aabb);
        let above = self.nodes[sibling].parent;
        let joined = self.allocate(Node {
            aabb: self.nodes[leaf].aabb.union(self.nodes[sibling].aabb),
            parent: above,
            left: leaf,
            right: sibling,
            height: self.nodes[sibling].height + 1,
        });
        self.nodes[leaf].parent = joined;
        self.nodes[sibling].parent = joined;
        match above {
            NONE => self.root = joined,
            at => {
                if self.nodes[at].left == sibling {
                    self.nodes[at].left = joined;
                } else {
                    self.nodes[at].right = joined;
                }
            }
        }
        self.climb(joined);
        leaf
    }

    pub(crate) fn remove(&mut self, leaf: usize) {
        if leaf == self.root {
            self.root = NONE;
            self.release(leaf);
            self.height = 0;
            return;
        }
        let parent = self.nodes[leaf].parent;
        let above = self.nodes[parent].parent;
        let sibling = self.other(parent, leaf);
        match above {
            NONE => {
                self.root = sibling;
                self.nodes[sibling].parent = NONE;
            }
            at => {
                if self.nodes[at].left == parent {
                    self.nodes[at].left = sibling;
                } else {
                    self.nodes[at].right = sibling;
                }
                self.nodes[sibling].parent = at;
            }
        }
        self.release(parent);
        self.release(leaf);
        self.climb(sibling);
    }

    pub(crate) fn reindex(&mut self, leaf: usize, proxy: u32) {
        self.nodes[leaf].left = proxy as usize;
    }

    pub(crate) fn refresh(&mut self, leaf: usize, aabb: Aabb) -> bool {
        if covers(self.nodes[leaf].aabb, aabb) {
            return false;
        }
        self.nodes[leaf].aabb = fat(aabb);
        self.climb(leaf);
        true
    }

    pub(crate) fn query(&self, aabb: Aabb, found: &mut Vec<u32>, stack: &mut Vec<usize>) {
        found.clear();
        stack.clear();
        if self.root != NONE {
            stack.push(self.root);
        }
        while let Some(at) = stack.pop() {
            let node = self.nodes[at];
            if !node.aabb.intersects(aabb) {
                continue;
            }
            if node.height == 0 {
                found.push(node.left as u32);
                continue;
            }
            stack.push(node.left);
            stack.push(node.right);
        }
    }

    fn sibling(&self, from: usize, aabb: Aabb) -> usize {
        let center = aabb.center();
        let mut at = from;
        while self.nodes[at].height != 0 {
            let node = self.nodes[at];
            let left = self.nodes[node.left];
            let right = self.nodes[node.right];
            let left_growth = grown(left.aabb, center);
            let right_growth = grown(right.aabb, center);
            at = if left_growth <= right_growth {
                node.left
            } else {
                node.right
            };
        }
        at
    }

    fn climb(&mut self, mut at: usize) {
        while at != NONE {
            let settled = self.settle(at);
            let balanced = self.balance(settled);
            at = self.nodes[balanced].parent;
        }
        self.height = match self.root {
            NONE => 0,
            root => self.nodes[root].height,
        };
    }

    fn settle(&mut self, at: usize) -> usize {
        let node = self.nodes[at];
        if node.height == 0 {
            return at;
        }
        let left = self.nodes[node.left];
        let right = self.nodes[node.right];
        self.nodes[at].aabb = left.aabb.union(right.aabb);
        self.nodes[at].height = 1 + left.height.max(right.height);
        at
    }

    fn balance(&mut self, at: usize) -> usize {
        let node = self.nodes[at];
        if node.height < 2 {
            return at;
        }
        let left = self.nodes[node.left];
        let right = self.nodes[node.right];
        if left.height > right.height + 1 {
            return self.raise(at, node.left, node.right);
        }
        if right.height > left.height + 1 {
            return self.raise(at, node.right, node.left);
        }
        at
    }

    fn raise(&mut self, low: usize, high: usize, light: usize) -> usize {
        let node = self.nodes[low];
        let raised = self.nodes[high];
        let first = self.nodes[raised.left];
        let second = self.nodes[raised.right];
        let (carried, kept) = if first.height >= second.height {
            (raised.left, raised.right)
        } else {
            (raised.right, raised.left)
        };
        self.nodes[high].left = low;
        self.nodes[high].right = carried;
        self.nodes[high].parent = node.parent;
        self.nodes[carried].parent = high;
        self.nodes[low].left = light;
        self.nodes[low].right = kept;
        self.nodes[kept].parent = low;
        self.nodes[low].parent = high;
        match node.parent {
            NONE => self.root = high,
            above => {
                if self.nodes[above].left == low {
                    self.nodes[above].left = high;
                } else {
                    self.nodes[above].right = high;
                }
            }
        }
        self.settle(low);
        self.settle(high);
        high
    }

    fn other(&self, parent: usize, child: usize) -> usize {
        let node = self.nodes[parent];
        if node.left == child {
            node.right
        } else {
            node.left
        }
    }

    fn allocate(&mut self, node: Node) -> usize {
        match self.free.pop() {
            Some(at) => {
                self.nodes[at] = node;
                at
            }
            None => {
                self.nodes.push(node);
                self.nodes.len() - 1
            }
        }
    }

    fn release(&mut self, at: usize) {
        self.nodes[at] = Node {
            aabb: Aabb::new(Vec3::ZERO, Vec3::ZERO),
            parent: NONE,
            left: NONE,
            right: NONE,
            height: 0,
        };
        self.free.push(at);
    }
}

fn fat(aabb: Aabb) -> Aabb {
    Aabb::new(aabb.min - Vec3::splat(FAT), aabb.max + Vec3::splat(FAT))
}

fn covers(outer: Aabb, inner: Aabb) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && outer.min.z <= inner.min.z
        && outer.max.x >= inner.max.x
        && outer.max.y >= inner.max.y
        && outer.max.z >= inner.max.z
}

fn grown(aabb: Aabb, center: Vec3) -> f32 {
    aabb.union(Aabb::new(center, center))
        .half_extents()
        .length()
        - aabb.half_extents().length()
}

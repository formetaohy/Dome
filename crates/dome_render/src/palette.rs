use dome_ecs::Entity;
use dome_gpu::{BufferBinding, BufferUsages, Device, GpuBuffer, Queue, Submission};
use dome_math::Mat4;
use std::collections::{HashMap, HashSet};

const JOINT_BYTES: u64 = std::mem::size_of::<Mat4>() as u64;
const POOL_BYTES: u64 = 4096;
const HALVES: u64 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Skinning {
    pub(crate) palette: u32,
    pub(crate) joints: u32,
}

pub(crate) const NONE: Skinning = Skinning {
    palette: 0,
    joints: 0,
};

#[derive(Clone, Copy)]
struct Range {
    palette: u32,
    joints: u32,
}
struct Staged {
    range: Range,
    first: usize,
    joints: usize,
    fresh: bool,
}

pub(crate) struct Palettes {
    placed: HashMap<Entity, Range>,
    free: Vec<Range>,
    matrices: Vec<Mat4>,
    staged: Vec<Staged>,
    posed_entities: HashSet<Entity>,
    pool: Option<GpuBuffer>,
    joints: u32,
    used: u32,
    at: u64,
    posed: u32,
}

impl Palettes {
    pub(crate) fn new() -> Self {
        Self {
            placed: HashMap::new(),
            free: Vec::new(),
            matrices: Vec::new(),
            staged: Vec::new(),
            posed_entities: HashSet::new(),
            pool: None,
            joints: 0,
            used: 0,
            at: 0,
            posed: 0,
        }
    }

    pub(crate) fn begin(&mut self) {
        self.matrices.clear();
        self.staged.clear();
        self.posed_entities.clear();
    }

    pub(crate) fn push(&mut self, entity: Entity, joints: &[Mat4]) -> Skinning {
        assert!(
            !joints.is_empty(),
            "a skinned instance poses at least one joint",
        );
        let wanted = joints.len() as u32;
        let fresh = match self.placed.get(&entity) {
            Some(range) if range.joints == wanted => false,
            Some(range) => {
                self.release(range.palette, range.joints);
                self.claim(entity, wanted);
                true
            }
            None => {
                self.claim(entity, wanted);
                true
            }
        };
        let range = self
            .placed
            .get(&entity)
            .copied()
            .expect("a posed entity holds the range it claimed");
        let first = self.matrices.len();
        self.matrices.extend_from_slice(joints);
        self.posed_entities.insert(entity);
        self.staged.push(Staged {
            range,
            first,
            joints: joints.len(),
            fresh,
        });
        Skinning {
            palette: range.palette,
            joints: range.joints,
        }
    }

    pub(crate) fn commit(&mut self, device: &Device, queue: &Queue) {
        self.release_stale();
        self.fit(device, queue, self.used);
        self.posed = self.base();
        let other = self.previous_base();
        let pool = self
            .pool
            .as_ref()
            .expect("a committed frame holds the buffer its poses write");
        for run in self.runs() {
            let mut records = Vec::new();
            for staged in &run {
                records.extend(
                    self.matrices[staged.first..staged.first + staged.joints]
                        .iter()
                        .map(joint_record),
                );
            }
            let bytes = bytemuck::cast_slice(&records);
            let palette = run[0].range.palette;
            pool.write_at(queue, u64::from(self.posed + palette) * JOINT_BYTES, bytes);
            if run[0].fresh {
                pool.write_at(queue, u64::from(other + palette) * JOINT_BYTES, bytes);
            }
        }
    }

    fn release_stale(&mut self) {
        let held = std::mem::take(&mut self.posed_entities);
        let stale = self
            .placed
            .iter()
            .filter(|(entity, _)| !held.contains(entity))
            .map(|(entity, range)| (*entity, *range))
            .collect::<Vec<_>>();
        for (entity, range) in stale {
            self.placed.remove(&entity);
            self.release(range.palette, range.joints);
        }
        self.posed_entities = held;
        self.coalesce();
    }

    fn coalesce(&mut self) {
        if self.free.len() < 2 {
            return;
        }
        self.free.sort_unstable_by_key(|range| range.palette);
        let mut merged: Vec<Range> = Vec::with_capacity(self.free.len());
        for range in self.free.drain(..) {
            match merged.last_mut() {
                Some(last) if last.palette + last.joints == range.palette => {
                    last.joints += range.joints
                }
                _ => merged.push(range),
            }
        }
        self.free = merged;
    }

    fn runs(&self) -> Vec<Vec<&Staged>> {
        let mut order = (0..self.staged.len()).collect::<Vec<_>>();
        order.sort_unstable_by_key(|index| self.staged[*index].range.palette);
        let mut runs: Vec<Vec<&Staged>> = Vec::new();
        let mut end = 0u32;
        let mut fresh = false;
        for index in order {
            let staged = &self.staged[index];
            if runs.is_empty() || staged.fresh != fresh || staged.range.palette != end {
                runs.push(Vec::new());
                fresh = staged.fresh;
            }
            let run = runs.last_mut().expect("a run opens before its stages");
            end = staged.range.palette + staged.range.joints;
            run.push(staged);
        }
        runs
    }

    pub(crate) fn finish(&mut self) {
        self.at = 1 - self.at;
    }

    pub(crate) fn binding(&self) -> BufferBinding<'_> {
        let pool = self
            .pool
            .as_ref()
            .expect("a committed frame holds the palette it poses");
        pool.binding(0, self.bytes())
    }

    pub(crate) fn base(&self) -> u32 {
        self.at as u32 * self.joints
    }

    pub(crate) fn previous_base(&self) -> u32 {
        (1 - self.at) as u32 * self.joints
    }

    pub(crate) fn posed(&self) -> u32 {
        self.posed
    }

    fn bytes(&self) -> u64 {
        HALVES * u64::from(self.joints) * JOINT_BYTES
    }

    fn claim(&mut self, entity: Entity, joints: u32) {
        let range = match self.take(joints) {
            Some(range) => range,
            None => {
                let range = Range {
                    palette: self.used,
                    joints,
                };
                self.used += joints;
                range
            }
        };
        self.placed.insert(entity, range);
    }

    fn release(&mut self, palette: u32, joints: u32) {
        self.free.push(Range { palette, joints });
    }

    fn take(&mut self, joints: u32) -> Option<Range> {
        let at = self.free.iter().position(|held| held.joints >= joints)?;
        let held = self.free.swap_remove(at);
        if held.joints == joints {
            return Some(held);
        }
        self.free.push(Range {
            palette: held.palette + joints,
            joints: held.joints - joints,
        });
        Some(Range {
            palette: held.palette,
            joints,
        })
    }

    fn fit(&mut self, device: &Device, queue: &Queue, joints: u32) {
        let wanted = HALVES * u64::from(joints) * JOINT_BYTES;
        let held = HALVES * u64::from(self.joints) * JOINT_BYTES;
        if self.pool.is_some() && wanted <= held {
            return;
        }
        let grown = wanted
            .next_multiple_of(POOL_BYTES * HALVES)
            .max(held.saturating_mul(2))
            .max(POOL_BYTES * HALVES);
        let pool = GpuBuffer::new(device, "dome joints", grown, storage());
        if let Some(previous) = &self.pool {
            let mut submission = Submission::new(device, "dome palette growth");
            let span = u64::from(self.joints) * JOINT_BYTES;
            let half = grown / HALVES;
            for at in 0..HALVES {
                if span > 0 {
                    submission.copy(previous, at * span, &pool, at * half, span);
                }
            }
            submission.submit(queue);
        }
        self.joints = (grown / HALVES / JOINT_BYTES) as u32;
        self.pool = Some(pool);
    }
}

fn storage() -> BufferUsages {
    BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC
}

fn joint_record(matrix: &Mat4) -> crate::abi::JointRecord {
    crate::abi::JointRecord {
        col0: crate::frame::vector(matrix.col(0)),
        col1: crate::frame::vector(matrix.col(1)),
        col2: crate::frame::vector(matrix.col(2)),
        col3: crate::frame::vector(matrix.col(3)),
    }
}

#[cfg(test)]
mod tests {
    use super::{Palettes, Skinning};
    use dome_ecs::World;
    use dome_math::Mat4;

    #[test]
    fn a_posed_entity_keeps_the_palette_range_it_claimed_first() {
        let mut world = World::new();
        let posed = world.spawn();
        let later = world.spawn();
        let mut palettes = Palettes::new();
        palettes.begin();
        let first = palettes.push(posed, &[Mat4::IDENTITY, Mat4::IDENTITY]);
        assert_eq!(
            first,
            Skinning {
                palette: 0,
                joints: 2
            }
        );
        palettes.begin();
        let second = palettes.push(later, &[Mat4::IDENTITY]);
        assert_eq!(
            second,
            Skinning {
                palette: 2,
                joints: 1
            }
        );
        palettes.begin();
        let again = palettes.push(posed, &[Mat4::IDENTITY, Mat4::IDENTITY]);
        assert_eq!(
            again, first,
            "a posed entity keeps the range it claimed while another rig claims beside it",
        );
        palettes.begin();
        let grown = palettes.push(posed, &[Mat4::IDENTITY; 3]);
        assert_eq!(
            grown,
            Skinning {
                palette: 3,
                joints: 3
            },
            "a wider rig claims a range that holds it",
        );
        palettes.begin();
        let reused = palettes.push(world.spawn(), &[Mat4::IDENTITY, Mat4::IDENTITY]);
        assert_eq!(
            reused, first,
            "the range a wider rig released returns to the pool",
        );
    }
}

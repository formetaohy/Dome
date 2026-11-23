use crate::abi::JointRecord;
use dome_ecs::Entity;
use dome_gpu::{BufferBinding, BufferUsages, Device, GpuBuffer, Queue, Submission};
use dome_math::{Mat4, Transform, Vec3};
use std::collections::{HashMap, HashSet};

const JOINT_BYTES: u64 = std::mem::size_of::<Mat4>() as u64;
const POOL_BYTES: u64 = 4096;
const HALVES: usize = 2;
const FNV_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Skinning {
    pub(crate) palette: u32,
    pub(crate) joints: u32,
}

pub(crate) const NONE: Skinning = Skinning {
    palette: 0,
    joints: 0,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Stamp(u64);

impl Stamp {
    pub(crate) fn new() -> Self {
        Self(FNV_BASIS)
    }

    pub(crate) fn transform(self, transform: Transform) -> Self {
        self.floats(&[
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
            transform.rotation.x,
            transform.rotation.y,
            transform.rotation.z,
            transform.rotation.w,
            transform.scale.x,
            transform.scale.y,
            transform.scale.z,
        ])
    }

    pub(crate) fn point(self, point: Vec3) -> Self {
        self.floats(&[point.x, point.y, point.z])
    }

    pub(crate) fn reach(self, radius: f32) -> Self {
        self.floats(&[radius])
    }

    pub(crate) fn matrix(self, matrix: Mat4) -> Self {
        (0..4).fold(self, |held, index| {
            let column = matrix.col(index);
            held.floats(&[column.x, column.y, column.z, column.w])
        })
    }

    fn floats(self, values: &[f32]) -> Self {
        let mut state = self.0;
        for value in values {
            state ^= u64::from(value.to_bits());
            state = state.wrapping_mul(FNV_PRIME);
        }
        Self(state)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Posed {
    pub(crate) skinning: Skinning,
    pub(crate) bounds: (Vec3, f32),
}

#[derive(Clone, Copy)]
struct Range {
    palette: u32,
    joints: u32,
}

struct Held {
    range: Range,
    matrices: Vec<Mat4>,
    stamp: Option<Stamp>,
    bounds: (Vec3, f32),
    halves: [Option<Stamp>; HALVES],
    fresh: bool,
}

pub(crate) struct Palettes {
    held: HashMap<Entity, Held>,
    free: Vec<Range>,
    staged: Vec<Entity>,
    posed_entities: HashSet<Entity>,
    pool: Option<GpuBuffer>,
    joints: u32,
    used: u32,
    at: usize,
    posed: u32,
}

impl Palettes {
    pub(crate) fn new() -> Self {
        Self {
            held: HashMap::new(),
            free: Vec::new(),
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
        self.staged.clear();
        self.posed_entities.clear();
    }

    pub(crate) fn try_reuse(&mut self, entity: Entity, joints: u32, stamp: Stamp) -> Option<Posed> {
        let held = self.held.get(&entity)?;
        if held.range.joints != joints || held.stamp != Some(stamp) {
            return None;
        }
        let posed = Posed {
            skinning: Skinning {
                palette: held.range.palette,
                joints: held.range.joints,
            },
            bounds: held.bounds,
        };
        self.posed_entities.insert(entity);
        self.staged.push(entity);
        Some(posed)
    }

    pub(crate) fn store(
        &mut self,
        entity: Entity,
        matrices: &[Mat4],
        stamp: Stamp,
        bounds: (Vec3, f32),
    ) -> Skinning {
        assert!(
            !matrices.is_empty(),
            "a skinned instance poses at least one joint",
        );
        let wanted = matrices.len() as u32;
        match self.held.get(&entity) {
            Some(held) if held.range.joints == wanted => {}
            Some(held) => {
                let range = held.range;
                self.release(range);
                self.claim(entity, wanted);
            }
            None => self.claim(entity, wanted),
        }
        let held = self
            .held
            .get_mut(&entity)
            .expect("a claimed pose holds the range it claimed");
        held.matrices.clear();
        held.matrices.extend_from_slice(matrices);
        held.stamp = Some(stamp);
        held.bounds = bounds;
        let skinning = Skinning {
            palette: held.range.palette,
            joints: held.range.joints,
        };
        self.posed_entities.insert(entity);
        self.staged.push(entity);
        skinning
    }

    pub(crate) fn commit(&mut self, device: &Device, queue: &Queue) {
        self.release_stale();
        self.fit(device, queue, self.used);
        self.posed = self.base();
        let at = self.at;
        let bases = [self.half_base(0), self.half_base(1)];
        let pool = self
            .pool
            .as_ref()
            .expect("a committed frame holds the buffer its poses write");
        for entity in std::mem::take(&mut self.staged) {
            let held = self
                .held
                .get_mut(&entity)
                .expect("a staged pose holds its range");
            let Some(stamp) = held.stamp else {
                continue;
            };
            if !held.fresh && held.halves[at] == Some(stamp) {
                continue;
            }
            let records = held
                .matrices
                .iter()
                .copied()
                .map(joint_record)
                .collect::<Vec<_>>();
            let bytes = bytemuck::cast_slice(&records);
            let palette = u64::from(held.range.palette) * JOINT_BYTES;
            if held.fresh {
                held.fresh = false;
                for (half, base) in bases.iter().enumerate() {
                    pool.write_at(queue, *base + palette, bytes);
                    held.halves[half] = Some(stamp);
                }
            } else {
                pool.write_at(queue, bases[at] + palette, bytes);
                held.halves[at] = Some(stamp);
            }
        }
    }

    fn release_stale(&mut self) {
        let held = std::mem::take(&mut self.posed_entities);
        let stale = self
            .held
            .keys()
            .filter(|entity| !held.contains(entity))
            .copied()
            .collect::<Vec<_>>();
        for entity in stale {
            if let Some(released) = self.held.remove(&entity) {
                self.release(released.range);
            }
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

    fn half_base(&self, half: usize) -> u64 {
        u64::from(half as u32 * self.joints) * JOINT_BYTES
    }

    fn bytes(&self) -> u64 {
        HALVES as u64 * u64::from(self.joints) * JOINT_BYTES
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
        self.held.insert(
            entity,
            Held {
                range,
                matrices: Vec::new(),
                stamp: None,
                bounds: (Vec3::ZERO, 0.0),
                halves: [None; HALVES],
                fresh: true,
            },
        );
    }

    fn release(&mut self, range: Range) {
        self.free.push(range);
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
        let wanted = HALVES as u64 * u64::from(joints) * JOINT_BYTES;
        let held = HALVES as u64 * u64::from(self.joints) * JOINT_BYTES;
        if self.pool.is_some() && wanted <= held {
            return;
        }
        let grown = wanted
            .next_multiple_of(POOL_BYTES * HALVES as u64)
            .max(held.saturating_mul(2))
            .max(POOL_BYTES * HALVES as u64);
        let pool = GpuBuffer::new(device, "dome joints", grown, storage());
        if let Some(previous) = &self.pool {
            let mut submission = Submission::new(device, "dome palette growth");
            let span = u64::from(self.joints) * JOINT_BYTES;
            let half = grown / HALVES as u64;
            for at in 0..HALVES {
                if span > 0 {
                    submission.copy(previous, at as u64 * span, &pool, at as u64 * half, span);
                }
            }
            submission.submit(queue);
        }
        self.joints = (grown / HALVES as u64 / JOINT_BYTES) as u32;
        self.pool = Some(pool);
    }
}

fn storage() -> BufferUsages {
    BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC
}

fn joint_record(matrix: Mat4) -> JointRecord {
    JointRecord {
        col0: crate::frame::vector(matrix.col(0)),
        col1: crate::frame::vector(matrix.col(1)),
        col2: crate::frame::vector(matrix.col(2)),
        col3: crate::frame::vector(matrix.col(3)),
    }
}

#[cfg(test)]
mod tests {
    use super::{Palettes, Skinning, Stamp};
    use dome_ecs::World;
    use dome_math::{Mat4, Vec3};

    fn stamp(seed: f32) -> Stamp {
        Stamp::new().reach(seed)
    }

    #[test]
    fn a_posed_entity_keeps_the_palette_range_it_claimed_first() {
        let mut world = World::new();
        let posed = world.spawn();
        let later = world.spawn();
        let mut palettes = Palettes::new();
        palettes.begin();
        let first = palettes.store(posed, &[Mat4::IDENTITY; 2], stamp(1.0), (Vec3::ZERO, 1.0));
        assert_eq!(
            first,
            Skinning {
                palette: 0,
                joints: 2
            }
        );
        palettes.begin();
        let second = palettes.store(later, &[Mat4::IDENTITY], stamp(1.0), (Vec3::ZERO, 1.0));
        assert_eq!(
            second,
            Skinning {
                palette: 2,
                joints: 1
            }
        );
        palettes.begin();
        let again = palettes.store(posed, &[Mat4::IDENTITY; 2], stamp(2.0), (Vec3::ZERO, 1.0));
        assert_eq!(
            again, first,
            "a posed entity keeps the range it claimed while another rig claims beside it",
        );
        palettes.begin();
        let grown = palettes.store(posed, &[Mat4::IDENTITY; 3], stamp(3.0), (Vec3::ZERO, 1.0));
        assert_eq!(
            grown,
            Skinning {
                palette: 3,
                joints: 3
            },
            "a wider rig claims a range that holds it",
        );
        palettes.begin();
        let reused = palettes.store(
            world.spawn(),
            &[Mat4::IDENTITY; 2],
            stamp(1.0),
            (Vec3::ZERO, 1.0),
        );
        assert_eq!(
            reused, first,
            "the range a wider rig released returns to the pool",
        );
    }

    #[test]
    fn an_unchanged_pose_hands_back_the_bounds_and_range_it_stored() {
        let mut world = World::new();
        let entity = world.spawn();
        let mut palettes = Palettes::new();
        let bounds = (Vec3::new(1.0, 2.0, 3.0), 4.0);
        palettes.begin();
        let stored = palettes.store(entity, &[Mat4::IDENTITY; 2], stamp(7.0), bounds);
        palettes.begin();
        let posed = palettes
            .try_reuse(entity, 2, stamp(7.0))
            .expect("a pose that did not change reuses the palette it stored");
        assert_eq!(posed.skinning, stored);
        assert_eq!(posed.bounds, bounds);
        palettes.begin();
        assert!(
            palettes.try_reuse(entity, 2, stamp(8.0)).is_none(),
            "a pose that changed stages its palette again",
        );
        palettes.begin();
        assert!(
            palettes.try_reuse(entity, 3, stamp(7.0)).is_none(),
            "a rig of another width claims a range that holds it",
        );
    }
}

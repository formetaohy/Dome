use crate::frame::Batch;
use crate::mesh::Mesh;

const ABSENT: u32 = u32::MAX;

pub(crate) struct Batches {
    epoch: u32,
    stamps: Vec<u32>,
    counts: Vec<u32>,
    index: Vec<u32>,
    touched: Vec<(Mesh, bool)>,
    batches: Vec<Batch>,
    bases: Vec<u32>,
    slots: u32,
    depth: u32,
}

impl Batches {
    pub(crate) fn new() -> Self {
        Self {
            epoch: 1,
            stamps: Vec::new(),
            counts: Vec::new(),
            index: Vec::new(),
            touched: Vec::new(),
            batches: Vec::new(),
            bases: Vec::new(),
            slots: 0,
            depth: 0,
        }
    }

    pub(crate) fn begin(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.touched.clear();
    }

    pub(crate) fn claim(&mut self, mesh: Mesh, blended: bool) {
        let at = Self::slot(mesh, blended);
        if self.stamps.len() <= at {
            self.grow(at + 1);
        }
        if self.stamps[at] != self.epoch {
            self.stamps[at] = self.epoch;
            self.counts[at] = 0;
            self.touched.push((mesh, blended));
        }
        self.counts[at] += 1;
    }

    pub(crate) fn settle(&mut self, blocks: u32) {
        let stale = self.touched.len() != self.batches.len()
            || self.touched.iter().any(|(mesh, blended)| {
                let batch = self.index[Self::slot(*mesh, *blended)];
                batch == ABSENT || self.batches[batch as usize].mesh != *mesh
            });
        if stale {
            self.rebuild();
        }
        for batch in &mut self.batches {
            batch.count = self.counts[Self::slot(batch.mesh, batch.blended)];
        }
        self.bases.clear();
        let mut slot = 0;
        for batch in &self.batches {
            self.bases.push(slot);
            slot = (slot + batch.count).div_ceil(blocks) * blocks;
        }
        self.slots = slot;
    }

    pub(crate) fn table(&self) -> &[Batch] {
        &self.batches
    }

    pub(crate) fn bases(&self) -> &[u32] {
        &self.bases
    }

    pub(crate) fn slots(&self) -> u32 {
        self.slots
    }

    pub(crate) fn depth(&self) -> u32 {
        self.depth
    }

    #[track_caller]
    pub(crate) fn batch_of(&self, mesh: Mesh, blended: bool) -> u32 {
        let batch = self.index[Self::slot(mesh, blended)];
        assert!(
            batch != ABSENT,
            "an instance of mesh {} names a batch its collection holds",
            mesh.index(),
        );
        batch
    }

    fn rebuild(&mut self) {
        self.index.fill(ABSENT);
        self.touched
            .sort_unstable_by_key(|(mesh, blended)| (*blended, mesh.index()));
        self.touched.dedup();
        self.batches.clear();
        self.depth = 0;
        for (at, (mesh, blended)) in self.touched.iter().enumerate() {
            self.index[Self::slot(*mesh, *blended)] = at as u32;
            self.batches.push(Batch {
                mesh: *mesh,
                blended: *blended,
                count: 0,
            });
            if !blended {
                self.depth = at as u32 + 1;
            }
        }
    }

    fn grow(&mut self, len: usize) {
        let absent = self.epoch.wrapping_sub(1);
        self.stamps.resize(len, absent);
        self.counts.resize(len, 0);
        self.index.resize(len, ABSENT);
    }

    fn slot(mesh: Mesh, blended: bool) -> usize {
        mesh.index() as usize * 2 + usize::from(blended)
    }
}

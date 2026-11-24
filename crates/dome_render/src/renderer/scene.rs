use super::bounds::{joint_transform, skinned_bounds, static_bounds};
use super::*;
use crate::instances::Source;
use crate::palette::Stamp;

pub(super) struct Upload {
    pub(super) offset: u64,
    pub(super) records: Vec<InstanceRecord>,
}

pub(super) struct Collection {
    pub(super) entities: Vec<Entity>,
    pub(super) batches: Vec<Batch>,
    pub(super) bases: Vec<u32>,
    pub(super) slots: u32,
    pub(super) depth_batches: u32,
    pub(super) total: u32,
    pub(super) spheres: Vec<(Vec3, f32)>,
    pub(super) highlight: u32,
    pub(super) uploads: Vec<Upload>,
}

impl Renderer {
    pub(super) fn collect(
        &mut self,
        world: &World,
        highlight: Option<Entity>,
        phase: f32,
    ) -> Collection {
        let Renderer {
            instances,
            meshes,
            materials,
            palettes,
            ..
        } = self;
        let mut gathered = Vec::new();
        world.for_each_pair::<Transform, MeshInstance>(|entity, transform, instance| {
            if world.contains::<Hidden>(entity) {
                return;
            }
            let transform = pose_of(world, entity, phase).unwrap_or(*transform);
            gathered.push((
                entity,
                transform,
                *instance,
                materials.mode(instance.material) == AlphaMode::Blend,
            ));
        });
        instances.begin();
        for (entity, ..) in &gathered {
            instances.claim(*entity);
        }
        instances.settle();
        for (entity, transform, instance, blended) in &gathered {
            let index = instances
                .slot_of(*entity)
                .expect("a claimed slot answers the entity that claimed it");
            instances.at_mut(index as usize).staged = Some(Source {
                transform: *transform,
                mesh: instance.mesh,
                material: instance.material,
                tint: instance.tint,
                blended: *blended,
            });
        }
        palettes.begin();
        let mut scratch = Vec::new();
        let mut joints = Vec::new();
        for index in 0..instances.len() {
            let slot = instances.at_mut(index);
            let entity = slot.entity;
            let source = slot.source();
            slot.skinning = crate::palette::NONE;
            slot.bounds = None;
            let Some(skin) = world.get::<Skin>(entity) else {
                continue;
            };
            let geometry = meshes.geometry(source.mesh);
            assert!(
                geometry.skin_count > 0,
                "a skinned instance renders a mesh without skin weights",
            );
            assert_eq!(
                skin.joints().len(),
                skin.inverse_binds().len(),
                "a skin holds {} joints for {} inverse bind matrices",
                skin.joints().len(),
                skin.inverse_binds().len(),
            );
            joints.clear();
            let mut stamp = Stamp::new()
                .transform(source.transform)
                .point(geometry.center)
                .reach(geometry.radius);
            for (joint, bind) in skin.joints().iter().zip(skin.inverse_binds()) {
                let held = joint_transform(world, *joint, phase);
                stamp = stamp.transform(held).matrix(*bind);
                joints.push(held);
            }
            let (skinning, bounds) = match palettes.try_reuse(entity, joints.len() as u32, stamp) {
                Some(posed) => (posed.skinning, posed.bounds),
                None => {
                    let inverse = source.transform.matrix().inverse();
                    scratch.clear();
                    for (held, bind) in joints.iter().zip(skin.inverse_binds()) {
                        scratch.push(inverse * held.matrix() * *bind);
                    }
                    let bounds = skinned_bounds(geometry, source.transform, &scratch);
                    (palettes.store(entity, &scratch, stamp, bounds), bounds)
                }
            };
            let slot = instances.at_mut(index);
            slot.skinning = skinning;
            slot.bounds = Some(bounds);
        }
        palettes.commit(&self.device, &self.queue);
        let alignment = self.device.binding_alignment().max(WORD_BYTES);
        let blocks = span::blocks(alignment, INDEX_BYTES);
        self.batches.begin();
        for (_, _, instance, blended) in &gathered {
            self.batches.claim(instance.mesh, *blended);
        }
        self.batches.settle(blocks);
        let total = instances.len() as u32;
        if self
            .instance_buffer
            .fit(&self.device, u64::from(total) * INSTANCE_BYTES)
        {
            instances.restart();
        }
        for index in 0..instances.len() {
            let slot = instances.at(index);
            let source = slot.source();
            let batch = self.batches.batch_of(source.mesh, source.blended);
            let model = source.transform.matrix();
            let previous = slot.model.unwrap_or(model);
            let bounds = slot
                .bounds
                .unwrap_or_else(|| static_bounds(meshes.geometry(source.mesh), source.transform));
            let record = record_of(
                slot.entity,
                source.transform,
                previous,
                &MeshInstance {
                    mesh: source.mesh,
                    material: source.material,
                    tint: source.tint,
                },
                bounds,
                Placement {
                    batch,
                    base: self.batches.bases()[batch as usize],
                    skinning: slot.skinning,
                    hzb: !source.blended,
                },
            );
            if slot.record != Some(record) {
                let slot = instances.at_mut(index);
                slot.record = Some(record);
                slot.model = Some(model);
                instances.mark(index);
            }
        }
        let mut uploads = Vec::new();
        for run in runs_of(instances.dirty()) {
            let records = run
                .clone()
                .map(|index| {
                    instances
                        .at(index as usize)
                        .record
                        .expect("a changed slot holds the record it uploads")
                })
                .collect::<Vec<_>>();
            uploads.push(Upload {
                offset: u64::from(run.start) * INSTANCE_BYTES,
                records,
            });
        }
        let spheres = instances
            .slots()
            .iter()
            .map(|slot| {
                let record = slot.record.expect("a collected slot holds its record");
                (
                    Vec3::new(record.bounds.x, record.bounds.y, record.bounds.z),
                    record.bounds.w,
                )
            })
            .collect::<Vec<_>>();
        Collection {
            batches: self.batches.table().to_vec(),
            bases: self.batches.bases().to_vec(),
            slots: self.batches.slots(),
            depth_batches: self.batches.depth(),
            total,
            spheres,
            highlight: highlight
                .and_then(|held| instances.slot_of(held))
                .unwrap_or(NO_HIGHLIGHT),
            entities: instances.slots().iter().map(|slot| slot.entity).collect(),
            uploads,
        }
    }
}

fn runs_of(dirty: &[u32]) -> Vec<std::ops::Range<u32>> {
    let mut sorted = dirty.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut runs = Vec::new();
    let mut start = 0;
    while start < sorted.len() {
        let mut end = start + 1;
        while end < sorted.len() && sorted[end] == sorted[end - 1] + 1 {
            end += 1;
        }
        runs.push(sorted[start]..sorted[end - 1] + 1);
        start = end;
    }
    runs
}

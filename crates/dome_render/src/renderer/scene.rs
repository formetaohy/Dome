use super::bounds::{joint_world, skinned_bounds, static_bounds};
use super::*;
use crate::instances::Source;

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
    pub(super) fn collect(&mut self, world: &World, highlight: Option<Entity>) -> Collection {
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
            gathered.push((
                entity,
                *transform,
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
            let model = source.transform.matrix();
            let inverse = model.inverse();
            scratch.clear();
            for (joint, bind) in skin.joints().iter().zip(skin.inverse_binds()) {
                let joint = joint_world(world, *joint);
                scratch.push(inverse * joint * *bind);
            }
            let skinning = palettes.push(entity, &scratch);
            let bounds = skinned_bounds(geometry, source.transform, &scratch);
            let slot = instances.at_mut(index);
            slot.skinning = skinning;
            slot.bounds = Some(bounds);
        }
        palettes.commit(&self.device, &self.queue);
        let alignment = self
            .device
            .limits()
            .min_storage_buffer_offset_alignment
            .max(WORD_BYTES);
        let blocks = span::blocks(alignment, DRAW_BYTES);
        let mut counted = HashMap::new();
        for (_, _, instance, blended) in &gathered {
            let held = counted
                .entry((instance.mesh.index(), *blended))
                .or_insert((instance.mesh, 0u32));
            held.1 += 1;
        }
        let mut pairs = counted.into_iter().collect::<Vec<_>>();
        pairs.sort_unstable_by_key(|((mesh, blended), _)| (*blended, *mesh));
        let counts = pairs
            .iter()
            .map(|(_, (_, count))| *count)
            .collect::<Vec<_>>();
        let (bases, slots) = bases_of(&counts, blocks);
        let depth_batches = pairs.iter().filter(|(key, _)| !key.1).count() as u32;
        let batches = pairs
            .iter()
            .map(|(_, (mesh, count))| Batch {
                mesh: *mesh,
                count: *count,
            })
            .collect::<Vec<_>>();
        let batch_of = pairs
            .iter()
            .enumerate()
            .map(|(index, (key, _))| (*key, index as u32))
            .collect::<HashMap<_, _>>();
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
            let batch = batch_of[&(source.mesh.index(), source.blended)];
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
                    base: bases[batch as usize],
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
            batches,
            bases,
            slots,
            depth_batches,
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

use crate::clip::{Property, Sample};
use crate::layer::AnimationLayer;
use crate::source::ClipSource;
use dome_ecs::{Entity, LocalTransform, World};
use dome_math::{Quat, Transform, Vec3};

pub(crate) struct Pose {
    entries: Vec<Entry>,
}

struct Entry {
    entity: Entity,
    translation: Blend<Vec3>,
    rotation: Blend<Quat>,
    scale: Blend<Vec3>,
}

struct Blend<T> {
    value: T,
    weight: f32,
}

impl<T: Copy> Blend<T> {
    fn new(value: T) -> Self {
        Self { value, weight: 0.0 }
    }
}

impl Blend<Vec3> {
    fn add(&mut self, value: Vec3, weight: f32) {
        let total = self.weight + weight;
        self.value = self.value * (self.weight / total) + value * (weight / total);
        self.weight = total;
    }
}

impl Blend<Quat> {
    fn add(&mut self, value: Quat, weight: f32) {
        let total = self.weight + weight;
        self.value = self.value.slerp(value, weight / total);
        self.weight = total;
    }
}

impl Pose {
    pub(crate) fn new<'a>(world: &World, sources: impl Iterator<Item = &'a ClipSource>) -> Self {
        let mut targets = Vec::new();
        for source in sources {
            targets.extend_from_slice(source.targets());
        }
        targets.sort_unstable();
        targets.dedup();
        let entries = targets
            .into_iter()
            .map(|entity| {
                let base = world
                    .get::<LocalTransform>(entity)
                    .unwrap_or_else(|| {
                        panic!(
                            "an animation targets entity {}, which holds no local transform",
                            entity.index(),
                        )
                    })
                    .0;
                Entry {
                    entity,
                    translation: Blend::new(base.translation),
                    rotation: Blend::new(base.rotation),
                    scale: Blend::new(base.scale),
                }
            })
            .collect();
        Self { entries }
    }

    pub(crate) fn sample(&mut self, layer: &AnimationLayer) {
        let weight = layer.weight();
        if weight <= 0.0 {
            return;
        }
        let source = layer.source();
        for (track, target) in source.clip().tracks().iter().zip(source.targets()) {
            let at = self
                .entries
                .binary_search_by_key(target, |entry| entry.entity)
                .expect("an animation poses the targets its source names");
            let entry = &mut self.entries[at];
            match track.sample(layer.time()) {
                Sample::Vector(vector) => match track.property() {
                    Property::Translation => entry.translation.add(vector, weight),
                    Property::Scale => entry.scale.add(vector, weight),
                    Property::Rotation => {
                        unreachable!("a rotation track samples a vector")
                    }
                },
                Sample::Rotation(rotation) => {
                    assert_eq!(
                        track.property(),
                        Property::Rotation,
                        "the track of {} samples a rotation",
                        track.target(),
                    );
                    entry.rotation.add(rotation, weight);
                }
            }
        }
    }

    pub(crate) fn transforms(self) -> Vec<(Entity, Transform)> {
        self.entries
            .into_iter()
            .map(|entry| {
                (
                    entry.entity,
                    Transform {
                        translation: entry.translation.value,
                        rotation: entry.rotation.value,
                        scale: entry.scale.value,
                    },
                )
            })
            .collect()
    }
}

use crate::binding::{BindGroup, BindGroupLayout, GroupKey};
use crate::identity::Retirements;
use crate::shader::BindingSpec;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const WARM_BIND_GROUPS: usize = 8192;

struct Held {
    group: BindGroup,
    used: u64,
}

#[derive(Default)]
struct Groups {
    entries: HashMap<GroupKey, Held>,
    clock: u64,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct LayoutKey {
    specs: Vec<BindingSpec>,
    graphics: bool,
}

#[derive(Default)]
pub(crate) struct Cache {
    shaders: Mutex<HashMap<(usize, usize), wgpu::ShaderModule>>,
    layouts: Mutex<HashMap<LayoutKey, Arc<BindGroupLayout>>>,
    groups: Mutex<Groups>,
    retirements: Arc<Retirements>,
}

impl Cache {
    pub(crate) fn retirements(&self) -> &Arc<Retirements> {
        &self.retirements
    }

    pub(crate) fn len(&self) -> usize {
        self.groups
            .lock()
            .expect("a bind group cache is never poisoned")
            .entries
            .len()
    }

    pub(crate) fn shader(
        &self,
        device: &wgpu::Device,
        label: &'static str,
        source: &'static str,
    ) -> wgpu::ShaderModule {
        let key = (source.as_ptr() as usize, source.len());
        self.shaders
            .lock()
            .expect("a shader module cache is never poisoned")
            .entry(key)
            .or_insert_with(|| {
                device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(label),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                })
            })
            .clone()
    }

    pub(crate) fn layout(
        &self,
        specs: Vec<BindingSpec>,
        graphics: bool,
        build: impl FnOnce(&[BindingSpec]) -> wgpu::BindGroupLayout,
    ) -> Arc<BindGroupLayout> {
        let key = LayoutKey { specs, graphics };
        if let Some(layout) = self
            .layouts
            .lock()
            .expect("a bind group layout cache is never poisoned")
            .get(&key)
        {
            return Arc::clone(layout);
        }
        let native = build(&key.specs);
        let layout = Arc::new(BindGroupLayout::new(native, key.specs.clone()));
        self.layouts
            .lock()
            .expect("a bind group layout cache is never poisoned")
            .insert(key, Arc::clone(&layout));
        layout
    }

    pub(crate) fn group(&self, key: GroupKey, build: impl FnOnce() -> BindGroup) -> BindGroup {
        self.bury();
        let mut groups = self
            .groups
            .lock()
            .expect("a bind group cache is never poisoned");
        let clock = groups.clock + 1;
        groups.clock = clock;
        if let Some(held) = groups.entries.get_mut(&key) {
            held.used = clock;
            return held.group.clone();
        }
        drop(groups);
        let group = build();
        let mut groups = self
            .groups
            .lock()
            .expect("a bind group cache is never poisoned");
        groups.clock += 1;
        let used = groups.clock;
        groups.entries.insert(
            key,
            Held {
                group: group.clone(),
                used,
            },
        );
        if groups.entries.len() > WARM_BIND_GROUPS {
            let oldest = groups.clock - WARM_BIND_GROUPS as u64;
            groups.entries.retain(|_, held| held.used > oldest);
        }
        group
    }

    pub(crate) fn bury(&self) {
        let retired = self.retirements.drain();
        if retired.is_empty() {
            return;
        }
        self.groups
            .lock()
            .expect("a bind group cache is never poisoned")
            .entries
            .retain(|key, _| !key.retired(&retired));
    }
}

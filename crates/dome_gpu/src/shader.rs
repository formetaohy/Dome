#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindingKind {
    Storage,
    WritableStorage,
    SampledTexture,
    UnfilterableTexture,
    DepthTexture,
    FilteringSampler,
    NonFilteringSampler,
    ComparisonSampler,
    TextureTable,
    SamplerTable,
}

impl BindingKind {
    pub const fn buffer(self) -> bool {
        matches!(self, Self::Storage | Self::WritableStorage)
    }

    pub const fn texture(self) -> bool {
        matches!(
            self,
            Self::SampledTexture
                | Self::UnfilterableTexture
                | Self::DepthTexture
                | Self::TextureTable
        )
    }

    pub const fn sampler(self) -> bool {
        matches!(
            self,
            Self::FilteringSampler
                | Self::NonFilteringSampler
                | Self::ComparisonSampler
                | Self::SamplerTable
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BindingSpec {
    pub group: u32,
    pub binding: u32,
    pub kind: BindingKind,
}

impl BindingSpec {
    pub const fn storage(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::Storage,
        }
    }

    pub const fn writable_storage(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::WritableStorage,
        }
    }

    pub const fn sampled_texture(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::SampledTexture,
        }
    }

    pub const fn unfilterable_texture(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::UnfilterableTexture,
        }
    }

    pub const fn depth_texture(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::DepthTexture,
        }
    }

    pub const fn filtering_sampler(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::FilteringSampler,
        }
    }

    pub const fn non_filtering_sampler(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::NonFilteringSampler,
        }
    }

    pub const fn comparison_sampler(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::ComparisonSampler,
        }
    }

    pub const fn texture_table(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::TextureTable,
        }
    }

    pub const fn sampler_table(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::SamplerTable,
        }
    }
}

pub struct GraphicsShader {
    pub label: &'static str,
    pub source: &'static str,
    pub vertex: &'static str,
    pub fragment: &'static str,
    pub bindings: &'static [BindingSpec],
    pub targets: usize,
}

pub struct ComputeShader {
    pub label: &'static str,
    pub source: &'static str,
    pub entry: &'static str,
    pub bindings: &'static [BindingSpec],
    pub workgroup: u32,
}

impl GraphicsShader {
    pub fn label(&self) -> &'static str {
        self.label
    }
}

impl ComputeShader {
    pub fn label(&self) -> &'static str {
        self.label
    }
}

pub(crate) fn group_count(bindings: &[BindingSpec], label: &str, compute: bool) -> u32 {
    let groups = bindings
        .iter()
        .map(|binding| binding.group)
        .max()
        .map_or(0, |group| group + 1);
    if compute {
        assert_eq!(
            groups, 1,
            "the compute program {label} binds {groups} groups where a dispatch binds one"
        );
    }
    let mut sorted = bindings.to_vec();
    sorted.sort_unstable_by_key(|binding| (binding.group, binding.binding));
    let mut group = 0;
    let mut slot = 0;
    for (index, binding) in sorted.iter().enumerate() {
        if index == 0 {
            assert_eq!(
                binding.group, 0,
                "the program {label} skips a binding group"
            );
        } else if binding.group != group {
            assert_eq!(
                binding.group,
                group + 1,
                "the program {label} skips a binding group"
            );
            group = binding.group;
            slot = 0;
        }
        assert_eq!(
            binding.binding, slot,
            "the program {label} skips a binding slot within group {group}"
        );
        slot += 1;
    }
    groups
}

pub(crate) fn group_specs(bindings: &[BindingSpec], group: u32, label: &str) -> Vec<BindingSpec> {
    let groups = group_count(bindings, label, false);
    assert!(
        group < groups,
        "the program {label} declares {groups} bind groups where {group} is requested"
    );
    bindings
        .iter()
        .filter(|binding| binding.group == group)
        .copied()
        .collect()
}

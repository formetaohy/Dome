use crate::shader::{
    Access, Interface, Module, Scalar, Space, Stage, Target, Type, TypeId, element_name,
};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub const METAL_SIZE_BUFFER_SLOT: u8 = 30;

pub const TEXTURE_TABLE_CAPACITY: u32 = 1024;
pub const SAMPLER_TABLE_CAPACITY: u32 = 64;

pub const MAX_BINDING_BYTES: u64 = 1 << 34;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Backend {
    Vulkan,
    Metal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindingKind {
    ReadOnlyStorage,
    ReadWriteStorage,
    TableStorage,
    SampledImage,
    Sampler,
    SampledImageTable,
    SamplerTable,
}

impl BindingKind {
    pub const fn writable(self) -> bool {
        matches!(self, Self::ReadWriteStorage | Self::TableStorage)
    }

    pub const fn storage(self) -> bool {
        matches!(
            self,
            Self::ReadOnlyStorage | Self::ReadWriteStorage | Self::TableStorage
        )
    }

    pub const fn buffer(self) -> bool {
        matches!(
            self,
            Self::ReadOnlyStorage
                | Self::ReadWriteStorage
                | Self::TableStorage
                | Self::SampledImageTable
                | Self::SamplerTable
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BindingSpec {
    pub group: u32,
    pub binding: u32,
    pub kind: BindingKind,
    pub count: u32,
}

impl BindingSpec {
    pub const fn storage(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::ReadOnlyStorage,
            count: 1,
        }
    }

    pub const fn writable_storage(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::ReadWriteStorage,
            count: 1,
        }
    }

    pub const fn table_storage(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::TableStorage,
            count: 1,
        }
    }

    pub const fn sampled_image(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::SampledImage,
            count: 1,
        }
    }

    pub const fn sampler(group: u32, binding: u32) -> Self {
        Self {
            group,
            binding,
            kind: BindingKind::Sampler,
            count: 1,
        }
    }

    pub const fn sampled_image_table(group: u32, binding: u32, count: u32) -> Self {
        assert!(count > 0, "a sampled image table holds no image");
        Self {
            group,
            binding,
            kind: BindingKind::SampledImageTable,
            count,
        }
    }

    pub const fn sampler_table(group: u32, binding: u32, count: u32) -> Self {
        assert!(count > 0, "a sampler table holds no sampler");
        Self {
            group,
            binding,
            kind: BindingKind::SamplerTable,
            count,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderBinding {
    pub group: u32,
    pub binding: u32,
    pub name: String,
    pub kind: BindingKind,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShaderTranslation {
    Spirv(Vec<u32>),
    Msl { source: String, entry: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphicsTranslation {
    Spirv {
        module: Vec<u32>,
        vertex: String,
        fragment: String,
    },
    Msl {
        source: String,
        vertex: String,
        fragment: String,
    },
}

pub fn reflect(module: &Module) -> Vec<ShaderBinding> {
    let mut bindings = module
        .globals()
        .iter()
        .filter_map(|global| {
            let binding = global.binding?;
            let (kind, count) = match &global.space {
                Space::Storage => (
                    match global.access {
                        Access::Read => BindingKind::ReadOnlyStorage,
                        Access::ReadWrite => BindingKind::ReadWriteStorage,
                        Access::Table => BindingKind::TableStorage,
                    },
                    1,
                ),
                Space::UniformConstant => match module.ty(global.ty) {
                    Type::Image => (BindingKind::SampledImage, 1),
                    Type::Sampler => (BindingKind::Sampler, 1),
                    Type::Array {
                        element,
                        count: Some(count),
                    } => match module.ty(*element) {
                        Type::Image => (BindingKind::SampledImageTable, *count),
                        Type::Sampler => (BindingKind::SamplerTable, *count),
                        other => panic!(
                            "the uniform constant table {} holds {}",
                            global.name,
                            element_name(other)
                        ),
                    },
                    other => panic!(
                        "the uniform constant {} is {}",
                        global.name,
                        element_name(other)
                    ),
                },
                other => panic!(
                    "the bound global {} lives in {} memory",
                    global.name,
                    other.name()
                ),
            };
            Some(ShaderBinding {
                group: binding.group,
                binding: binding.binding,
                name: global.name.clone(),
                kind,
                count,
            })
        })
        .collect::<Vec<_>>();
    bindings.sort_by_key(|binding| (binding.group, binding.binding));
    bindings
}

pub fn describe(bindings: &[ShaderBinding]) -> String {
    let mut out = String::new();
    for binding in bindings {
        writeln!(
            out,
            "group {} binding {} {} {:?}",
            binding.group, binding.binding, binding.name, binding.kind,
        )
        .expect("a string accepts binding descriptions");
    }
    out
}

struct Compiled {
    label: String,
    module: Module,
    entry: String,
    workgroup: u32,
    bindings: Vec<BindingSpec>,
    reflected: Vec<ShaderBinding>,
    spirv: Vec<u32>,
}

#[derive(Clone)]
pub struct ComputeProgram {
    compiled: Arc<Compiled>,
}

impl PartialEq for ComputeProgram {
    fn eq(&self, other: &Self) -> bool {
        self.compiled.spirv == other.compiled.spirv
            && self.compiled.bindings == other.compiled.bindings
    }
}

impl Eq for ComputeProgram {}

impl Hash for ComputeProgram {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.compiled.spirv.hash(state);
        self.compiled.bindings.hash(state);
    }
}

impl std::fmt::Debug for ComputeProgram {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct(&self.compiled.label)
            .field("entry", &self.compiled.entry)
            .field("bindings", &self.compiled.bindings)
            .field("instructions", &self.compiled.spirv.len())
            .finish()
    }
}

impl ComputeProgram {
    pub fn new(label: &str, module: Module, entry: &str, bindings: &[BindingSpec]) -> Self {
        let buffers = bindings
            .iter()
            .filter(|binding| binding.kind.storage())
            .count();
        assert!(
            !bindings.is_empty() && buffers <= METAL_SIZE_BUFFER_SLOT as usize,
            "a compute program binds between one and 30 buffers"
        );
        assert!(
            module.compute().is_some(),
            "{label} is not a compute program"
        );
        let program = module.stage_function(Stage::Compute);
        assert_eq!(
            program.name, entry,
            "{label} carries the device entry {} instead of {entry}",
            program.name
        );
        assert!(
            program.result.is_none(),
            "{label} declares an entry that returns a value"
        );
        let workgroup = module.workgroup_size();
        let reflected = reflect(&module);
        assert_eq!(
            reflected.len(),
            bindings.len(),
            "{label} declares different storage resources from its bindings:\n{}",
            describe(&reflected)
        );
        dense(bindings, label);
        for (declared, spec) in reflected.iter().zip(bindings) {
            assert_eq!(
                (declared.group, declared.binding),
                (spec.group, spec.binding),
                "binding group {} slot {} is missing",
                spec.group,
                spec.binding
            );
            assert_eq!(
                (declared.kind, declared.count),
                (spec.kind, spec.count),
                "binding group {} slot {} has the wrong kind or count",
                spec.group,
                spec.binding
            );
        }
        for global in module.globals() {
            if global.space == crate::shader::Space::WorkGroup {
                assert!(
                    matches!(
                        module.ty(global.ty),
                        crate::shader::Type::Array { count: Some(_), .. }
                    ),
                    "{label} declares the workgroup variable {} as a {}",
                    global.name,
                    element_name(module.ty(global.ty))
                );
            }
        }
        let spirv = crate::shader::spirv::write(&module);
        Self {
            compiled: Arc::new(Compiled {
                label: label.to_owned(),
                module,
                entry: entry.to_owned(),
                workgroup,
                bindings: bindings.to_vec(),
                reflected,
                spirv,
            }),
        }
    }

    pub fn label(&self) -> &str {
        &self.compiled.label
    }

    pub fn entry(&self) -> &str {
        &self.compiled.entry
    }

    pub fn workgroup_size(&self) -> u32 {
        self.compiled.workgroup
    }

    pub fn bindings(&self) -> &[BindingSpec] {
        &self.compiled.bindings
    }

    pub fn reflected(&self) -> &[ShaderBinding] {
        &self.compiled.reflected
    }

    pub fn spirv(&self) -> &[u32] {
        &self.compiled.spirv
    }

    pub fn module(&self) -> &Module {
        &self.compiled.module
    }

    pub fn requirements(&self) -> crate::shader::Requirements {
        self.compiled.module.requirements()
    }

    pub fn translate(&self, backend: Backend) -> ShaderTranslation {
        let compiled = &self.compiled;
        match backend {
            Backend::Vulkan => {
                Target::SPIRV.supports(compiled.module.requirements(), compiled.label.as_str());
                ShaderTranslation::Spirv(compiled.spirv.clone())
            }
            Backend::Metal => {
                let source = crate::shader::msl::write(&compiled.module);
                ShaderTranslation::Msl {
                    entry: crate::shader::msl::symbol(&compiled.entry),
                    source,
                }
            }
        }
    }
}
fn dense(bindings: &[BindingSpec], label: &str) {
    let mut sorted = bindings.to_vec();
    sorted.sort_by_key(|binding| (binding.group, binding.binding));
    let mut group = 0;
    let mut slot = 0;
    for binding in sorted {
        if binding.group != group {
            assert_eq!(
                binding.group,
                group + 1,
                "{label} skips a bind group before {}",
                binding.group
            );
            group = binding.group;
            slot = 0;
        }
        assert_eq!(
            binding.binding, slot,
            "{label} skips a binding slot within group {group}"
        );
        slot += 1;
    }
}

struct GraphicsCompiled {
    label: String,
    module: Module,
    vertex: String,
    fragment: String,
    bindings: Vec<BindingSpec>,
    reflected: Vec<ShaderBinding>,
    spirv: Vec<u32>,
}

#[derive(Clone)]
pub struct GraphicsProgram {
    compiled: Arc<GraphicsCompiled>,
}

impl GraphicsProgram {
    pub fn new(label: &str, module: Module, bindings: &[BindingSpec]) -> Self {
        assert!(
            bindings
                .iter()
                .filter(|binding| binding.kind.buffer())
                .count()
                <= METAL_SIZE_BUFFER_SLOT as usize,
            "{label} binds more buffers than a device holds"
        );
        dense(bindings, label);
        let (vertex, fragment) = module
            .graphics()
            .unwrap_or_else(|| panic!("{label} is not a graphics program"));
        let vertex_stage = module.stage_function(Stage::Vertex);
        let fragment_stage = module.stage_function(Stage::Fragment);
        let vertex_name = vertex_stage.name.clone();
        let fragment_name = fragment_stage.name.clone();
        assert!(
            vertex_stage.result.is_none() && fragment_stage.result.is_none(),
            "{label} declares a graphics stage that returns a value"
        );
        assert!(
            fragment_stage.arguments.is_empty(),
            "{label} declares a fragment stage that reads builtin arguments"
        );
        let mut builtins = BTreeSet::new();
        for argument in &vertex_stage.arguments {
            let builtin = argument.builtin.unwrap_or_else(|| {
                panic!("the vertex argument {} is not a builtin", argument.name)
            });
            assert!(
                Stage::Vertex.builtin(builtin),
                "{label} reads the {} builtin outside a compute stage",
                builtin.name()
            );
            assert!(
                builtins.insert(builtin),
                "{label} declares the {} builtin twice",
                builtin.name()
            );
        }
        let reflected = reflect(&module);
        assert_eq!(
            reflected.len(),
            bindings.len(),
            "{label} declares different storage resources from its bindings:\n{}",
            describe(&reflected)
        );
        dense(bindings, label);
        for (declared, spec) in reflected.iter().zip(bindings) {
            assert_eq!(
                (declared.group, declared.binding),
                (spec.group, spec.binding),
                "binding group {} slot {} is missing",
                spec.group,
                spec.binding
            );
            assert_eq!(
                (declared.kind, declared.count),
                (spec.kind, spec.count),
                "binding group {} slot {} has the wrong kind or count",
                spec.group,
                spec.binding
            );
        }
        let vertex_used = module.accessed_globals(vertex);
        let fragment_used = module.accessed_globals(fragment);
        let outputs = locate(&module, &vertex_used, Space::Output, label);
        let inputs = locate(&module, &fragment_used, Space::Input, label);
        let targets = locate(&module, &fragment_used, Space::Output, label);
        assert_eq!(
            outputs, inputs,
            "{label} links the varyings {outputs:?} of its vertex stage to {inputs:?} in its fragment stage"
        );
        assert_eq!(
            targets.len(),
            1,
            "{label} writes {} fragment targets where one color target exists",
            targets.len()
        );
        let position = module
            .globals()
            .iter()
            .filter(|global| global.interface == Some(Interface::Position))
            .collect::<Vec<_>>();
        assert_eq!(
            position.len(),
            1,
            "{label} declares {} clip space positions",
            position.len()
        );
        assert_eq!(
            position[0].space,
            Space::Output,
            "{label} reads the clip space position instead of writing it"
        );
        let position_index = module
            .globals()
            .iter()
            .position(|global| global.interface == Some(Interface::Position))
            .expect("the clip space position is a global") as u32;
        assert!(
            vertex_used.contains(&position_index),
            "{label} never writes the clip space position of its vertex stage"
        );
        assert!(
            matches!(
                module.ty(position[0].ty),
                Type::Vector {
                    scalar: Scalar::F32,
                    length: 4
                }
            ),
            "{label} writes the clip space position as {}",
            element_name(module.ty(position[0].ty))
        );
        let spirv = crate::shader::spirv::write(&module);
        Self {
            compiled: Arc::new(GraphicsCompiled {
                label: label.to_owned(),
                module,
                vertex: vertex_name,
                fragment: fragment_name,
                bindings: bindings.to_vec(),
                reflected,
                spirv,
            }),
        }
    }

    pub fn label(&self) -> &str {
        &self.compiled.label
    }

    pub fn vertex_entry(&self) -> &str {
        &self.compiled.vertex
    }

    pub fn fragment_entry(&self) -> &str {
        &self.compiled.fragment
    }

    pub fn bindings(&self) -> &[BindingSpec] {
        &self.compiled.bindings
    }

    pub fn reflected(&self) -> &[ShaderBinding] {
        &self.compiled.reflected
    }

    pub fn spirv(&self) -> &[u32] {
        &self.compiled.spirv
    }

    pub fn module(&self) -> &Module {
        &self.compiled.module
    }

    pub fn requirements(&self) -> crate::shader::Requirements {
        self.compiled.module.requirements()
    }

    pub fn translate(&self, backend: Backend) -> GraphicsTranslation {
        let compiled = &self.compiled;
        match backend {
            Backend::Vulkan => {
                Target::SPIRV.supports(compiled.module.requirements(), compiled.label.as_str());
                GraphicsTranslation::Spirv {
                    module: compiled.spirv.clone(),
                    vertex: compiled.vertex.clone(),
                    fragment: compiled.fragment.clone(),
                }
            }
            Backend::Metal => GraphicsTranslation::Msl {
                source: crate::shader::msl::write(&compiled.module),
                vertex: crate::shader::msl::symbol(&compiled.vertex),
                fragment: crate::shader::msl::symbol(&compiled.fragment),
            },
        }
    }
}

fn locate(module: &Module, used: &BTreeSet<u32>, space: Space, label: &str) -> Vec<(u32, TypeId)> {
    let mut located = used
        .iter()
        .map(|index| module.global(*index))
        .filter(|global| global.space == space)
        .filter_map(|global| match global.interface {
            Some(Interface::Location(location)) => Some((location, global.ty)),
            Some(Interface::Position) => None,
            None => panic!(
                "{label} reads {} without an interface from its {} stage",
                global.name,
                space.name()
            ),
        })
        .collect::<Vec<_>>();
    located.sort_by_key(|(location, _)| *location);
    for (index, (location, _)) in located.iter().enumerate() {
        assert_eq!(
            *location as usize,
            index,
            "{label} skips or repeats the {} varying location {location}",
            space.name()
        );
    }
    located
}

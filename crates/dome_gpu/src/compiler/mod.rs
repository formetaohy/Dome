use crate::ast;
use crate::layout::{FieldType, RecordLayout};
use crate::shader::{
    Access, Binding, BindingKind, BindingSpec, ComputeProgram, Function, Global, GraphicsProgram,
    Instruction as DeviceInstruction, Interface, Member, Module, Scalar, Space, Stage, TypeId,
    ValueId,
};
use std::collections::{BTreeMap, HashMap};

pub mod device;
mod lower;
mod resource;

pub use resource::{
    AtomicU32, Fvec2, Fvec4, In, Out, Position, Read, ReadWrite, Sampler, SamplerTable, Texture2D,
    TextureTable, Uvec3, Uvec4, Workgroup,
};

pub struct Compiler {
    module: Module,
    records: HashMap<String, TypeId>,
    functions: BTreeMap<String, ast::Function>,
    lowered: HashMap<String, u32>,
    globals: HashMap<String, u32>,
    scalars: HashMap<String, TypeId>,
    constants: HashMap<String, u32>,
    bindings: Vec<BindingSpec>,
}

impl Compiler {
    pub fn empty() -> Self {
        let mut module = Module::new(1);
        let mut scalars = HashMap::new();
        for (name, scalar) in [
            ("u32", Scalar::U32),
            ("i32", Scalar::I32),
            ("f32", Scalar::F32),
            ("bool", Scalar::Bool),
        ] {
            let ty = module.scalar(scalar);
            scalars.insert(name.to_owned(), ty);
        }
        for (name, scalar, length) in [("uvec3", Scalar::U32, 3), ("uvec4", Scalar::U32, 4)] {
            let ty = module.vector(scalar, length);
            scalars.insert(name.to_owned(), ty);
        }
        for (name, length) in [("fvec2", 2), ("fvec4", 4)] {
            let held = module.vector(Scalar::F32, length);
            scalars.insert(name.to_owned(), held);
        }
        scalars.insert("Texture2D".to_owned(), module.image());
        scalars.insert("Sampler".to_owned(), module.sampler());
        Self {
            module,
            records: HashMap::new(),
            functions: BTreeMap::new(),
            lowered: HashMap::new(),
            globals: HashMap::new(),
            scalars,
            constants: HashMap::new(),
            bindings: Vec::new(),
        }
    }

    pub(crate) fn module(&self) -> &Module {
        &self.module
    }

    pub(crate) fn module_mut(&mut self) -> &mut Module {
        &mut self.module
    }

    pub(crate) fn scalar(&self, name: &str) -> TypeId {
        *self
            .scalars
            .get(name)
            .unwrap_or_else(|| panic!("the Rust device type {name} is not declared"))
    }

    pub(crate) fn ty(&mut self, name: &str) -> TypeId {
        if name == "AtomicU32" {
            return self.module.atomic(Scalar::U32);
        }
        if name == "f16" {
            return self.module.scalar(Scalar::F16);
        }
        if let Some(ty) = self.records.get(name) {
            return *ty;
        }
        if let Some(ty) = self.scalars.get(name) {
            return *ty;
        }
        panic!("the Rust device type {name} is not declared")
    }

    pub(crate) fn array(&mut self, element: TypeId, count: u32) -> TypeId {
        self.module.array(element, Some(count))
    }

    pub(crate) fn pointer(&mut self, space: Space, base: TypeId) -> TypeId {
        self.module.pointer(space, base)
    }

    pub(crate) fn define(&mut self, ty: TypeId) -> ValueId {
        self.module.define(ty)
    }

    pub(crate) fn constant_value(&self, name: &str) -> Option<u32> {
        self.constants.get(name).copied()
    }

    pub fn record(&mut self, layout: RecordLayout) {
        assert!(
            !self.records.contains_key(layout.name),
            "a device type is declared twice"
        );
        let members = layout
            .fields
            .iter()
            .map(|field| {
                let ty = match field.ty {
                    FieldType::U32 => self.scalar("u32"),
                    FieldType::I32 => self.scalar("i32"),
                    FieldType::F32 => self.scalar("f32"),
                    FieldType::U32x4 => self.scalar("uvec4"),
                    FieldType::F32x4 => self.scalar("fvec4"),
                };
                Member {
                    name: field.name.to_owned(),
                    ty,
                    offset: field.offset,
                }
            })
            .collect::<Vec<_>>();
        let ty = self.module.structure(layout.name, members, layout.size);
        self.records.insert(layout.name.to_owned(), ty);
    }

    pub fn storage_array(&mut self, name: &str, element: &str, spec: BindingSpec) {
        let element = self.ty(element);
        let ty = self.module.array(element, None);
        self.storage(name, ty, spec);
    }

    pub fn storage_record(&mut self, name: &str, record: &str, spec: BindingSpec) {
        let ty = self.ty(record);
        self.storage(name, ty, spec);
    }

    pub fn texture(&mut self, name: &str, spec: BindingSpec) {
        let ty = self.ty("Texture2D");
        self.uniform_constant(name, ty, spec);
    }

    pub fn sampler(&mut self, name: &str, spec: BindingSpec) {
        let ty = self.ty("Sampler");
        self.uniform_constant(name, ty, spec);
    }

    pub fn texture_table(&mut self, name: &str, spec: BindingSpec) {
        assert_eq!(
            spec.kind,
            BindingKind::SampledImageTable,
            "the texture table {name} binds a sampled image table"
        );
        let element = self.ty("Texture2D");
        let ty = self.module.array(element, Some(spec.count));
        self.uniform_constant_table(name, ty, spec);
    }

    pub fn sampler_table(&mut self, name: &str, spec: BindingSpec) {
        assert_eq!(
            spec.kind,
            BindingKind::SamplerTable,
            "the sampler table {name} binds a sampler table"
        );
        let element = self.ty("Sampler");
        let ty = self.module.array(element, Some(spec.count));
        self.uniform_constant_table(name, ty, spec);
    }

    fn uniform_constant_table(&mut self, name: &str, ty: TypeId, spec: BindingSpec) {
        assert!(spec.count > 0, "a device table {name} holds no entry");
        self.global(name, ty, Space::UniformConstant, spec, Access::Read, false);
    }

    fn uniform_constant(&mut self, name: &str, ty: TypeId, spec: BindingSpec) {
        assert!(
            matches!(
                spec.kind,
                crate::shader::BindingKind::SampledImage | crate::shader::BindingKind::Sampler
            ),
            "a uniform constant {name} reads a texture or a sampler"
        );
        self.global(name, ty, Space::UniformConstant, spec, Access::Read, false);
    }

    fn storage(&mut self, name: &str, ty: TypeId, spec: BindingSpec) {
        self.global(
            name,
            ty,
            Space::Storage,
            spec,
            match spec.kind {
                BindingKind::ReadOnlyStorage => Access::Read,
                BindingKind::ReadWriteStorage => Access::ReadWrite,
                BindingKind::TableStorage => Access::Table,
                BindingKind::SampledImage
                | BindingKind::Sampler
                | BindingKind::SampledImageTable
                | BindingKind::SamplerTable => {
                    panic!("a storage binding reads a texture or a sampler")
                }
            },
            spec.kind.writable(),
        );
    }

    fn global(
        &mut self,
        name: &str,
        ty: TypeId,
        space: Space,
        spec: BindingSpec,
        access: Access,
        coherent: bool,
    ) {
        let expected = self
            .bindings
            .iter()
            .filter(|binding| binding.group == spec.group)
            .count() as u32;
        assert_eq!(
            spec.binding, expected,
            "binding slots are dense within their group"
        );
        let index = self.module.add_global(Global {
            name: name.to_owned(),
            ty,
            space,
            binding: Some(Binding {
                group: spec.group,
                binding: spec.binding,
            }),
            access,
            coherent,
            interface: None,
        });
        assert!(self.globals.insert(name.to_owned(), index).is_none());
        self.bindings.push(spec);
    }

    pub fn workgroup(&mut self, name: &str, element: &str, count: u32) {
        let element = self.ty(element);
        let ty = self.module.array(element, Some(count));
        let index = self.module.add_global(Global {
            name: name.to_owned(),
            ty,
            space: Space::WorkGroup,
            binding: None,
            access: Access::ReadWrite,
            coherent: false,
            interface: None,
        });
        assert!(self.globals.insert(name.to_owned(), index).is_none());
    }

    pub fn workgroup_bytes(&mut self, name: &str, element: &str, bytes: u64) {
        let ty = self.ty(element);
        let width = u64::from(self.module.size(ty));
        assert!(
            bytes.is_multiple_of(width),
            "a workgroup array of {name} holds {bytes} bytes, and {element} of it spans {width}",
        );
        let count = u32::try_from(bytes / width).unwrap_or_else(|_| {
            panic!(
                "a workgroup array of {name} spans {} elements",
                bytes / width
            )
        });
        self.workgroup(name, element, count);
    }

    pub fn varying(&mut self, name: &str, element: &str, space: Space, interface: Interface) {
        assert!(
            space.interface(),
            "a varying of {name} lives in input or output memory"
        );
        let ty = self.ty(element);
        let index = self.module.add_global(Global {
            name: name.to_owned(),
            ty,
            space,
            binding: None,
            access: if space == Space::Input {
                Access::Read
            } else {
                Access::ReadWrite
            },
            coherent: false,
            interface: Some(interface),
        });
        assert!(
            self.globals.insert(name.to_owned(), index).is_none(),
            "the Rust device name {name} is declared twice"
        );
    }

    pub(crate) fn declares_global(&self, name: &str) -> bool {
        self.globals.contains_key(name)
    }

    pub(crate) fn workgroup_global(&self, name: &str) -> u32 {
        *self
            .globals
            .get(name)
            .unwrap_or_else(|| panic!("the Rust device name {name} is not declared"))
    }

    pub fn declared_workgroup_bytes(&self) -> u64 {
        self.module
            .globals()
            .iter()
            .filter(|global| global.space == Space::WorkGroup)
            .map(|global| u64::from(self.module.size(global.ty)))
            .sum()
    }

    pub fn constant(&mut self, name: &str, value: u32) {
        assert!(
            self.constants.insert(name.to_owned(), value).is_none(),
            "{name} is defined twice"
        );
    }

    pub fn function(&mut self, function: ast::Function) {
        let name = function.name.clone();
        assert!(
            self.functions.insert(name.clone(), function).is_none(),
            "the Rust device function {name} is declared twice"
        );
    }

    pub fn select(&mut self, source: &str, name: &str) {
        let mut function = self
            .functions
            .get(source)
            .unwrap_or_else(|| panic!("Rust device function {source} does not exist"))
            .clone();
        function.name = name.to_owned();
        self.function(function);
    }

    pub fn insert_case(&mut self, name: &str, arm: ast::Arm) {
        let function = self
            .functions
            .get_mut(name)
            .unwrap_or_else(|| panic!("device dispatcher {name} does not exist"));
        assert_eq!(
            ast::match_count(&function.body),
            1,
            "device dispatcher {name} has exactly one match"
        );
        let cases =
            ast::match_cases(&mut function.body).expect("a device dispatcher has one match");
        assert!(
            !cases.iter().any(|present| present.pattern == arm.pattern),
            "device dispatcher {name} has a duplicate case"
        );
        let fallback = cases
            .iter()
            .position(|present| matches!(present.pattern, ast::Pattern::Default))
            .unwrap_or(cases.len());
        cases.insert(fallback, arm);
    }

    pub fn retain_cases(&mut self, name: &str, cases: &[String]) {
        let function = self
            .functions
            .get_mut(name)
            .unwrap_or_else(|| panic!("Rust device dispatcher {name} does not exist"));
        let mut found = false;
        for statement in &mut function.body {
            if let ast::Statement::Match { arms, .. } = statement {
                assert!(!found, "a specialized dispatcher has one Rust match");
                found = true;
                arms.retain(|arm| match &arm.pattern {
                    ast::Pattern::Default => true,
                    ast::Pattern::Constant(path) => cases.contains(path),
                    _ => panic!("a device dispatcher requires qualified Rust constants"),
                });
            }
        }
        assert!(found, "{name} does not dispatch a Rust match");
    }

    pub fn specialize(&mut self, template: &str, name: &str, constants: &[(&str, u32)]) {
        let mut function = self
            .functions
            .get(template)
            .unwrap_or_else(|| panic!("the Rust device template {template} does not exist"))
            .clone();
        function.name = name.to_owned();
        let suffix = name
            .rsplit_once('_')
            .expect("a specialized name has a suffix")
            .1;
        let constants = constants
            .iter()
            .map(|(name, value)| (name.to_string(), *value))
            .collect::<HashMap<_, _>>();
        for statement in &mut function.body {
            statement.specialize(&constants, suffix);
        }
        self.function(function);
    }

    pub fn finish(mut self, label: &str, entry: &str, workgroup: u32) -> ComputeProgram {
        assert!(
            self.functions.contains_key(entry),
            "{label} does not define the Rust device entry {entry}"
        );
        assert!(workgroup > 0, "a device program runs no thread");
        self.module.set_workgroup(workgroup);
        self.declare_all(&[entry]);
        self.lower_all(&[(entry, Some(Stage::Compute))]);
        let index = *self
            .lowered
            .get(entry)
            .expect("the device entry is declared");
        self.module.set_entry(index);
        self.module.verify();
        ComputeProgram::new(label, self.module, entry, &self.bindings)
    }

    pub fn finish_graphics(mut self, label: &str, vertex: &str, fragment: &str) -> GraphicsProgram {
        for entry in [vertex, fragment] {
            assert!(
                self.functions.contains_key(entry),
                "{label} does not define the Rust device stage {entry}"
            );
        }
        self.declare_all(&[vertex, fragment]);
        self.lower_all(&[
            (vertex, Some(Stage::Vertex)),
            (fragment, Some(Stage::Fragment)),
        ]);
        let vertex_index = *self
            .lowered
            .get(vertex)
            .expect("the vertex stage is declared");
        let fragment_index = *self
            .lowered
            .get(fragment)
            .expect("the fragment stage is declared");
        self.module.set_graphics(vertex_index, fragment_index);
        self.module.verify();
        GraphicsProgram::new(label, self.module, &self.bindings)
    }

    fn declare_all(&mut self, entries: &[&str]) {
        let order = self.reachable(entries);
        for name in order {
            let source = self
                .functions
                .get(&name)
                .unwrap_or_else(|| panic!("Rust device function {name} is not declared"))
                .clone();
            let index = self.module.declare(Function {
                name: source.name.clone(),
                arguments: Vec::new(),
                result: None,
                locals: Vec::new(),
                body: Vec::new(),
            });
            self.lowered.insert(name, index);
        }
    }

    fn lower_all(&mut self, entries: &[(&str, Option<Stage>)]) {
        let stages = self.stages(entries);
        let mut order = self.lowered.iter().collect::<Vec<_>>();
        order.sort_by_key(|(_, index)| **index);
        let order = order
            .into_iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        for name in order {
            let source = self.functions[&name].clone();
            let stage = stages.get(&name).copied().flatten();
            let entry = entries.iter().any(|(entry, _)| *entry == name);
            let lowered =
                lower::FunctionLower::new(self, &source, stage, entry).lower(&source.body);
            let index = self.lowered[&name];
            *self.module.function(index) = lowered;
        }
    }

    fn stages(&self, entries: &[(&str, Option<Stage>)]) -> HashMap<String, Option<Stage>> {
        let names = entries.iter().map(|(entry, _)| *entry).collect::<Vec<_>>();
        let order = self.reachable(&names);
        let mut stages = entries
            .iter()
            .map(|(entry, stage)| ((*entry).to_owned(), *stage))
            .collect::<HashMap<_, _>>();
        for name in order.iter().rev() {
            let Some(stage) = stages.get(name).copied().flatten() else {
                continue;
            };
            for callee in calls(&self.functions[name].body) {
                match stages.get_mut(&callee) {
                    Some(present) if *present == Some(stage) => {}
                    Some(present) => *present = None,
                    None => {
                        stages.insert(callee, Some(stage));
                    }
                }
            }
        }
        stages
    }

    fn reachable(&self, entries: &[&str]) -> Vec<String> {
        fn visit(
            name: &str,
            functions: &BTreeMap<String, ast::Function>,
            path: &mut Vec<String>,
            order: &mut Vec<String>,
        ) {
            if order.iter().any(|present| present == name) {
                return;
            }
            assert!(
                !path.iter().any(|present| present == name),
                "recursive Rust device functions are not supported: {name}"
            );
            path.push(name.to_owned());
            let function = functions
                .get(name)
                .unwrap_or_else(|| panic!("Rust device function {name} is not declared"));
            for callee in calls(&function.body) {
                if functions.contains_key(&callee) {
                    visit(&callee, functions, path, order);
                }
            }
            path.pop();
            order.push(name.to_owned());
        }
        let mut order = Vec::new();
        for entry in entries {
            visit(entry, &self.functions, &mut Vec::new(), &mut order);
        }
        order
    }

    pub(crate) fn lower_callee(&mut self, name: &str) -> u32 {
        *self
            .lowered
            .get(name)
            .unwrap_or_else(|| panic!("Rust device function {name} is not declared"))
    }

    pub(crate) fn returns_value(&self, name: &str) -> bool {
        self.functions
            .get(name)
            .is_some_and(|function| function.result.is_some())
    }
}

fn calls(body: &[ast::Statement]) -> Vec<String> {
    let mut names = Vec::new();
    for statement in body {
        match statement {
            ast::Statement::Expression(expr) => calls_of_expression(expr, &mut names),
            ast::Statement::Let { value, .. } => calls_of_expression(value, &mut names),
            ast::Statement::Assign { place, value, .. } => {
                calls_of_expression(place, &mut names);
                calls_of_expression(value, &mut names);
            }
            ast::Statement::If {
                condition,
                accept,
                reject,
            } => {
                calls_of_expression(condition, &mut names);
                names.extend(calls(accept));
                names.extend(calls(reject));
            }
            ast::Statement::Match { selector, arms } => {
                calls_of_expression(selector, &mut names);
                for arm in arms {
                    names.extend(calls(&arm.body));
                }
            }
            ast::Statement::For { iterator, body, .. } => {
                calls_of_expression(iterator, &mut names);
                names.extend(calls(body));
            }
            ast::Statement::While { condition, body } => {
                calls_of_expression(condition, &mut names);
                names.extend(calls(body));
            }
            ast::Statement::Loop(body) | ast::Statement::Block(body) => {
                names.extend(calls(body));
            }
            ast::Statement::Return(value) => {
                if let Some(value) = value {
                    calls_of_expression(value, &mut names);
                }
            }
            ast::Statement::Break | ast::Statement::Continue => {}
        }
    }
    names
}

fn calls_of_expression(expression: &ast::Expression, names: &mut Vec<String>) {
    use ast::Expression as E;
    match expression {
        E::Call { name, arguments } => {
            names.push(name.clone());
            for argument in arguments {
                calls_of_expression(argument, names);
            }
        }
        E::Field { base, .. } | E::Unary { value: base, .. } | E::Cast { value: base, .. } => {
            calls_of_expression(base, names);
        }
        E::Reference(base) => calls_of_expression(base, names),
        E::Index { base, index } => {
            calls_of_expression(base, names);
            calls_of_expression(index, names);
        }
        E::Binary { left, right, .. } => {
            calls_of_expression(left, names);
            calls_of_expression(right, names);
        }
        E::Repeat { value, length } => {
            calls_of_expression(value, names);
            calls_of_expression(length, names);
        }
        E::Integer { .. } | E::Float(_) | E::Bool(_) | E::Name(_) => {}
    }
}

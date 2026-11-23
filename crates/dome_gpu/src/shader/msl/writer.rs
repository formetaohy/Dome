use super::source::Source;
use super::walk::{argument_value, collect, nested, terminates, usage};
use crate::shader::instruction::leaves_a_loop;
use crate::shader::{
    Address, AtomicOp, Barrier, Constant, DerivativeOp, Function, Instruction, Interface, MathFun,
    MatrixLayout, Module, Scalar, Space, Stage, StageEntry, Target, Type, TypeId, UnaryOp, ValueId,
};
use std::collections::{HashMap, HashSet};

pub fn write(module: &Module) -> String {
    Target::MSL.supports(module.requirements(), "the Metal device program");
    Writer::new(module).run()
}

pub fn symbol(identifier: &str) -> String {
    format!("d_{identifier}")
}

fn scalar_name(scalar: Scalar) -> &'static str {
    match scalar {
        Scalar::U32 => "uint",
        Scalar::I32 => "int",
        Scalar::F32 => "float",
        Scalar::F16 => "half",
        Scalar::Bool => "bool",
    }
}

struct Writer<'m> {
    module: &'m Module,
    out: Source,
    declarations: HashMap<ValueId, &'m Instruction>,
    names: HashMap<ValueId, String>,
    loops: Vec<String>,
    used: Vec<Vec<u32>>,
    stage: Option<Stage>,
}

impl<'m> Writer<'m> {
    fn new(module: &'m Module) -> Self {
        let mut declarations = HashMap::new();
        for function in module.functions() {
            collect(&function.body, &mut declarations);
        }
        let used = usage(module);
        Self {
            module,
            out: Source::new(),
            declarations,
            names: HashMap::new(),
            loops: Vec::new(),
            used,
            stage: None,
        }
    }

    fn run(mut self) -> String {
        self.out.line("// language: metal2.3");
        self.out.line("#include <metal_stdlib>");
        self.out.line("#include <simd/simd.h>");
        self.out.line("");
        self.out.line("using namespace metal;");
        self.out.line("");
        self.layouts();
        self.structures();
        self.tables();
        if let Some((vertex, _)) = self.module.graphics() {
            self.varyings(vertex);
        }
        for index in 0..self.module.functions().len() as u32 {
            if self.module.is_entry(index) {
                continue;
            }
            let function = &self.module.functions()[index as usize];
            let prototype = format!("{} {}(", self.result_name(function), symbol(&function.name));
            let parameters = self.parameters(index);
            self.out.line(format!("{prototype}{parameters});"));
        }
        self.out.line("");
        for index in 0..self.module.functions().len() as u32 {
            if self.module.is_entry(index) {
                continue;
            }
            self.function(index);
        }
        for entry in self.module.stages().to_vec() {
            self.entry(entry);
        }
        self.out.finish()
    }

    fn result_name(&self, function: &Function) -> String {
        match function.result {
            Some(ty) => self.value_type(ty),
            None => "void".to_owned(),
        }
    }

    fn parameters(&self, index: u32) -> String {
        let function = &self.module.functions()[index as usize];
        let mut parameters = function
            .arguments
            .iter()
            .map(|argument| {
                format!(
                    "{} {}",
                    self.value_type(argument.ty),
                    symbol(&argument.name)
                )
            })
            .collect::<Vec<_>>();
        for global in &self.used[index as usize] {
            let declared = self.module.global(*global);
            if !declared.space.interface() && declared.space != Space::WorkGroup {
                parameters.push(self.global_parameter(*global));
            }
        }
        parameters.join(", ")
    }

    fn arguments(&self, index: u32, arguments: &[ValueId]) -> String {
        let mut rendered = arguments
            .iter()
            .map(|argument| self.value(*argument))
            .collect::<Vec<_>>();
        for global in &self.used[index as usize] {
            let declared = self.module.global(*global);
            if !declared.space.interface() && declared.space != Space::WorkGroup {
                rendered.push(symbol(&declared.name));
            }
        }
        rendered.join(", ")
    }

    fn global_parameter(&self, index: u32) -> String {
        let global = self.module.global(index);
        let global_name = symbol(&global.name);
        match global.space {
            Space::Storage => {
                let element = self.element_type(global.ty);
                let pointee = self.pointee_type(element);
                let qualifier = if global.access.writable() {
                    "device"
                } else {
                    "device const"
                };
                format!("{qualifier} {pointee}* {global_name}")
            }
            Space::WorkGroup => {
                let element = match self.module.ty(global.ty) {
                    Type::Array { element, .. } => *element,
                    _ => panic!(
                        "the Metal workgroup variable {} is {}",
                        global.name,
                        crate::shader::element_name(self.module.ty(global.ty))
                    ),
                };
                let pointee = self.pointee_type(element);
                format!("threadgroup {pointee}* {global_name}")
            }
            Space::UniformConstant => match self.module.ty(global.ty) {
                Type::Array { element, .. } => match self.module.ty(*element) {
                    Type::Image | Type::Sampler => {
                        format!("constant {}& {global_name}", table_name(&global.name))
                    }
                    other => panic!(
                        "the Metal uniform constant table {} holds {}",
                        global.name,
                        crate::shader::element_name(other)
                    ),
                },
                _ => format!("{} {global_name}", self.value_type(global.ty)),
            },
            Space::Input | Space::Output => panic!(
                "the Metal interface variable {} is a stage member",
                global.name
            ),
            Space::Function => panic!("a device module global lives in function memory"),
        }
    }

    fn tables(&mut self) {
        for global in self.module.globals() {
            if global.space != Space::UniformConstant {
                continue;
            }
            let Type::Array {
                element,
                count: Some(count),
            } = self.module.ty(global.ty)
            else {
                continue;
            };
            let declared = match self.module.ty(*element) {
                Type::Image => "texture2d<float>",
                Type::Sampler => "sampler",
                other => panic!(
                    "the Metal uniform constant table {} holds {}",
                    global.name,
                    crate::shader::element_name(other)
                ),
            };
            self.out
                .open(format!("struct {}", table_name(&global.name)));
            self.out
                .line(format!("array<{declared}, {count}> values [[id(0)]];"));
            self.out.close("};");
            self.out.line("");
        }
    }

    fn pointee_type(&self, ty: TypeId) -> String {
        match self.module.ty(ty) {
            Type::Atomic(scalar) => format!("atomic_{}", scalar_name(*scalar)),
            _ => self.value_type(ty),
        }
    }

    fn value_type(&self, ty: TypeId) -> String {
        match self.module.ty(ty) {
            Type::Scalar(scalar) => scalar_name(*scalar).to_owned(),
            Type::Vector { scalar, length } => format!("{}{length}", scalar_name(*scalar)),
            Type::Array { element, count } => {
                let count = count.expect("a device array holds a fixed number of elements");
                format!("{}[{count}]", self.value_type(*element))
            }
            Type::Struct { name, .. } => name.clone(),
            Type::Atomic(scalar) => format!("atomic_{}", scalar_name(*scalar)),
            Type::Image => "texture2d<float>".to_owned(),
            Type::Sampler => "sampler".to_owned(),
            Type::Pointer { .. } => panic!("a Metal shader carries no device pointer type"),
            Type::CooperativeMatrix {
                scalar,
                rows,
                columns,
                ..
            } => format!(
                "simdgroup_matrix<{}, {rows}, {columns}>",
                scalar_name(*scalar)
            ),
        }
    }

    fn element_type(&self, ty: TypeId) -> TypeId {
        match self.module.ty(ty) {
            Type::Array { element, .. } => *element,
            _ => ty,
        }
    }

    fn layouts(&self) {
        for ty in self.module.type_ids() {
            let Type::Struct {
                name: structure,
                members,
                span,
            } = self.module.ty(ty)
            else {
                continue;
            };
            let mut offset = 0u32;
            let mut alignment = 1u32;
            for member in members {
                let align = self.module.alignment(member.ty).clamp(1, 16);
                offset = offset.div_ceil(align) * align;
                assert_eq!(
                    offset, member.offset,
                    "the device struct {structure} member {} sits at {} while Metal packs it at {offset}",
                    member.name, member.offset,
                );
                offset += self.module.size(member.ty);
                alignment = alignment.max(align);
            }
            offset = offset.div_ceil(alignment) * alignment;
            assert_eq!(
                offset, *span,
                "the device struct {structure} spans {span} bytes while Metal packs it into {offset}",
            );
        }
    }

    fn structures(&mut self) {
        for ty in self.module.type_ids() {
            let Type::Struct {
                name: structure,
                members,
                ..
            } = self.module.ty(ty)
            else {
                continue;
            };
            self.out.open(format!("struct {structure}"));
            for member in members {
                let ty = self.value_type(member.ty);
                let member = symbol(&member.name);
                self.out.line(format!("{ty} {member};"));
            }
            self.out.close("};");
            self.out.line("");
        }
    }

    fn function(&mut self, index: u32) {
        let function = &self.module.functions()[index as usize];
        let result = self.result_name(function);
        let parameters = self.parameters(index);
        self.out
            .open(format!("{result} {}({parameters})", symbol(&function.name)));
        self.locals(function);
        self.prepare(function);
        self.block(&function.body);
        self.out.close("}");
        self.out.line("");
    }

    fn entry(&mut self, entry: StageEntry) {
        let function = &self.module.functions()[entry.function as usize];
        match entry.stage {
            Stage::Compute => self.compute(function),
            Stage::Vertex => self.vertex(entry.function, function),
            Stage::Fragment => self.fragment(entry.function, function),
        }
    }

    fn compute(&mut self, entry: &'m Function) {
        let index = self.module.compute().expect("a compute program");
        self.kernel(index, entry, "kernel void");
    }

    fn kernel(&mut self, index: u32, entry: &'m Function, declaration: &str) {
        let mut parameters = entry
            .arguments
            .iter()
            .map(|argument| {
                let attribute = match argument.builtin.expect("an entry argument is a builtin") {
                    crate::shader::BuiltIn::LocalInvocationIndex => {
                        "[[thread_index_in_threadgroup]]"
                    }
                    crate::shader::BuiltIn::WorkGroupId => "[[threadgroup_position_in_grid]]",
                    crate::shader::BuiltIn::GlobalInvocationId => "[[thread_position_in_grid]]",
                    other => panic!("a compute stage reads the {} builtin", other.name()),
                };
                format!(
                    "{} {} {attribute}",
                    self.value_type(argument.ty),
                    symbol(&argument.name)
                )
            })
            .collect::<Vec<_>>();
        parameters.extend(self.resource_parameters(index));
        let entry_name = symbol(&entry.name);
        self.out.line(format!(
            "[[max_total_threads_per_threadgroup({})]] {declaration} {entry_name}(",
            self.module.workgroup_size()
        ));
        for parameter in &parameters {
            self.out.line(format!("    {parameter},"));
        }
        self.out.line(") {");
        self.out.enter();
        for global in &self.used[index as usize] {
            let declared = self.module.global(*global);
            if declared.space != Space::WorkGroup {
                continue;
            }
            let (element, count) = match self.module.ty(declared.ty) {
                Type::Array {
                    element,
                    count: Some(count),
                } => (*element, *count),
                _ => panic!(
                    "the Metal workgroup variable {} is not an array",
                    declared.name
                ),
            };
            let declared_type = self.pointee_type(element);
            self.out.line(format!(
                "threadgroup {declared_type} {}[{count}];",
                symbol(&declared.name)
            ));
        }
        self.locals(entry);
        self.prepare(entry);
        self.block(&entry.body);
        self.out.close("}");
    }

    fn vertex(&mut self, index: u32, entry: &'m Function) {
        let declared = "d_varyings";
        let mut parameters = entry
            .arguments
            .iter()
            .map(|argument| {
                let attribute = match argument.builtin.expect("an entry argument is a builtin") {
                    crate::shader::BuiltIn::VertexIndex => "[[vertex_id]]",
                    crate::shader::BuiltIn::InstanceIndex => "[[instance_id]]",
                    other => panic!("a vertex stage reads the {} builtin", other.name()),
                };
                format!(
                    "{} {} {attribute}",
                    self.value_type(argument.ty),
                    symbol(&argument.name)
                )
            })
            .collect::<Vec<_>>();
        parameters.extend(self.resource_parameters(index));
        self.out
            .line(format!("vertex {declared} {}(", symbol(&entry.name)));
        for parameter in &parameters {
            self.out.line(format!("    {parameter},"));
        }
        self.out.line(") {");
        self.out.enter();
        self.out.line(format!("{declared} d_out = {{}};"));
        self.graphics_body(entry, Stage::Vertex);
        self.out.close("}");
        self.out.line("");
    }

    fn fragment(&mut self, index: u32, entry: &'m Function) {
        let outputs = self.interfaced(index, Space::Output);
        let declared = "d_fragment_output";
        self.interface_struct(declared, &outputs, Stage::Fragment, Space::Output);
        let mut parameters = vec!["d_varyings d_in [[stage_in]]".to_owned()];
        parameters.extend(self.resource_parameters(index));
        self.out
            .line(format!("fragment {declared} {}(", symbol(&entry.name)));
        for parameter in &parameters {
            self.out.line(format!("    {parameter},"));
        }
        self.out.line(") {");
        self.out.enter();
        self.out.line(format!("{declared} d_out = {{}};"));
        self.graphics_body(entry, Stage::Fragment);
        self.out.close("}");
        self.out.line("");
    }

    fn graphics_body(&mut self, entry: &'m Function, stage: Stage) {
        self.stage = Some(stage);
        self.locals(entry);
        self.prepare(entry);
        self.block(&entry.body);
        if !terminates(&entry.body) {
            self.out.line("return d_out;");
        }
        self.stage = None;
    }

    fn resource_parameters(&self, index: u32) -> Vec<String> {
        self.used[index as usize]
            .iter()
            .filter(|global| {
                let declared = self.module.global(**global);
                !declared.space.interface() && declared.space != Space::WorkGroup
            })
            .map(|global| {
                let (class, slot) = self.resource_slot(*global);
                format!("{} [[{class}({slot})]]", self.global_parameter(*global))
            })
            .collect()
    }

    fn resource_slot(&self, index: u32) -> (&'static str, u32) {
        let class = match self.module.global(index).space {
            Space::Storage => "buffer",
            Space::UniformConstant => match self.module.ty(self.module.global(index).ty) {
                Type::Image => "texture",
                Type::Sampler => "sampler",
                Type::Array { element, .. } => match self.module.ty(*element) {
                    Type::Image | Type::Sampler => "buffer",
                    other => panic!(
                        "the Metal uniform constant table {} holds {}",
                        self.module.global(index).name,
                        crate::shader::element_name(other)
                    ),
                },
                other => panic!(
                    "the Metal uniform constant {} is {}",
                    self.module.global(index).name,
                    crate::shader::element_name(other)
                ),
            },
            other => panic!(
                "the Metal resource {} lives in {} memory",
                self.module.global(index).name,
                other.name()
            ),
        };
        let slot = self
            .module
            .globals()
            .iter()
            .take(index as usize)
            .filter(|global| match (global.space, self.module.ty(global.ty)) {
                (Space::Storage, _) => class == "buffer",
                (Space::UniformConstant, Type::Image) => class == "texture",
                (Space::UniformConstant, Type::Sampler) => class == "sampler",
                (Space::UniformConstant, Type::Array { element, .. }) => {
                    class == "buffer"
                        && matches!(self.module.ty(*element), Type::Image | Type::Sampler)
                }
                _ => false,
            })
            .count() as u32;
        (class, slot)
    }

    fn varyings(&mut self, vertex: u32) {
        let position =
            self.module
                .globals()
                .iter()
                .position(|global| global.interface == Some(Interface::Position))
                .expect("a graphics program writes the clip space position") as u32;
        let mut members = vec![position];
        members.extend(
            self.interfaced(vertex, Space::Output)
                .into_iter()
                .filter(|index| self.module.global(*index).interface != Some(Interface::Position)),
        );
        self.interface_struct("d_varyings", &members, Stage::Vertex, Space::Output);
    }

    fn interfaced(&self, index: u32, space: Space) -> Vec<u32> {
        self.used[index as usize]
            .iter()
            .copied()
            .filter(|global| self.module.global(*global).space == space)
            .collect()
    }

    fn varying_member(&self, global: &crate::shader::Global) -> String {
        if global.space == Space::Output {
            return symbol(&global.name);
        }
        let Some(Interface::Location(location)) = global.interface else {
            panic!("the input varying {} has no location", global.name);
        };
        let output = self
            .module
            .globals()
            .iter()
            .find(|output| output.interface == Some(Interface::Location(location)))
            .unwrap_or_else(|| {
                panic!(
                    "no vertex stage output carries the varying location {location} of {}",
                    global.name
                )
            });
        symbol(&output.name)
    }

    fn interface_struct(&mut self, name: &str, members: &[u32], stage: Stage, space: Space) {
        self.out.open(format!("struct {name}"));
        for index in members {
            let global = self.module.global(*index);
            let declared = self.value_type(global.ty);
            let member = symbol(&global.name);
            let interface = global
                .interface
                .unwrap_or_else(|| panic!("the interface member {} has no place", global.name));
            let flat = matches!(
                self.module.ty(global.ty),
                Type::Scalar(Scalar::U32 | Scalar::I32)
                    | Type::Vector {
                        scalar: Scalar::U32 | Scalar::I32,
                        ..
                    }
            );
            let attribute = match (stage, space, interface) {
                (_, _, Interface::Position) => " [[position]]".to_owned(),
                (Stage::Fragment, Space::Output, Interface::Location(location)) => {
                    format!(" [[color({location})]]")
                }
                (_, _, Interface::Location(location)) => {
                    if flat {
                        format!(" [[user(locn{location}), flat]]")
                    } else {
                        format!(" [[user(locn{location})]]")
                    }
                }
            };
            self.out.line(format!("{declared} {member}{attribute};"));
        }
        self.out.close("};");
        self.out.line("");
    }

    fn locals(&mut self, function: &Function) {
        let names = local_names(self.module, function);
        for (local, name) in function.locals.iter().zip(&names) {
            let declared = self.value_type(local.ty);
            self.out.line(format!("{declared} {name};"));
        }
        self.declare_values(&function.body);
    }

    fn prepare(&mut self, function: &'m Function) {
        let mut names = HashMap::new();
        for (position, argument) in function.arguments.iter().enumerate() {
            if let Some(value) = argument_value(&function.body, position) {
                names.insert(value, symbol(&argument.name));
            }
        }
        let locals = local_names(self.module, function);
        self.addresses(&locals, &function.body, &mut names);
        self.resource_names(&function.body, &mut names);
        self.names = names;
    }

    fn addresses(
        &self,
        locals: &[String],
        body: &[Instruction],
        names: &mut HashMap<ValueId, String>,
    ) {
        for instruction in body {
            match instruction {
                Instruction::Address {
                    address: Address::Local(index),
                    result,
                } => {
                    names.insert(*result, locals[*index as usize].clone());
                }
                Instruction::Address {
                    address: Address::Global(index),
                    result,
                } => {
                    names.insert(*result, self.global_place(*index));
                }
                _ => {}
            }
            nested(instruction, &mut |block| {
                self.addresses(locals, block, names)
            })
        }
    }

    fn resource_names(&self, body: &[Instruction], names: &mut HashMap<ValueId, String>) {
        for instruction in body {
            if let Instruction::Load { pointer, result } = instruction
                && self.resource(self.module.value_ty(*result))
            {
                names.insert(*result, self.place_value(*pointer));
            }
            nested(instruction, &mut |block| self.resource_names(block, names));
        }
    }

    fn resource(&self, ty: TypeId) -> bool {
        matches!(self.module.ty(ty), Type::Image | Type::Sampler)
    }

    fn global_place(&self, index: u32) -> String {
        let global = self.module.global(index);
        if global.space.interface() {
            let holder = match global.space {
                Space::Input => "d_in",
                Space::Output => "d_out",
                _ => unreachable!("an interface variable lives in input or output memory"),
            };
            return format!("{holder}.{}", self.varying_member(global));
        }
        let named = symbol(&global.name);
        if let Type::Array { element, .. } = self.module.ty(global.ty)
            && matches!(self.module.ty(*element), Type::Image | Type::Sampler)
        {
            return format!("{named}.values");
        }
        if matches!(self.module.ty(global.ty), Type::Array { .. })
            || global.space == Space::UniformConstant
        {
            named
        } else {
            format!("{named}[0]")
        }
    }

    fn declare_values(&mut self, body: &[Instruction]) {
        for instruction in body {
            if let Some(result) = instruction.result()
                && self.hoisted(instruction)
            {
                let declared = self.value_type(self.module.value_ty(result));
                self.out.line(format!("{declared} d_v{};", result.index()));
            }
            match instruction {
                Instruction::If { accept, reject, .. } => {
                    self.declare_values(accept);
                    self.declare_values(reject);
                }
                Instruction::Switch { cases, default, .. } => {
                    for (_, block) in cases {
                        self.declare_values(block);
                    }
                    self.declare_values(default);
                }
                Instruction::Loop { body, continuing } => {
                    self.declare_values(body);
                    self.declare_values(continuing);
                }
                Instruction::Block(block) => self.declare_values(block),
                _ => {}
            }
        }
    }

    fn hoisted(&self, instruction: &Instruction) -> bool {
        match instruction {
            Instruction::Load { result, .. } if self.resource(self.module.value_ty(*result)) => {
                false
            }
            Instruction::Argument { .. } | Instruction::Address { .. } => false,
            Instruction::Access { result, .. } | Instruction::AccessIndex { result, .. } => {
                !matches!(
                    self.module.ty(self.module.value_ty(*result)),
                    Type::Pointer { .. }
                )
            }
            other => other.result().is_some(),
        }
    }

    fn block(&mut self, body: &[Instruction]) {
        for instruction in body {
            self.instruction(instruction);
        }
    }

    fn instruction(&mut self, instruction: &Instruction) {
        match instruction {
            Instruction::Argument { .. } | Instruction::Address { .. } => {}
            Instruction::Access { result, .. } | Instruction::AccessIndex { result, .. } => {
                if self.hoisted(instruction) {
                    let target = self.target(*result);
                    let text = self.place(instruction);
                    self.out.line(format!("{target} = {text};"));
                }
            }
            Instruction::Sample {
                image,
                sampler,
                coordinates,
                level,
                result,
            } => {
                let target = self.target(*result);
                let image = self.value(*image);
                let sampler = self.value(*sampler);
                let coordinates = self.value(*coordinates);
                let text = match level {
                    None => format!("{image}.sample({sampler}, {coordinates})"),
                    Some(level) => {
                        let level = self.value(*level);
                        format!("{image}.sample({sampler}, {coordinates}, level({level}))")
                    }
                };
                self.out.line(format!("{target} = {text};"));
            }
            Instruction::Load { pointer, result } => {
                if self.resource(self.module.value_ty(*result)) {
                    return;
                }
                let target = self.target(*result);
                let text = self.place_value(*pointer);
                if self.is_atomic_pointer(*pointer) {
                    self.out.line(format!(
                        "{target} = atomic_load_explicit(&{text}, memory_order_relaxed);"
                    ));
                } else {
                    self.out.line(format!("{target} = {text};"));
                }
            }
            Instruction::Store { pointer, value } => {
                let text = self.place_value(*pointer);
                let value = self.value(*value);
                if self.is_atomic_pointer(*pointer) {
                    self.out.line(format!(
                        "atomic_store_explicit(&{text}, {value}, memory_order_relaxed);"
                    ));
                } else {
                    self.out.line(format!("{text} = {value};"));
                }
            }
            Instruction::Unary { op, value, result } => {
                let target = self.target(*result);
                let value = self.value(*value);
                let text = match op {
                    UnaryOp::Negate => format!("(-{value})"),
                    UnaryOp::LogicalNot => format!("(!{value})"),
                    UnaryOp::BitwiseNot => format!("(~{value})"),
                };
                self.out.line(format!("{target} = {text};"));
            }
            Instruction::Derivative { op, value, result } => {
                let target = self.target(*result);
                let value = self.value(*value);
                let text = match op {
                    DerivativeOp::X => format!("dfdx({value})"),
                    DerivativeOp::Y => format!("dfdy({value})"),
                };
                self.out.line(format!("{target} = {text};"));
            }
            Instruction::Binary {
                op,
                left,
                right,
                result,
            } => {
                let target = self.target(*result);
                let left = self.value(*left);
                let right = self.value(*right);
                let text = format!("({left} {} {right})", op.name());
                self.out.line(format!("{target} = {text};"));
            }
            Instruction::Select {
                condition,
                accept,
                reject,
                result,
            } => {
                let target = self.target(*result);
                let condition = self.value(*condition);
                let accept = self.value(*accept);
                let reject = self.value(*reject);
                self.out
                    .line(format!("{target} = ({condition} ? {accept} : {reject});"));
            }
            Instruction::Convert { value, result } => {
                let target = self.target(*result);
                let declared = self.value_type(self.module.value_ty(*result));
                let value = self.value(*value);
                self.out.line(format!("{target} = {declared}({value});"));
            }
            Instruction::Bitcast { value, result } => {
                let target = self.target(*result);
                let declared = self.value_type(self.module.value_ty(*result));
                let value = self.value(*value);
                self.out
                    .line(format!("{target} = as_type<{declared}>({value});"));
            }
            Instruction::Math {
                fun,
                arguments,
                result,
            } => {
                let target = self.target(*result);
                let text = if *fun == MathFun::UnpackHalf2x16 {
                    let value = self.value(arguments[0]);
                    format!("float2(as_type<half2>({value}))")
                } else {
                    let arguments = arguments
                        .iter()
                        .map(|argument| self.value(*argument))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let name = fun.name();
                    format!("metal::{name}({arguments})")
                };
                self.out.line(format!("{target} = {text};"));
            }
            Instruction::Compose {
                constituents,
                result,
            } => {
                let target = self.target(*result);
                let declared = self.value_type(self.module.value_ty(*result));
                let constituents = constituents
                    .iter()
                    .map(|constituent| self.value(*constituent))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.out
                    .line(format!("{target} = {declared}({constituents});"));
            }
            Instruction::Call {
                function,
                arguments,
                result,
            } => {
                let callee = symbol(&self.module.functions()[*function as usize].name);
                let arguments = self.arguments(*function, arguments);
                match result {
                    Some(result) => {
                        let target = self.target(*result);
                        self.out.line(format!("{target} = {callee}({arguments});"));
                    }
                    None => self.out.line(format!("{callee}({arguments});")),
                }
            }
            Instruction::Atomic {
                op,
                pointer,
                value,
                result,
            } => {
                let target = self.target(*result);
                let cell = self.place_value(*pointer);
                let value = self.value(*value);
                let name = match op {
                    AtomicOp::Add => "atomic_fetch_add_explicit",
                    AtomicOp::Subtract => "atomic_fetch_sub_explicit",
                    AtomicOp::Min => "atomic_fetch_min_explicit",
                    AtomicOp::Max => "atomic_fetch_max_explicit",
                    AtomicOp::And => "atomic_fetch_and_explicit",
                    AtomicOp::Or => "atomic_fetch_or_explicit",
                    AtomicOp::Xor => "atomic_fetch_xor_explicit",
                    AtomicOp::Exchange => "atomic_exchange_explicit",
                };
                self.out.line(format!(
                    "{target} = {name}(&{cell}, {value}, memory_order_relaxed);"
                ));
            }
            Instruction::CompareExchange {
                pointer,
                expected,
                desired,
                result,
            } => {
                let target = self.target(*result);
                let cell = self.place_value(*pointer);
                let expected = self.value(*expected);
                let desired = self.value(*desired);
                let held = format!("expected_{target}");
                self.out.line(format!("auto {held} = {expected};"));
                self.out.line(format!(
                    "atomic_compare_exchange_weak_explicit(&{cell}, &{held}, {desired}, memory_order_acq_rel, memory_order_relaxed);"
                ));
                self.out.line(format!("{target} = {held};"));
            }
            Instruction::WorkGroupUniformLoad { pointer, result } => {
                self.barrier(Barrier::WorkGroup);
                let target = self.target(*result);
                let text = self.place_value(*pointer);
                self.out.line(format!("{target} = {text};"));
                self.barrier(Barrier::WorkGroup);
            }
            Instruction::Barrier(barrier) => self.barrier(*barrier),
            Instruction::If {
                condition,
                accept,
                reject,
            } => {
                let condition = self.value(*condition);
                self.out.open(format!("if ({condition})"));
                self.block(accept);
                self.out.close("}");
                if !reject.is_empty() {
                    self.out.open("else");
                    self.block(reject);
                    self.out.close("}");
                }
            }
            Instruction::Switch {
                selector,
                cases,
                default,
            } => {
                let selector = self.value(*selector);
                if cases.iter().any(|(_, body)| leaves_a_loop(body)) || leaves_a_loop(default) {
                    for (index, (value, body)) in cases.iter().enumerate() {
                        let branch = if index == 0 { "if" } else { "else if" };
                        self.out.open(format!("{branch} ({selector} == {value})"));
                        self.block(body);
                        self.out.close("}");
                    }
                    if !default.is_empty() {
                        if cases.is_empty() {
                            self.block(default);
                        } else {
                            self.out.open("else");
                            self.block(default);
                            self.out.close("}");
                        }
                    }
                } else {
                    self.out.open(format!("switch ({selector})"));
                    for (value, body) in cases {
                        self.out.open(format!("case {value}:"));
                        self.block(body);
                        if !terminates(body) {
                            self.out.line("break;");
                        }
                        self.out.close("}");
                    }
                    self.out.open("default:");
                    self.block(default);
                    if !terminates(default) {
                        self.out.line("break;");
                    }
                    self.out.close("}");
                    self.out.close("}");
                }
            }
            Instruction::Loop { body, continuing } => {
                let step = self.step(continuing);
                self.loops.push(step);
                self.out.open("while (true)");
                self.block(body);
                let step = self.loops.pop().expect("a device loop is open");
                self.out.raw(&step);
                self.out.close("}");
            }
            Instruction::Break => self.out.line("break;"),
            Instruction::Continue => {
                let step = self.loops.last().cloned().unwrap_or_default();
                if step.is_empty() {
                    self.out.line("continue;");
                } else {
                    self.out.line("{");
                    self.out.raw(&step);
                    self.out.line("    continue;");
                    self.out.line("}");
                }
            }
            Instruction::Return { value } => match value {
                Some(value) => {
                    let value = self.value(*value);
                    self.out.line(format!("return {value};"));
                }
                None if self.stage.is_some() => self.out.line("return d_out;"),
                None => self.out.line("return;"),
            },
            Instruction::Block(body) => self.block(body),
            Instruction::Discard => {
                self.out.line("discard_fragment();");
                match self.stage {
                    Some(_) => self.out.line("return d_out;"),
                    None => self.out.line("return;"),
                }
            }
            Instruction::MatrixFill { value, result } => {
                let target = self.target(*result);
                let value = self.value(*value);
                self.out.line(format!("{target} = {value};"));
            }
            Instruction::MatrixLoad {
                pointer,
                stride,
                layout,
                result,
            } => {
                let target = self.target(*result);
                let pointer = self.place_value(*pointer);
                let stride = self.value(*stride);
                let transpose = matches!(layout, MatrixLayout::ColumnMajor);
                self.out.line(format!(
                    "simdgroup_load({target}, &{pointer}, {stride}, ushort2(0, 0), {transpose});"
                ));
            }
            Instruction::MatrixStore {
                pointer,
                value,
                stride,
                layout,
            } => {
                let pointer = self.place_value(*pointer);
                let value = self.value(*value);
                let stride = self.value(*stride);
                let transpose = matches!(layout, MatrixLayout::ColumnMajor);
                self.out.line(format!(
                    "simdgroup_store({value}, &{pointer}, {stride}, ushort2(0, 0), {transpose});"
                ));
            }
            Instruction::MatrixMulAdd {
                left,
                right,
                accumulate,
                result,
            } => {
                let target = self.target(*result);
                let left = self.value(*left);
                let right = self.value(*right);
                let accumulate = self.value(*accumulate);
                self.out.line(format!("{target} = {accumulate};"));
                self.out.line(format!(
                    "simdgroup_multiply_accumulate({target}, {left}, {right}, {target});"
                ));
            }
            Instruction::MatrixLength { .. } => {
                panic!("a Metal shader cannot measure a cooperative matrix")
            }
            Instruction::MatrixExtract { value, result, .. } => {
                let target = self.target(*result);
                let value = self.value(*value);
                self.out.line(format!("{target} = {value}"));
            }
            Instruction::MatrixInsert { value, result, .. } => {
                let target = self.target(*result);
                let value = self.value(*value);
                self.out.line(format!("{target} = {value};"));
            }
        }
    }

    fn step(&mut self, continuing: &[Instruction]) -> String {
        let out = std::mem::replace(&mut self.out, Source::new());
        self.block(continuing);
        std::mem::replace(&mut self.out, out).finish()
    }

    fn barrier(&mut self, barrier: Barrier) {
        match barrier {
            Barrier::WorkGroup => self
                .out
                .line("threadgroup_barrier(mem_flags::mem_threadgroup);"),
            Barrier::Storage => self.out.line("threadgroup_barrier(mem_flags::mem_device);"),
        }
    }

    fn target(&self, value: ValueId) -> String {
        format!("d_v{}", value.index())
    }

    fn is_atomic_pointer(&self, value: ValueId) -> bool {
        match self.module.ty(self.module.value_ty(value)) {
            Type::Pointer { base, .. } => matches!(self.module.ty(*base), Type::Atomic(_)),
            _ => false,
        }
    }

    fn value(&self, value: ValueId) -> String {
        if let Some(text) = self.names.get(&value) {
            return text.clone();
        }
        match self.module.constant_of(value) {
            Some(Constant::U32(number)) => format!("{number}u"),
            Some(Constant::I32(number)) => format!("{number}"),
            Some(Constant::F32(number)) => crate::shader::literal::float(number),
            Some(Constant::Bool(flag)) => format!("{flag}"),
            Some(Constant::Zero(ty)) => {
                let declared = self.value_type(ty);
                format!("{declared}({{}})")
            }
            None => format!("d_v{}", value.index()),
        }
    }

    fn place(&self, instruction: &Instruction) -> String {
        match instruction {
            Instruction::Access { base, index, .. } => {
                let named = self.place_value(*base);
                let index = self.value(*index);
                format!("{named}[{index}]")
            }
            Instruction::AccessIndex { base, index, .. } => {
                let text = self.place_value(*base);
                let pointee = self.module.ty(self.module.value_ty(*base));
                let member = match pointee {
                    Type::Pointer { base: pointed, .. } => match self.module.ty(*pointed) {
                        Type::Struct { members, .. } => {
                            Some(symbol(&members[*index as usize].name))
                        }
                        _ => None,
                    },
                    Type::Struct { members, .. } => Some(symbol(&members[*index as usize].name)),
                    _ => None,
                };
                match member {
                    Some(member) => format!("{text}.{member}"),
                    None => format!("{text}[{index}]"),
                }
            }
            other => panic!("the device instruction {other:?} is not a place"),
        }
    }

    fn place_value(&self, value: ValueId) -> String {
        match self.declarations.get(&value) {
            Some(instruction @ (Instruction::Access { .. } | Instruction::AccessIndex { .. })) => {
                self.place(instruction)
            }
            Some(Instruction::Address {
                address: Address::Global(index),
                ..
            }) => self.global_place(*index),
            _ => self.value(value),
        }
    }
}

fn table_name(name: &str) -> String {
    format!("d_table_{name}")
}

fn local_names(module: &Module, function: &Function) -> Vec<String> {
    let mut used = function
        .arguments
        .iter()
        .map(|argument| symbol(&argument.name))
        .chain(module.globals().iter().map(|global| symbol(&global.name)))
        .collect::<HashSet<_>>();
    function
        .locals
        .iter()
        .enumerate()
        .map(|(index, local)| {
            let base = symbol(&local.name);
            let mut name = base.clone();
            let mut count = 0;
            while !used.insert(name.clone()) {
                count += 1;
                name = format!("{base}_{index}_{count}");
            }
            name
        })
        .collect()
}

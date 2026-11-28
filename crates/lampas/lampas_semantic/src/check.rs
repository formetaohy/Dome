use crate::Type;
use crate::checked::{
    Checked, CheckedBoot, CheckedComponent, CheckedField, CheckedFunction, CheckedGlobal,
    CheckedRun, CheckedSystem, HostSignature,
};
use crate::symbol::{ExprInfo, Intrinsic, StmtInfo};
use lampas_bytecode::{Access, ComponentParam, Kind, Phase};
use lampas_syntax::{
    ComponentDecl, Diagnostic, ExprId, FunctionDecl, GlobalDecl, Item, ParamDecl, Program,
    ScheduleDecl, Sources, Span, StmtId, SystemDecl, TypeExpr, TypeExprKind,
};
use std::collections::HashMap;

pub fn check(program: &Program, hosts: &[HostSignature]) -> Result<Checked, Diagnostic> {
    let mut checker = Checker::new(program, hosts);
    checker.declare(program)?;
    checker.bodies(program)?;
    Ok(checker.finish())
}

pub(crate) fn access_of(access: lampas_syntax::Access) -> Access {
    match access {
        lampas_syntax::Access::Read => Access::Read,
        lampas_syntax::Access::Write => Access::Write,
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Local {
    Value(Type),
    Component { component: u16, access: Access },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Body {
    Function,
    System { entity: bool },
    Boot,
}

#[derive(Clone)]
pub(crate) struct Setup {
    pub(crate) locals: Vec<Local>,
    pub(crate) names: Vec<Option<String>>,
    pub(crate) result: Type,
    pub(crate) body: Body,
}

struct Declared {
    types: Vec<Type>,
    locals: Vec<Local>,
    names: Vec<Option<String>>,
}

pub(crate) struct Signature {
    pub(crate) params: Vec<Type>,
    pub(crate) access: Vec<Option<Access>>,
    pub(crate) result: Type,
}

pub(crate) struct Checker<'a> {
    pub(crate) sources: &'a Sources,
    pub(crate) hosts: &'a [HostSignature],
    pub(crate) components: Vec<CheckedComponent>,
    pub(crate) component_index: HashMap<String, u16>,
    pub(crate) globals: Vec<CheckedGlobal>,
    pub(crate) global_index: HashMap<String, u16>,
    pub(crate) functions: Vec<CheckedFunction>,
    pub(crate) function_index: HashMap<String, u16>,
    pub(crate) signatures: Vec<Signature>,
    pub(crate) systems: Vec<CheckedSystem>,
    pub(crate) runs: Vec<CheckedRun>,
    pub(crate) boot: Option<CheckedBoot>,
    pub(crate) host_index: HashMap<String, u16>,
    pub(crate) infos: HashMap<ExprId, ExprInfo>,
    pub(crate) statements: HashMap<StmtId, StmtInfo>,
    pub(crate) locals: Vec<Local>,
    pub(crate) scopes: Vec<HashMap<String, u8>>,
    pub(crate) result: Type,
    pub(crate) body: Body,
    pub(crate) loops: usize,
    function_setups: Vec<Setup>,
    system_setups: Vec<Setup>,
    schedules: Vec<Schedule>,
    boot_setup: Option<Setup>,
    names: HashMap<String, Span>,
}

struct Schedule {
    phase: Phase,
    names: Vec<(String, Span)>,
}

impl<'a> Checker<'a> {
    fn new(program: &'a Program, hosts: &'a [HostSignature]) -> Self {
        let host_index = hosts
            .iter()
            .enumerate()
            .map(|(index, host)| (host.name.clone(), index as u16))
            .collect();
        Self {
            sources: &program.sources,
            hosts,
            components: Vec::new(),
            component_index: HashMap::new(),
            globals: Vec::new(),
            global_index: HashMap::new(),
            functions: Vec::new(),
            function_index: HashMap::new(),
            signatures: Vec::new(),
            systems: Vec::new(),
            runs: Vec::new(),
            boot: None,
            host_index,
            infos: HashMap::new(),
            statements: HashMap::new(),
            locals: Vec::new(),
            scopes: Vec::new(),
            result: Type::VOID,
            body: Body::Boot,
            loops: 0,
            function_setups: Vec::new(),
            system_setups: Vec::new(),
            schedules: Vec::new(),
            boot_setup: None,
            names: HashMap::new(),
        }
    }

    fn finish(self) -> Checked {
        Checked {
            components: self.components,
            globals: self.globals,
            functions: self.functions,
            systems: self.systems,
            runs: self.runs,
            boot: self.boot,
            hosts: self.hosts.len(),
            infos: self.infos,
            statements: self.statements,
        }
    }

    pub(crate) fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        self.sources.diagnostic(span, message)
    }

    pub(crate) fn describe(&self, ty: Type) -> String {
        match ty {
            Type::Void => String::from("no value"),
            Type::Value(kind) => String::from(kind.name()),
            Type::List(kind) => format!("a list of {}", kind.name()),
            Type::Component(id) => {
                format!("the {} component", self.components[id as usize].name)
            }
        }
    }

    pub(crate) fn mismatch(&self, span: Span, expected: Type, found: Type) -> Diagnostic {
        self.error(
            span,
            format!(
                "{} stands where {} belongs",
                self.describe(found),
                self.describe(expected),
            ),
        )
    }

    fn declare(&mut self, program: &Program) -> Result<(), Diagnostic> {
        for item in &program.items {
            self.declare_name(item)?;
        }
        for item in &program.items {
            self.declare_item(item)?;
        }
        self.order()
    }

    fn order(&mut self) -> Result<(), Diagnostic> {
        let index = self
            .systems
            .iter()
            .enumerate()
            .map(|(index, system)| (system.name.clone(), index as u16))
            .collect::<HashMap<_, _>>();
        let mut placed = vec![false; self.systems.len()];
        let mut runs = Vec::new();
        for schedule in &self.schedules {
            for (name, span) in &schedule.names {
                let Some(system) = index.get(name).copied() else {
                    return Err(self.error(
                        *span,
                        format!("the schedule names the system {name}, which stands nowhere"),
                    ));
                };
                if std::mem::replace(&mut placed[system as usize], true) {
                    return Err(
                        self.error(*span, format!("the schedule names the system {name} twice"))
                    );
                }
                runs.push(CheckedRun {
                    phase: schedule.phase,
                    system,
                });
            }
        }
        for (index, system) in self.systems.iter().enumerate() {
            if !placed[index] {
                return Err(self.error(
                    system.span,
                    format!("the system {} stands in no schedule", system.name),
                ));
            }
        }
        self.runs = runs;
        Ok(())
    }

    fn declare_name(&mut self, item: &Item) -> Result<(), Diagnostic> {
        let Some(name) = item.name().map(ToString::to_string) else {
            return Ok(());
        };
        let span = item.span();
        if self.names.insert(name.clone(), span).is_some() {
            return Err(self.error(span, format!("the name {name} stands twice")));
        }
        if let Some(intrinsic) = Intrinsic::named(&name) {
            return Err(self.error(
                span,
                format!(
                    "the name {name} belongs to the {} intrinsic",
                    intrinsic.name()
                ),
            ));
        }
        match item {
            Item::Component(decl) => {
                if !decl
                    .name
                    .name
                    .starts_with(|held: char| held.is_ascii_uppercase())
                {
                    return Err(self.error(
                        decl.name.span,
                        format!("the component {name} starts with a capital letter"),
                    ));
                }
                let id = self.component_index.len() as u16;
                self.component_index.insert(name, id);
            }
            Item::Global(decl) => {
                self.declare_value_name(decl.name.span, &name, "global")?;
                let id = self.global_index.len() as u16;
                self.global_index.insert(name, id);
            }
            Item::Function(decl) => {
                self.declare_value_name(decl.name.span, &name, "function")?;
                let id = self.function_index.len() as u16;
                self.function_index.insert(name, id);
            }
            Item::System(decl) => {
                if !decl
                    .name
                    .name
                    .starts_with(|held: char| held.is_ascii_lowercase())
                {
                    return Err(self.error(
                        decl.name.span,
                        format!("the system {name} starts with a small letter"),
                    ));
                }
            }
            Item::Schedule(_) | Item::Boot(_) => {}
        }
        Ok(())
    }

    fn declare_value_name(&self, span: Span, name: &str, what: &str) -> Result<(), Diagnostic> {
        if self.host_index.contains_key(name) {
            return Err(self.error(
                span,
                format!("the {what} {name} shares the name of a host function"),
            ));
        }
        if !name.starts_with(|held: char| held.is_ascii_lowercase()) {
            return Err(self.error(
                span,
                format!("the {what} {name} starts with a small letter"),
            ));
        }
        Ok(())
    }

    fn declare_item(&mut self, item: &Item) -> Result<(), Diagnostic> {
        match item {
            Item::Component(decl) => self.declare_component(decl),
            Item::Global(decl) => self.declare_global(decl),
            Item::Function(decl) => self.declare_function(decl),
            Item::System(decl) => self.declare_system(decl),
            Item::Schedule(decl) => self.declare_schedule(decl),
            Item::Boot(_) => self.declare_boot(),
        }
    }

    fn declare_schedule(&mut self, decl: &ScheduleDecl) -> Result<(), Diagnostic> {
        let phase = match decl.phase {
            lampas_syntax::Phase::Update => Phase::Update,
            lampas_syntax::Phase::Fixed => Phase::Fixed,
        };
        if self.schedules.iter().any(|held| held.phase == phase) {
            return Err(self.error(
                decl.span,
                format!(
                    "the script schedules the {} phase twice",
                    lampas_syntax::Phase::name(decl.phase),
                ),
            ));
        }
        self.schedules.push(Schedule {
            phase,
            names: decl
                .systems
                .iter()
                .map(|ident| (ident.name.clone(), ident.span))
                .collect(),
        });
        Ok(())
    }

    fn declare_component(&mut self, decl: &ComponentDecl) -> Result<(), Diagnostic> {
        let name = decl.name.name.clone();
        let mut fields: Vec<CheckedField> = Vec::new();
        for field in &decl.fields {
            if fields.iter().any(|held| held.name == field.name.name) {
                return Err(self.error(
                    field.name.span,
                    format!(
                        "the component {name} holds the field {} twice",
                        field.name.name
                    ),
                ));
            }
            let kind = self.scalar_kind(&field.ty, "a field")?;
            let default = match &field.default {
                Some(expr) => Some(self.constant(expr, Type::Value(kind))?),
                None => None,
            };
            fields.push(CheckedField {
                name: field.name.name.clone(),
                kind,
                default,
            });
        }
        let id = self.components.len() as u16;
        assert_eq!(
            self.component_index.get(&name),
            Some(&id),
            "a component holds the identifier its name declared",
        );
        self.components.push(CheckedComponent { name, fields });
        Ok(())
    }

    fn declare_global(&mut self, decl: &GlobalDecl) -> Result<(), Diagnostic> {
        let name = decl.name.name.clone();
        let ty = self.value_type(&decl.ty)?;
        let value = self.constant(&decl.value, ty)?;
        let id = self.globals.len() as u16;
        assert_eq!(
            self.global_index.get(&name),
            Some(&id),
            "a global holds the identifier its name declared",
        );
        self.globals.push(CheckedGlobal { name, ty, value });
        Ok(())
    }

    fn declare_function(&mut self, decl: &FunctionDecl) -> Result<(), Diagnostic> {
        let name = decl.name.name.clone();
        let Declared {
            types: params,
            locals,
            names,
        } = self.params(&decl.params)?;
        let result = match &decl.result {
            Some(ty) => self.value_type(ty)?,
            None => Type::VOID,
        };
        let access = locals
            .iter()
            .map(|local| match local {
                Local::Component { access, .. } => Some(*access),
                Local::Value(_) => None,
            })
            .collect();
        let id = self.functions.len() as u16;
        assert_eq!(
            self.function_index.get(&name),
            Some(&id),
            "a function holds the identifier its name declared",
        );
        self.signatures.push(Signature {
            params: params.clone(),
            access,
            result,
        });
        self.functions.push(CheckedFunction {
            name,
            arity: params.len() as u8,
            locals: 0,
            result,
        });
        self.function_setups.push(Setup {
            locals,
            names,
            result,
            body: Body::Function,
        });
        Ok(())
    }

    fn declare_system(&mut self, decl: &SystemDecl) -> Result<(), Diagnostic> {
        let name = decl.name.name.clone();
        let mut params = Vec::new();
        let mut locals = Vec::new();
        let mut names = Vec::new();
        if !decl.params.is_empty() {
            locals.push(Local::Value(Type::ENTITY));
            names.push(None);
        }
        for param in &decl.params {
            let Type::Component(component) = self.param(param)? else {
                return Err(self.error(
                    param.ty.span,
                    format!("the system {name} walks components only"),
                ));
            };
            let access = access_of(
                param
                    .access
                    .expect("a component parameter names its access"),
            );
            if params
                .iter()
                .any(|held: &ComponentParam| held.component == component)
            {
                return Err(self.error(
                    param.name.span,
                    format!("the system {name} walks one component twice"),
                ));
            }
            if names
                .iter()
                .any(|held| held.as_deref() == Some(param.name.name.as_str()))
            {
                return Err(self.error(
                    param.name.span,
                    format!("the system {name} names {} twice", param.name.name),
                ));
            }
            params.push(ComponentParam { component, access });
            locals.push(Local::Component { component, access });
            names.push(Some(param.name.name.clone()));
        }
        self.systems.push(CheckedSystem {
            name: name.clone(),
            span: decl.name.span,
            params,
            locals: 0,
        });
        self.system_setups.push(Setup {
            locals,
            names,
            result: Type::VOID,
            body: Body::System {
                entity: !decl.params.is_empty(),
            },
        });
        Ok(())
    }

    fn declare_boot(&mut self) -> Result<(), Diagnostic> {
        self.boot = Some(CheckedBoot { locals: 0 });
        self.boot_setup = Some(Setup {
            locals: Vec::new(),
            names: Vec::new(),
            result: Type::VOID,
            body: Body::Boot,
        });
        Ok(())
    }

    fn params(&mut self, params: &[ParamDecl]) -> Result<Declared, Diagnostic> {
        let mut types = Vec::new();
        let mut locals = Vec::new();
        let mut names: Vec<Option<String>> = Vec::new();
        for param in params {
            if names
                .iter()
                .any(|held| held.as_deref() == Some(param.name.name.as_str()))
            {
                return Err(self.error(
                    param.name.span,
                    format!("the name {} stands twice", param.name.name),
                ));
            }
            let ty = self.param(param)?;
            types.push(ty);
            locals.push(match ty {
                Type::Component(component) => Local::Component {
                    component,
                    access: access_of(
                        param
                            .access
                            .expect("a component parameter names its access"),
                    ),
                },
                other => Local::Value(other),
            });
            names.push(Some(param.name.name.clone()));
        }
        Ok(Declared {
            types,
            locals,
            names,
        })
    }

    fn param(&self, param: &ParamDecl) -> Result<Type, Diagnostic> {
        match &param.ty.kind {
            TypeExprKind::Named(ident) => match self.component_index.get(&ident.name) {
                Some(component) => {
                    if param.access.is_none() {
                        return Err(self.error(
                            param.name.span,
                            format!("the component {} is read or written", ident.name),
                        ));
                    }
                    Ok(Type::Component(*component))
                }
                None => Err(self.error(
                    ident.span,
                    format!("the source holds no type {}", ident.name),
                )),
            },
            _ => {
                if param.access.is_some() {
                    return Err(self.error(param.ty.span, "an access names a component"));
                }
                self.value_type(&param.ty)
            }
        }
    }

    pub(crate) fn value_type(&self, ty: &TypeExpr) -> Result<Type, Diagnostic> {
        match &ty.kind {
            TypeExprKind::Bool => Ok(Type::BOOL),
            TypeExprKind::Int => Ok(Type::INT),
            TypeExprKind::Float => Ok(Type::FLOAT),
            TypeExprKind::Text => Ok(Type::TEXT),
            TypeExprKind::Vector => Ok(Type::VECTOR),
            TypeExprKind::Entity => Ok(Type::ENTITY),
            TypeExprKind::List(element) => Ok(Type::List(self.scalar_kind(element, "a list")?)),
            TypeExprKind::Named(ident) => match self.component_index.get(&ident.name) {
                Some(_) => Err(self.error(
                    ident.span,
                    format!("the component {} is no value", ident.name),
                )),
                None => Err(self.error(
                    ident.span,
                    format!("the source holds no type {}", ident.name),
                )),
            },
        }
    }

    fn scalar_kind(&self, ty: &TypeExpr, what: &str) -> Result<Kind, Diagnostic> {
        match &ty.kind {
            TypeExprKind::Bool => Ok(Kind::Bool),
            TypeExprKind::Int => Ok(Kind::Int),
            TypeExprKind::Float => Ok(Kind::Float),
            TypeExprKind::Text => Ok(Kind::Text),
            TypeExprKind::Vector => Ok(Kind::Vector),
            TypeExprKind::Entity => Ok(Kind::Entity),
            TypeExprKind::List(_) => Err(self.error(ty.span, format!("{what} holds no list"))),
            TypeExprKind::Named(ident) => match self.component_index.get(&ident.name) {
                Some(_) => Err(self.error(
                    ident.span,
                    format!("{what} holds no {} component", ident.name),
                )),
                None => Err(self.error(
                    ident.span,
                    format!("the source holds no type {}", ident.name),
                )),
            },
        }
    }

    fn bodies(&mut self, program: &Program) -> Result<(), Diagnostic> {
        let mut functions = 0;
        let mut systems = 0;
        for item in &program.items {
            match item {
                Item::Function(decl) => {
                    let setup = self.function_setups[functions].clone();
                    self.begin(setup);
                    self.block(&decl.body)?;
                    self.end();
                    if self.functions[functions].result != Type::VOID
                        && !crate::statement::diverges(&decl.body)
                    {
                        return Err(self.error(
                            decl.name.span,
                            format!(
                                "the function {} returns {} on no path",
                                decl.name.name,
                                self.describe(self.functions[functions].result),
                            ),
                        ));
                    }
                    self.functions[functions].locals = self.locals.len() as u16;
                    functions += 1;
                }
                Item::System(decl) => {
                    let setup = self.system_setups[systems].clone();
                    self.begin(setup);
                    self.block(&decl.body)?;
                    self.end();
                    self.systems[systems].locals = self.locals.len() as u16;
                    systems += 1;
                }
                Item::Boot(decl) => {
                    let setup = self
                        .boot_setup
                        .clone()
                        .expect("a boot declaration holds its setup");
                    self.begin(setup);
                    self.block(&decl.body)?;
                    self.end();
                    self.boot
                        .as_mut()
                        .expect("a boot declaration holds its body")
                        .locals = self.locals.len() as u16;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn begin(&mut self, setup: Setup) {
        self.locals = setup.locals;
        self.scopes = vec![
            setup
                .names
                .iter()
                .enumerate()
                .filter_map(|(slot, name)| name.as_ref().map(|name| (name.clone(), slot as u8)))
                .collect(),
        ];
        self.result = setup.result;
        self.body = setup.body;
        self.loops = 0;
    }

    fn end(&mut self) {
        self.scopes.clear();
    }

    pub(crate) fn declare_local(
        &mut self,
        name: &str,
        span: Span,
        local: Local,
    ) -> Result<u8, Diagnostic> {
        for scope in &self.scopes {
            if scope.contains_key(name) {
                return Err(self.error(span, format!("the name {name} stays in scope")));
            }
        }
        let slot = self.hidden(local, span)?;
        self.scopes
            .last_mut()
            .expect("a body holds its scope")
            .insert(String::from(name), slot);
        Ok(slot)
    }

    pub(crate) fn hidden(&mut self, local: Local, span: Span) -> Result<u8, Diagnostic> {
        if self.locals.len() >= u8::MAX as usize {
            return Err(self.error(span, "the body holds more than 255 slots"));
        }
        self.locals.push(local);
        Ok(self.locals.len() as u8 - 1)
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<u8> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    pub(crate) fn local_type(&self, slot: u8) -> Type {
        match self.locals[slot as usize] {
            Local::Value(ty) => ty,
            Local::Component { component, .. } => Type::Component(component),
        }
    }
}

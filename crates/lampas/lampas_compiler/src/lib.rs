mod builder;
mod expression;

use crate::builder::FunctionBuilder;
use lampas_bytecode::{Component, Constant, Field, Function, Global, Module, System};
use lampas_semantic::{Checked, CheckedRun};
use lampas_syntax::{Diagnostic, Item, Program, SystemDecl};
use std::collections::HashMap;

pub fn compile(program: &Program, checked: &Checked) -> Result<Module, Diagnostic> {
    let mut module = Module::new();
    for component in &checked.components {
        module.components.push(Component {
            name: component.name.clone(),
            fields: component
                .fields
                .iter()
                .map(|field| Field {
                    name: field.name.clone(),
                    kind: field.kind,
                    default: field.default.clone(),
                })
                .collect(),
        });
    }
    for global in &checked.globals {
        module.globals.push(Global {
            name: global.name.clone(),
            value: global.value.clone(),
        });
    }
    let mut constants = Vec::new();
    let mut functions = 0;
    for item in &program.items {
        match item {
            Item::Function(decl) => {
                let locals = checked.functions[functions].locals;
                module.functions.push(build(
                    checked,
                    &mut constants,
                    &program.sources,
                    &decl.name.name,
                    locals,
                    false,
                    &decl.body,
                )?);
                functions += 1;
            }
            Item::Boot(decl) => {
                let locals = checked
                    .boot
                    .as_ref()
                    .expect("a boot declaration holds its slots")
                    .locals;
                let function = module.functions.len() as u32;
                module.functions.push(build(
                    checked,
                    &mut constants,
                    &program.sources,
                    "boot",
                    locals,
                    false,
                    &decl.body,
                )?);
                module.boot = Some(function);
            }
            _ => {}
        }
    }
    let bodies = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::System(decl) => Some((decl.name.name.as_str(), decl)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    for CheckedRun { phase, system } in &checked.runs {
        let system = &checked.systems[*system as usize];
        let decl: &SystemDecl = bodies
            .get(system.name.as_str())
            .expect("a checked system holds the declaration it names");
        let function = module.functions.len() as u32;
        module.functions.push(build(
            checked,
            &mut constants,
            &program.sources,
            &system.name,
            system.locals,
            true,
            &decl.body,
        )?);
        module.systems.push(System {
            name: system.name.clone(),
            phase: *phase,
            function,
            components: system.params.clone(),
        });
    }
    module.constants = constants;
    Ok(module)
}

fn build(
    checked: &Checked,
    constants: &mut Vec<Constant>,
    sources: &lampas_syntax::Sources,
    name: &str,
    locals: u16,
    system: bool,
    body: &lampas_syntax::Block,
) -> Result<Function, Diagnostic> {
    FunctionBuilder::new(checked, constants, sources, name, locals, system, body.span).body(body)
}

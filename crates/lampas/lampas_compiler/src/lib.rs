mod builder;
mod expression;

use crate::builder::FunctionBuilder;
use lampas_bytecode::{Component, Constant, Field, Function, Global, Module, Phase, System};
use lampas_semantic::Checked;
use lampas_syntax::{Diagnostic, Item, Program};

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
    let mut systems = 0;
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
            Item::System(decl) => {
                let locals = checked.systems[systems].locals;
                let function = module.functions.len() as u32;
                module.functions.push(build(
                    checked,
                    &mut constants,
                    &program.sources,
                    &decl.name.name,
                    locals,
                    true,
                    &decl.body,
                )?);
                module.systems.push(System {
                    name: decl.name.name.clone(),
                    phase: match decl.phase {
                        lampas_syntax::Phase::Update => Phase::Update,
                        lampas_syntax::Phase::Fixed => Phase::Fixed,
                    },
                    function,
                    components: checked.systems[systems].params.clone(),
                });
                systems += 1;
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

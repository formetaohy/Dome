use crate::command::{self, Command};
use crate::host::Hosts;
use crate::query::Matches;
use crate::runner::Runner;
use crate::store::Store;
use dome_ecs::World;
use lampas_bytecode::{Module, Phase};
use lampas_syntax::Diagnostic;
use lampas_vm::{Value, Vm};
use std::path::Path;

pub struct Scripts {
    module: Module,
    vm: Vm,
    store: Store,
    hosts: Hosts,
    matches: Matches,
    commands: Vec<Command>,
}

impl Scripts {
    pub fn load(root: &Path, hosts: Hosts) -> Result<Self, Diagnostic> {
        let module = crate::compile_folder(root, &hosts)?;
        Ok(Self::new(module, hosts))
    }

    pub fn new(module: Module, hosts: Hosts) -> Self {
        let vm = Vm::new(&module);
        let store = Store::new(&module);
        Self {
            module,
            vm,
            store,
            hosts,
            matches: Matches::new(),
            commands: Vec::new(),
        }
    }

    pub fn module(&self) -> &Module {
        &self.module
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn globals(&self) -> &[Value] {
        self.vm.globals()
    }

    pub fn boot(&mut self, world: &mut World) {
        let Some(function) = self.module.boot else {
            return;
        };
        self.store.retain(world);
        {
            let mut runner = Runner {
                store: &mut self.store,
                world,
                hosts: &mut self.hosts,
                commands: &mut self.commands,
            };
            self.vm.boot(&self.module, function, &mut runner);
        }
        command::apply(&mut self.commands, &mut self.store, world);
        self.store.retain(world);
    }

    pub fn update(&mut self, world: &mut World) {
        self.phase(world, Phase::Update);
    }

    pub fn fixed(&mut self, world: &mut World) {
        self.phase(world, Phase::Fixed);
    }

    fn phase(&mut self, world: &mut World, phase: Phase) {
        self.store.retain(world);
        for index in 0..self.module.systems.len() {
            if self.module.systems[index].phase != phase {
                continue;
            }
            let function = self.module.systems[index].function;
            let width = self.module.systems[index].components.len();
            self.matches
                .collect(&self.store, &self.module.systems[index].components);
            {
                let mut runner = Runner {
                    store: &mut self.store,
                    world,
                    hosts: &mut self.hosts,
                    commands: &mut self.commands,
                };
                if width == 0 {
                    self.vm.run(&self.module, function, None, &[], &mut runner);
                } else {
                    for at in 0..self.matches.len() {
                        let entity = self.matches.entity(at);
                        let rows = self.matches.rows(at);
                        if !self
                            .vm
                            .run(&self.module, function, Some(entity), rows, &mut runner)
                        {
                            break;
                        }
                    }
                }
            }
            if !self.commands.is_empty() {
                command::apply(&mut self.commands, &mut self.store, world);
                self.store.retain(world);
            }
        }
    }
}

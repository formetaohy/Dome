use crate::compile::ScriptError;
use crate::effects::Effects;
use crate::host::Hosts;
use crate::query::Matches;
use crate::runner::Runner;
use dome_ecs::{Layout, World};
use lampas_bytecode::{Module, Phase};
use lampas_vm::{RunOutcome, Trap, Value, Vm};
use std::path::Path;

pub struct ScriptSpawned;

pub struct Scripts {
    module: Module,
    layout: Vec<Layout>,
    vm: Vm,
    hosts: Hosts,
    matches: Matches,
    effects: Effects,
}

pub fn reset(world: &mut World) {
    let spawned = world
        .iter::<ScriptSpawned>()
        .map(|(entity, _)| entity)
        .collect::<Vec<_>>();
    for entity in spawned {
        world.despawn(entity);
    }
}

impl Scripts {
    pub fn load(root: &Path, hosts: Hosts) -> Result<Self, ScriptError> {
        let module = crate::compile_folder(root, &hosts)?;
        Ok(Self::new(module, hosts))
    }

    pub fn new(module: Module, hosts: Hosts) -> Self {
        let vm = Vm::new(&module);
        let layout = module
            .components
            .iter()
            .map(|component| {
                let fields = component
                    .fields
                    .iter()
                    .map(|field| field.kind)
                    .collect::<Vec<_>>();
                Layout::new(component.name.clone(), fields)
            })
            .collect();
        Self {
            module,
            layout,
            vm,
            hosts,
            matches: Matches::new(),
            effects: Effects::new(),
        }
    }

    pub fn module(&self) -> &Module {
        &self.module
    }

    pub fn globals(&self) -> &[Value] {
        self.vm.globals()
    }

    pub fn boot(&mut self, world: &mut World) -> Result<(), Trap> {
        world.reset_dynamic(&self.layout);
        let Some(function) = self.module.boot else {
            return Ok(());
        };
        let savepoint = self.vm.savepoint();
        let outcome = {
            let mut runner = Runner {
                world,
                hosts: &mut self.hosts,
                effects: &mut self.effects,
            };
            self.vm.boot(&self.module, function, &mut runner)
        };
        self.settle(world, savepoint, outcome)
    }

    pub fn update(&mut self, world: &mut World) -> Result<(), Trap> {
        self.phase(world, Phase::Update)
    }

    pub fn fixed(&mut self, world: &mut World) -> Result<(), Trap> {
        self.phase(world, Phase::Fixed)
    }

    fn phase(&mut self, world: &mut World, phase: Phase) -> Result<(), Trap> {
        let mut fault = None;
        for index in 0..self.module.systems.len() {
            if self.module.systems[index].phase != phase {
                continue;
            }
            let function = self.module.systems[index].function;
            let width = self.module.systems[index].components.len();
            self.matches
                .collect(world, &self.module.systems[index].components);
            let savepoint = self.vm.savepoint();
            let outcome = {
                let mut runner = Runner {
                    world,
                    hosts: &mut self.hosts,
                    effects: &mut self.effects,
                };
                if width == 0 {
                    self.vm
                        .run(&self.module, function, None, &[], &mut runner)
                        .map(|_| ())
                } else {
                    let mut outcome = Ok(());
                    for at in 0..self.matches.len() {
                        let entity = self.matches.entity(at);
                        let rows = self.matches.rows(at);
                        match self
                            .vm
                            .run(&self.module, function, Some(entity), rows, &mut runner)
                        {
                            Ok(RunOutcome::Stopped) => break,
                            Ok(RunOutcome::Completed) => {}
                            Err(trap) => {
                                outcome = Err(trap);
                                break;
                            }
                        }
                    }
                    outcome
                }
            };
            if let Err(trap) = self.settle(world, savepoint, outcome) {
                fault.get_or_insert(trap);
            }
        }
        match fault {
            Some(trap) => Err(trap),
            None => Ok(()),
        }
    }

    fn settle(
        &mut self,
        world: &mut World,
        savepoint: Vec<Value>,
        outcome: Result<(), Trap>,
    ) -> Result<(), Trap> {
        match outcome {
            Ok(()) => match self.effects.commit(world) {
                Ok(()) => Ok(()),
                Err(trap) => {
                    self.effects.rollback(world);
                    self.vm.rewind(savepoint);
                    Err(trap)
                }
            },
            Err(trap) => {
                self.effects.rollback(world);
                self.vm.rewind(savepoint);
                Err(trap)
            }
        }
    }
}

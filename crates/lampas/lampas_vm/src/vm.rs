use crate::context::Context;
use crate::trap::Trap;
use crate::value::Value;
use dome_ecs::Entity;
use lampas_bytecode::Module;

pub(crate) const MAXIMUM_DEPTH: usize = 256;
pub(crate) const INSTRUCTION_BUDGET: usize = 4 << 20;

#[derive(Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) function: u32,
    pub(crate) base: usize,
    pub(crate) pc: usize,
    pub(crate) dst: u8,
}

pub(crate) enum Outcome {
    Returned(Value),
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Completed,
    Stopped,
}

pub struct Vm {
    pub(crate) stack: Vec<Value>,
    pub(crate) frames: Vec<Frame>,
    pub(crate) globals: Vec<Value>,
}

impl Vm {
    pub fn new(module: &Module) -> Self {
        Self {
            stack: Vec::new(),
            frames: Vec::new(),
            globals: module
                .globals
                .iter()
                .map(|global| Value::constant(&global.value))
                .collect(),
        }
    }

    pub fn globals(&self) -> &[Value] {
        &self.globals
    }

    pub fn savepoint(&self) -> Vec<Value> {
        self.globals.clone()
    }

    pub fn rewind(&mut self, savepoint: Vec<Value>) {
        assert_eq!(
            savepoint.len(),
            self.globals.len(),
            "a savepoint of {} globals rewinds {}",
            savepoint.len(),
            self.globals.len(),
        );
        self.globals = savepoint;
    }

    pub fn call(
        &mut self,
        module: &Module,
        function: u32,
        arguments: &[Value],
        context: &mut dyn Context,
    ) -> Result<Value, Trap> {
        match self.invoke(module, function, arguments, context)? {
            Outcome::Returned(value) => Ok(value),
            Outcome::Stopped => Err(Trap::new(format!(
                "the lampas function {} stops where its caller awaits a value",
                module.functions[function as usize].name,
            ))),
        }
    }

    pub fn boot(
        &mut self,
        module: &Module,
        function: u32,
        context: &mut dyn Context,
    ) -> Result<(), Trap> {
        self.call(module, function, &[], context).map(|_| ())
    }

    pub fn run(
        &mut self,
        module: &Module,
        function: u32,
        entity: Option<Entity>,
        components: &[u32],
        context: &mut dyn Context,
    ) -> Result<RunOutcome, Trap> {
        self.enter(module, function);
        if let Some(entity) = entity {
            self.stack.push(Value::Entity(entity));
        }
        for row in components {
            self.stack.push(Value::Component(*row));
        }
        self.start(module, function);
        let outcome = self.execute(module, context);
        match self.settle(outcome)? {
            Outcome::Returned(_) => Ok(RunOutcome::Completed),
            Outcome::Stopped => Ok(RunOutcome::Stopped),
        }
    }

    fn invoke(
        &mut self,
        module: &Module,
        function: u32,
        arguments: &[Value],
        context: &mut dyn Context,
    ) -> Result<Outcome, Trap> {
        self.enter(module, function);
        self.stack.extend_from_slice(arguments);
        self.start(module, function);
        let outcome = self.execute(module, context);
        self.settle(outcome)
    }

    fn settle(&mut self, outcome: Result<Outcome, Trap>) -> Result<Outcome, Trap> {
        if outcome.is_err() {
            self.stack.clear();
            self.frames.clear();
        }
        outcome
    }

    fn enter(&mut self, module: &Module, function: u32) {
        assert!(
            self.frames.is_empty() && self.stack.is_empty(),
            "a lampas machine runs one call at a time",
        );
        assert!(
            !module.functions.is_empty(),
            "a lampas module holds no function {}",
            function,
        );
    }

    fn start(&mut self, module: &Module, function: u32) {
        let registers = module.functions[function as usize].registers as usize;
        assert!(
            self.stack.len() <= registers,
            "the call passes more arguments than the function {} holds",
            module.functions[function as usize].name,
        );
        self.stack.resize(registers, Value::Unit);
        self.frames.push(Frame {
            function,
            base: 0,
            pc: 0,
            dst: 0,
        });
    }
}

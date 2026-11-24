use crate::context::Context;
use crate::value::Value;
use dome_ecs::Entity;
use lampas_bytecode::Module;

const MAXIMUM_DEPTH: usize = 256;

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

    pub fn call(
        &mut self,
        module: &Module,
        function: u32,
        arguments: &[Value],
        context: &mut dyn Context,
    ) -> Value {
        match self.invoke(module, function, arguments, context) {
            Outcome::Returned(value) => value,
            Outcome::Stopped => panic!(
                "the lampas function {} stops",
                module.functions[function as usize].name,
            ),
        }
    }

    pub fn boot(&mut self, module: &Module, function: u32, context: &mut dyn Context) {
        self.call(module, function, &[], context);
    }

    pub fn run(
        &mut self,
        module: &Module,
        function: u32,
        entity: Option<Entity>,
        components: &[u32],
        context: &mut dyn Context,
    ) -> bool {
        self.enter(module, function);
        if let Some(entity) = entity {
            self.stack.push(Value::Entity(entity));
        }
        for row in components {
            self.stack.push(Value::Component(*row));
        }
        self.start(module, function);
        match self.execute(module, context) {
            Outcome::Returned(_) => true,
            Outcome::Stopped => false,
        }
    }

    fn invoke(
        &mut self,
        module: &Module,
        function: u32,
        arguments: &[Value],
        context: &mut dyn Context,
    ) -> Outcome {
        self.enter(module, function);
        self.stack.extend_from_slice(arguments);
        self.start(module, function);
        self.execute(module, context)
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

    pub(crate) fn maximum_depth(&self) -> usize {
        MAXIMUM_DEPTH
    }
}

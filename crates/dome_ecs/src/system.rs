use crate::world::World;

pub trait System: 'static {
    fn run(&mut self, world: &mut World);
}

impl<F: FnMut(&mut World) + 'static> System for F {
    fn run(&mut self, world: &mut World) {
        self(world);
    }
}

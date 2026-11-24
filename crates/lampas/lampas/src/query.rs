use dome_ecs::{Entity, World};
use lampas_bytecode::ComponentParam;

#[derive(Default)]
pub struct Matches {
    entities: Vec<Entity>,
    rows: Vec<u32>,
    width: usize,
}

impl Matches {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn collect(&mut self, world: &World, components: &[ComponentParam]) {
        self.entities.clear();
        self.rows.clear();
        self.width = components.len();
        if components.is_empty() {
            return;
        }
        let dynamic = world.dynamic();
        let smallest = components
            .iter()
            .map(|param| param.component)
            .min_by_key(|component| dynamic.column(*component).len())
            .expect("a query walks one component");
        'entity: for entity in dynamic.entities(smallest) {
            let mark = self.rows.len();
            for param in components {
                match dynamic.row(param.component, entity) {
                    Some(row) => self.rows.push(row),
                    None => {
                        self.rows.truncate(mark);
                        continue 'entity;
                    }
                }
            }
            self.entities.push(entity);
        }
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn entity(&self, at: usize) -> Entity {
        self.entities[at]
    }

    pub fn rows(&self, at: usize) -> &[u32] {
        &self.rows[at * self.width..(at + 1) * self.width]
    }
}

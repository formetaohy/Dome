use crate::store::Store;
use dome_ecs::Entity;
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

    pub fn collect(&mut self, store: &Store, components: &[ComponentParam]) {
        self.entities.clear();
        self.rows.clear();
        self.width = components.len();
        if components.is_empty() {
            return;
        }
        let smallest = components
            .iter()
            .map(|param| param.component)
            .min_by_key(|component| store.len(*component))
            .expect("a query walks one component");
        'entity: for entity in store.entities(smallest) {
            let mark = self.rows.len();
            for param in components {
                match store.row(param.component, entity) {
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

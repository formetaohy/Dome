use dome_ecs::{Entity, World};
use dome_math::Vec3;
use lampas_bytecode::{FieldKind, Module};
use lampas_vm::Value;
use std::rc::Rc;

const VACANT: u32 = u32::MAX;

pub struct Store {
    columns: Vec<Column>,
    names: Vec<String>,
}

struct Column {
    sparse: Vec<u32>,
    entities: Vec<Entity>,
    fields: Vec<FieldColumn>,
}

enum FieldColumn {
    Bool(Vec<bool>),
    Int(Vec<i64>),
    Float(Vec<f64>),
    Text(Vec<Rc<str>>),
    Vector(Vec<Vec3>),
    Entity(Vec<Entity>),
}

impl FieldColumn {
    fn of(kind: FieldKind) -> Self {
        match kind {
            FieldKind::Bool => Self::Bool(Vec::new()),
            FieldKind::Int => Self::Int(Vec::new()),
            FieldKind::Float => Self::Float(Vec::new()),
            FieldKind::Text => Self::Text(Vec::new()),
            FieldKind::Vector => Self::Vector(Vec::new()),
            FieldKind::Entity => Self::Entity(Vec::new()),
        }
    }

    fn kind(&self) -> FieldKind {
        match self {
            Self::Bool(_) => FieldKind::Bool,
            Self::Int(_) => FieldKind::Int,
            Self::Float(_) => FieldKind::Float,
            Self::Text(_) => FieldKind::Text,
            Self::Vector(_) => FieldKind::Vector,
            Self::Entity(_) => FieldKind::Entity,
        }
    }

    fn read(&self, row: usize) -> Value {
        match self {
            Self::Bool(column) => Value::Bool(column[row]),
            Self::Int(column) => Value::Int(column[row]),
            Self::Float(column) => Value::Float(column[row]),
            Self::Text(column) => Value::Text(column[row].clone()),
            Self::Vector(column) => Value::Vector(column[row]),
            Self::Entity(column) => Value::Entity(column[row]),
        }
    }

    fn write(&mut self, row: usize, value: Value) {
        match (self, value) {
            (Self::Bool(column), Value::Bool(value)) => column[row] = value,
            (Self::Int(column), Value::Int(value)) => column[row] = value,
            (Self::Float(column), Value::Float(value)) => column[row] = value,
            (Self::Text(column), Value::Text(value)) => column[row] = value,
            (Self::Vector(column), Value::Vector(value)) => column[row] = value,
            (Self::Entity(column), Value::Entity(value)) => column[row] = value,
            (column, value) => panic!(
                "a lampas {} field holds no {}",
                column.kind().name(),
                value.kind(),
            ),
        }
    }

    fn push(&mut self, value: Value) {
        match (self, value) {
            (Self::Bool(column), Value::Bool(value)) => column.push(value),
            (Self::Int(column), Value::Int(value)) => column.push(value),
            (Self::Float(column), Value::Float(value)) => column.push(value),
            (Self::Text(column), Value::Text(value)) => column.push(value),
            (Self::Vector(column), Value::Vector(value)) => column.push(value),
            (Self::Entity(column), Value::Entity(value)) => column.push(value),
            (column, value) => panic!(
                "a lampas {} field holds no {}",
                column.kind().name(),
                value.kind(),
            ),
        }
    }

    fn swap_remove(&mut self, row: usize) {
        match self {
            Self::Bool(column) => {
                column.swap_remove(row);
            }
            Self::Int(column) => {
                column.swap_remove(row);
            }
            Self::Float(column) => {
                column.swap_remove(row);
            }
            Self::Text(column) => {
                column.swap_remove(row);
            }
            Self::Vector(column) => {
                column.swap_remove(row);
            }
            Self::Entity(column) => {
                column.swap_remove(row);
            }
        }
    }
}

impl Column {
    fn new(kinds: &[FieldKind]) -> Self {
        Self {
            sparse: Vec::new(),
            entities: Vec::new(),
            fields: kinds.iter().copied().map(FieldColumn::of).collect(),
        }
    }

    fn len(&self) -> usize {
        self.entities.len()
    }

    fn dense(&self, entity: Entity) -> Option<usize> {
        let dense = *self.sparse.get(entity.index() as usize)?;
        if dense == VACANT {
            return None;
        }
        let dense = dense as usize;
        (self.entities.get(dense) == Some(&entity)).then_some(dense)
    }

    fn insert(&mut self, entity: Entity, values: &[Value]) {
        assert_eq!(
            values.len(),
            self.fields.len(),
            "a lampas component holds its values",
        );
        match self.dense(entity) {
            Some(row) => {
                for (field, value) in values.iter().enumerate() {
                    self.fields[field].write(row, value.clone());
                }
            }
            None => {
                for (field, value) in values.iter().enumerate() {
                    self.fields[field].push(value.clone());
                }
                let row = self.entities.len();
                let slot = entity.index() as usize;
                if self.sparse.len() <= slot {
                    self.sparse.resize(slot + 1, VACANT);
                }
                self.sparse[slot] = row as u32;
                self.entities.push(entity);
            }
        }
    }

    fn swap_remove(&mut self, row: usize) {
        let entity = self.entities.swap_remove(row);
        self.sparse[entity.index() as usize] = VACANT;
        for field in &mut self.fields {
            field.swap_remove(row);
        }
        if let Some(moved) = self.entities.get(row) {
            self.sparse[moved.index() as usize] = row as u32;
        }
    }

    fn remove(&mut self, entity: Entity) {
        if let Some(row) = self.dense(entity) {
            self.swap_remove(row);
        }
    }
}

impl Store {
    pub fn new(module: &Module) -> Self {
        Self {
            columns: module
                .components
                .iter()
                .map(|component| {
                    let kinds = component
                        .fields
                        .iter()
                        .map(|field| field.kind)
                        .collect::<Vec<_>>();
                    Column::new(&kinds)
                })
                .collect(),
            names: module
                .components
                .iter()
                .map(|component| component.name.clone())
                .collect(),
        }
    }

    pub fn len(&self, component: u16) -> usize {
        self.columns[component as usize].len()
    }

    pub fn entities(&self, component: u16) -> impl Iterator<Item = Entity> + '_ {
        self.columns[component as usize].entities.iter().copied()
    }

    pub fn row(&self, component: u16, entity: Entity) -> Option<u32> {
        self.columns[component as usize]
            .dense(entity)
            .map(|row| row as u32)
    }

    pub fn read(&self, row: u32, component: u16, field: u16) -> Value {
        self.columns[component as usize].fields[field as usize].read(row as usize)
    }

    pub fn write(&mut self, row: u32, component: u16, field: u16, value: Value) {
        self.columns[component as usize].fields[field as usize].write(row as usize, value);
    }

    pub fn read_entity(&self, entity: Entity, component: u16, field: u16) -> Option<Value> {
        let row = self.row(component, entity)?;
        Some(self.read(row, component, field))
    }

    pub fn insert(&mut self, entity: Entity, component: u16, values: &[Value]) {
        self.columns[component as usize].insert(entity, values);
    }

    pub fn remove(&mut self, entity: Entity, component: u16) {
        let column = &mut self.columns[component as usize];
        assert!(
            column.dense(entity).is_some(),
            "the lampas component {} holds no entity {}",
            self.names[component as usize],
            entity.index(),
        );
        column.remove(entity);
    }

    pub fn retain(&mut self, world: &World) {
        for column in &mut self.columns {
            let mut at = 0;
            while at < column.entities.len() {
                if world.is_alive(column.entities[at]) {
                    at += 1;
                } else {
                    column.swap_remove(at);
                }
            }
        }
    }
}

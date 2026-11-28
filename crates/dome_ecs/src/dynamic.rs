use crate::entity::Entity;
use dome_math::Vec3;
use std::sync::Arc;

const VACANT: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Bool,
    Int,
    Float,
    Text,
    Vector,
    Entity,
}

impl Kind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Float => "float",
            Self::Text => "string",
            Self::Vector => "vec3",
            Self::Entity => "entity",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(Arc<str>),
    Vector(Vec3),
    Entity(Entity),
}

impl Value {
    pub fn kind(&self) -> Kind {
        match self {
            Self::Bool(_) => Kind::Bool,
            Self::Int(_) => Kind::Int,
            Self::Float(_) => Kind::Float,
            Self::Text(_) => Kind::Text,
            Self::Vector(_) => Kind::Vector,
            Self::Entity(_) => Kind::Entity,
        }
    }

    pub fn holds(&self, kind: Kind) -> bool {
        self.kind() == kind
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    name: String,
    fields: Vec<Kind>,
}

impl Layout {
    pub fn new(name: impl Into<String>, fields: Vec<Kind>) -> Self {
        let name = name.into();
        assert!(
            !name.is_empty(),
            "a dynamic component holds a name to be declared by",
        );
        Self { name, fields }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub struct DynamicColumn {
    layout: Layout,
    entities: Vec<Entity>,
    slots: Vec<u32>,
    values: Vec<Value>,
}

impl DynamicColumn {
    fn new(layout: Layout) -> Self {
        Self {
            layout,
            entities: Vec::new(),
            slots: Vec::new(),
            values: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.entities.iter().copied()
    }

    pub fn row(&self, entity: Entity) -> Option<u32> {
        self.dense(entity).map(|row| row as u32)
    }

    pub fn read(&self, row: u32, field: u16) -> &Value {
        assert!(
            (row as usize) < self.entities.len() && (field as usize) < self.width(),
            "the {} component holds no field {field} of row {row}",
            self.layout.name(),
        );
        &self.values[row as usize * self.width() + field as usize]
    }

    pub fn read_entity(&self, entity: Entity, field: u16) -> Option<&Value> {
        let row = self.row(entity)?;
        Some(self.read(row, field))
    }

    pub fn write(&mut self, row: u32, field: u16, value: Value) {
        assert!(
            (row as usize) < self.entities.len(),
            "the {} component holds no row {row}",
            self.layout.name(),
        );
        self.check(field, &value);
        let width = self.width();
        self.values[row as usize * width + field as usize] = value;
    }

    pub fn insert(&mut self, entity: Entity, values: &[Value]) {
        assert_eq!(
            values.len(),
            self.width(),
            "the {} component holds {} and {} stand here",
            self.layout.name(),
            self.width(),
            values.len(),
        );
        for (field, value) in values.iter().enumerate() {
            self.check(field as u16, value);
        }
        match self.dense(entity) {
            Some(row) => {
                let width = self.width();
                let at = row * width;
                self.values[at..at + width].clone_from_slice(values);
            }
            None => {
                let row = self.entities.len();
                let slot = entity.index() as usize;
                if self.slots.len() <= slot {
                    self.slots.resize(slot + 1, VACANT);
                }
                self.slots[slot] = row as u32;
                self.entities.push(entity);
                self.values.extend_from_slice(values);
            }
        }
    }

    pub fn remove(&mut self, entity: Entity) -> bool {
        let Some(row) = self.dense(entity) else {
            return false;
        };
        self.swap_remove(row);
        true
    }

    fn swap_remove(&mut self, row: usize) {
        let width = self.width();
        let last = self.entities.len() - 1;
        let removed = self.entities.swap_remove(row);
        self.slots[removed.index() as usize] = VACANT;
        if let Some(moved) = self.entities.get(row) {
            self.slots[moved.index() as usize] = row as u32;
        }
        if row != last {
            for field in 0..width {
                self.values.swap(row * width + field, last * width + field);
            }
        }
        self.values.truncate(last * width);
    }

    fn dense(&self, entity: Entity) -> Option<usize> {
        let row = *self.slots.get(entity.index() as usize)?;
        if row == VACANT {
            return None;
        }
        let row = row as usize;
        (self.entities.get(row) == Some(&entity)).then_some(row)
    }

    fn width(&self) -> usize {
        self.layout.fields.len()
    }

    fn check(&self, field: u16, value: &Value) {
        let kind = *self.layout.fields.get(field as usize).unwrap_or_else(|| {
            panic!(
                "the {} component holds no field {field}",
                self.layout.name(),
            )
        });
        assert!(
            value.holds(kind),
            "the {} component holds a {} at field {field} where {} belongs",
            self.layout.name(),
            value.kind().name(),
            kind.name(),
        );
    }
}

#[derive(Default)]
pub struct Dynamic {
    columns: Vec<DynamicColumn>,
}

impl Dynamic {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self, layout: &[Layout]) -> Dynamic {
        let columns = layout
            .iter()
            .cloned()
            .map(DynamicColumn::new)
            .collect::<Vec<_>>();
        std::mem::replace(self, Self { columns })
    }

    pub fn restore(&mut self, held: Dynamic, layout: &[Layout]) {
        assert_eq!(
            self.columns.len(),
            layout.len(),
            "a restored store keeps the components its layout declares",
        );
        for (at, (held, layout)) in held.columns.into_iter().zip(layout).enumerate() {
            if held.layout == *layout {
                self.columns[at] = held;
            }
        }
    }

    pub fn column(&self, component: u16) -> &DynamicColumn {
        self.columns
            .get(component as usize)
            .unwrap_or_else(|| self.absent(component))
    }

    pub fn column_mut(&mut self, component: u16) -> &mut DynamicColumn {
        let len = self.columns.len();
        self.columns.get_mut(component as usize).unwrap_or_else(|| {
            panic!(
                "the world holds {len} dynamic components and {} is walked",
                component,
            )
        })
    }

    pub fn entities(&self, component: u16) -> impl Iterator<Item = Entity> + '_ {
        self.column(component).entities()
    }

    pub fn row(&self, component: u16, entity: Entity) -> Option<u32> {
        self.column(component).row(entity)
    }

    pub fn read(&self, row: u32, component: u16, field: u16) -> &Value {
        self.column(component).read(row, field)
    }

    pub fn read_entity(&self, entity: Entity, component: u16, field: u16) -> Option<&Value> {
        self.column(component).read_entity(entity, field)
    }

    pub fn write(&mut self, row: u32, component: u16, field: u16, value: Value) {
        self.column_mut(component).write(row, field, value);
    }

    pub fn insert(&mut self, entity: Entity, component: u16, values: &[Value]) {
        self.column_mut(component).insert(entity, values);
    }

    pub fn remove(&mut self, entity: Entity, component: u16) -> bool {
        self.column_mut(component).remove(entity)
    }

    pub fn remove_entity(&mut self, entity: Entity) {
        for column in &mut self.columns {
            column.remove(entity);
        }
    }

    fn absent(&self, component: u16) -> ! {
        panic!(
            "the world holds {} dynamic components and {} is walked",
            self.columns.len(),
            component,
        )
    }
}

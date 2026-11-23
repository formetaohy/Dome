use crate::entity::Entity;
use std::slice;

const VACANT: u32 = u32::MAX;

pub struct Column<T> {
    sparse: Vec<u32>,
    entities: Vec<Entity>,
    values: Vec<T>,
}

impl<T> Column<T> {
    pub fn new() -> Self {
        Self {
            sparse: Vec::new(),
            entities: Vec::new(),
            values: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn contains(&self, entity: Entity) -> bool {
        self.dense(entity).is_some()
    }

    pub fn get(&self, entity: Entity) -> Option<&T> {
        self.values.get(self.dense(entity)?)
    }

    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        let index = self.dense(entity)?;
        self.values.get_mut(index)
    }

    pub fn insert(&mut self, entity: Entity, value: T) -> Option<T> {
        if let Some(index) = self.dense(entity) {
            return Some(std::mem::replace(&mut self.values[index], value));
        }
        let dense = self.entities.len();
        let slot = entity.index() as usize;
        if self.sparse.len() <= slot {
            self.sparse.resize(slot + 1, VACANT);
        }
        self.sparse[slot] = dense as u32;
        self.entities.push(entity);
        self.values.push(value);
        None
    }

    pub fn remove(&mut self, entity: Entity) -> Option<T> {
        let dense = self.dense(entity)?;
        self.sparse[entity.index() as usize] = VACANT;
        self.entities.swap_remove(dense);
        let removed = self.values.swap_remove(dense);
        if let Some(moved) = self.entities.get(dense) {
            self.sparse[moved.index() as usize] = dense as u32;
        }
        Some(removed)
    }

    pub fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.entities.iter().copied()
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            entities: self.entities.iter().copied(),
            values: self.values.iter(),
        }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            entities: self.entities.iter().copied(),
            values: self.values.as_mut_slice(),
        }
    }

    fn dense(&self, entity: Entity) -> Option<usize> {
        let dense = *self.sparse.get(entity.index() as usize)?;
        if dense == VACANT {
            return None;
        }
        let dense = dense as usize;
        (self.entities.get(dense) == Some(&entity)).then_some(dense)
    }
}

impl<T> Default for Column<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Iter<'a, T> {
    entities: std::iter::Copied<slice::Iter<'a, Entity>>,
    values: slice::Iter<'a, T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = (Entity, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        let entity = self.entities.next()?;
        let value = self.values.next()?;
        Some((entity, value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.values.size_hint()
    }
}

impl<T> ExactSizeIterator for Iter<'_, T> {}

pub struct IterMut<'a, T> {
    entities: std::iter::Copied<slice::Iter<'a, Entity>>,
    values: &'a mut [T],
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = (Entity, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        let entity = self.entities.next()?;
        let values = std::mem::take(&mut self.values);
        let (value, rest) = values.split_first_mut()?;
        self.values = rest;
        Some((entity, value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.values.len();
        (remaining, Some(remaining))
    }
}

impl<T> ExactSizeIterator for IterMut<'_, T> {}

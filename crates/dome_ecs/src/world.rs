use crate::column::Column;
use crate::component::Component;
use crate::entity::{Entity, EntityAllocator};
use crate::hierarchy::{LocalTransform, Parent, Tree};
use crate::resource::Resource;
use std::any::{Any, TypeId};
use std::collections::HashMap;

trait ErasedColumn: Send + Sync {
    fn remove_entity(&mut self, entity: Entity);
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Component> ErasedColumn for Column<T> {
    fn remove_entity(&mut self, entity: Entity) {
        self.remove(entity);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Default)]
pub struct World {
    entities: EntityAllocator,
    columns: Vec<Box<dyn ErasedColumn>>,
    index: HashMap<TypeId, usize>,
    resources: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    pub(crate) hierarchy: u64,
    pub(crate) tree: Tree,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn(&mut self) -> Entity {
        self.entities.allocate()
    }

    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.entities.is_alive(entity) {
            return false;
        }
        let mut held = vec![entity];
        let mut at = 0;
        while at < held.len() {
            let children = self.tree.children(held[at]);
            held.extend_from_slice(children);
            at += 1;
        }
        let parent = self.get::<Parent>(entity).map(Parent::entity);
        if let Some(parent) = parent {
            self.tree.detach(entity, parent);
        }
        for dead in &held {
            self.tree.release(*dead);
            for column in &mut self.columns {
                column.remove_entity(*dead);
            }
            self.entities.release(*dead);
            self.hierarchy = self.hierarchy.wrapping_add(1);
        }
        true
    }

    pub fn is_alive(&self, entity: Entity) -> bool {
        self.entities.is_alive(entity)
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.entities.iter()
    }

    pub fn insert<T: Component>(&mut self, entity: Entity, value: T) -> Option<T> {
        assert!(
            self.entities.is_alive(entity),
            "a component belongs to a live entity"
        );
        assert!(
            !Self::holds_parent::<T>(),
            "the parent of entity {} belongs to set_parent",
            entity.index(),
        );
        if Self::moves_hierarchy::<T>() {
            self.bump_hierarchy();
        }
        self.column_or_insert::<T>().insert(entity, value)
    }

    pub fn remove<T: Component>(&mut self, entity: Entity) -> Option<T> {
        assert!(
            !Self::holds_parent::<T>(),
            "the parent of entity {} belongs to clear_parent",
            entity.index(),
        );
        self.column_mut::<T>()?.remove(entity)
    }

    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        self.column::<T>()?.get(entity)
    }

    pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        self.column_mut::<T>()?.get_mut(entity)
    }

    pub fn contains<T: Component>(&self, entity: Entity) -> bool {
        self.column::<T>()
            .is_some_and(|column| column.contains(entity))
    }

    pub fn count<T: Component>(&self) -> usize {
        self.column::<T>().map_or(0, Column::len)
    }

    pub fn column<T: Component>(&self) -> Option<&Column<T>> {
        let index = *self.index.get(&TypeId::of::<T>())?;
        self.columns[index].as_any().downcast_ref()
    }

    pub(crate) fn column_mut<T: Component>(&mut self) -> Option<&mut Column<T>> {
        assert!(
            !Self::holds_parent::<T>(),
            "the parent column belongs to the hierarchy index",
        );
        if Self::moves_hierarchy::<T>() {
            self.bump_hierarchy();
        }
        let index = *self.index.get(&TypeId::of::<T>())?;
        self.columns[index].as_any_mut().downcast_mut()
    }

    pub fn hierarchy_change(&self) -> u64 {
        self.hierarchy
    }

    pub(crate) fn bump_hierarchy(&mut self) {
        self.hierarchy = self.hierarchy.wrapping_add(1);
    }

    pub(crate) fn store<T: Component>(&mut self, entity: Entity, value: T) -> Option<T> {
        self.column_or_insert::<T>().insert(entity, value)
    }

    pub(crate) fn drop_component<T: Component>(&mut self, entity: Entity) -> Option<T> {
        let index = *self.index.get(&TypeId::of::<T>())?;
        self.columns[index]
            .as_any_mut()
            .downcast_mut::<Column<T>>()?
            .remove(entity)
    }

    fn moves_hierarchy<T: Component>() -> bool {
        TypeId::of::<T>() == TypeId::of::<LocalTransform>()
    }

    fn holds_parent<T: Component>() -> bool {
        TypeId::of::<T>() == TypeId::of::<Parent>()
    }

    pub fn iter<T: Component>(&self) -> impl Iterator<Item = (Entity, &T)> + '_ {
        self.column::<T>().into_iter().flat_map(Column::iter)
    }

    pub fn insert_resource<T: Resource>(&mut self, resource: T) -> Option<T> {
        self.resources
            .insert(TypeId::of::<T>(), Box::new(resource))
            .map(|held| {
                *held
                    .downcast::<T>()
                    .expect("a resource keeps the type it was inserted with")
            })
    }

    pub fn remove_resource<T: Resource>(&mut self) -> Option<T> {
        self.resources.remove(&TypeId::of::<T>()).map(|held| {
            *held
                .downcast::<T>()
                .expect("a resource keeps the type it was inserted with")
        })
    }

    pub fn resource<T: Resource>(&self) -> Option<&T> {
        self.resources.get(&TypeId::of::<T>()).map(|held| {
            held.downcast_ref::<T>()
                .expect("a resource keeps the type it was inserted with")
        })
    }

    pub fn resource_mut<T: Resource>(&mut self) -> Option<&mut T> {
        self.resources.get_mut(&TypeId::of::<T>()).map(|held| {
            held.downcast_mut::<T>()
                .expect("a resource keeps the type it was inserted with")
        })
    }

    pub fn contains_resource<T: Resource>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<T>())
    }

    pub fn iter_mut<T: Component>(&mut self) -> impl Iterator<Item = (Entity, &mut T)> + '_ {
        self.column_mut::<T>()
            .into_iter()
            .flat_map(Column::iter_mut)
    }

    pub fn for_each_pair<A: Component, B: Component>(&self, mut each: impl FnMut(Entity, &A, &B)) {
        let Some(first) = self.column::<A>() else {
            return;
        };
        let Some(second) = self.column::<B>() else {
            return;
        };
        for (entity, first) in first.iter() {
            if let Some(second) = second.get(entity) {
                each(entity, first, second);
            }
        }
    }

    pub fn for_each_pair_mut<A: Component, B: Component>(
        &mut self,
        mut each: impl FnMut(Entity, &mut A, &mut B),
    ) {
        let Some((first, second)) = self.columns_mut::<A, B>() else {
            return;
        };
        for (entity, first) in first.iter_mut() {
            if let Some(second) = second.get_mut(entity) {
                each(entity, first, second);
            }
        }
    }

    fn column_or_insert<T: Component>(&mut self) -> &mut Column<T> {
        let key = TypeId::of::<T>();
        let index = if let Some(index) = self.index.get(&key).copied() {
            index
        } else {
            let index = self.columns.len();
            self.columns.push(Box::new(Column::<T>::new()));
            self.index.insert(key, index);
            index
        };
        self.columns[index]
            .as_any_mut()
            .downcast_mut()
            .expect("a column keeps the component type it was created for")
    }

    fn columns_mut<A: Component, B: Component>(
        &mut self,
    ) -> Option<(&mut Column<A>, &mut Column<B>)> {
        let left = *self.index.get(&TypeId::of::<A>())?;
        let right = *self.index.get(&TypeId::of::<B>())?;
        assert!(
            left != right,
            "a system mutably borrows one component column twice"
        );
        let (low, high) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        let (head, tail) = self.columns.split_at_mut(high);
        let (low_column, high_column) = (&mut head[low], &mut tail[0]);
        let (first, second) = if left < right {
            (
                low_column.as_any_mut().downcast_mut::<Column<A>>()?,
                high_column.as_any_mut().downcast_mut::<Column<B>>()?,
            )
        } else {
            (
                high_column.as_any_mut().downcast_mut::<Column<A>>()?,
                low_column.as_any_mut().downcast_mut::<Column<B>>()?,
            )
        };
        Some((first, second))
    }
}

impl std::fmt::Debug for World {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("World")
            .field("entities", &self.entities.len())
            .field("columns", &self.columns.len())
            .field("resources", &self.resources.len())
            .finish()
    }
}

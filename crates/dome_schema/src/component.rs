use crate::assets::Assets;
use dome_ecs::{Entity, World};
use dome_scene::{Axis, Changed, Component, ComponentSpec, Effect, Scene, Value};
use dome_toml::Toml;
use std::any::Any;

pub trait SceneComponent: Any + Clone + PartialEq + Send + Sync {
    const SPEC: &'static ComponentSpec;

    fn read(&self, field: &str) -> Option<Value>;

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String>;

    fn authored(&self) -> Toml;

    fn observed(&self) -> Toml;

    fn runtime(&self) -> Toml {
        self.observed()
    }

    fn install(
        &self,
        scene: &Scene,
        node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    );

    fn validate(&self, _scene: &Scene, _node: &str) -> Result<(), String> {
        Ok(())
    }

    fn stale(&self, changed: &Changed) -> bool {
        changed.references(|kind, id| self.references(kind, id))
    }

    fn refresh(
        &self,
        scene: &Scene,
        node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    ) {
        Self::remove(world, entity);
        self.install(scene, node, world, entity, assets);
    }

    fn create(scene: &Scene) -> Result<Self, String>;

    fn from_toml(value: Toml, what: &str) -> Result<Self, String>;

    fn observe(world: &World, entity: Entity) -> Option<Self>;

    fn remove(world: &mut World, entity: Entity);

    fn references(&self, _kind: dome_scene::ReferenceKind, _id: &str) -> bool {
        false
    }

    fn remap(&mut self, _prefix: &str) {}

    fn write_world(
        _world: &mut World,
        _entity: Entity,
        field: &str,
        _axis: Option<Axis>,
        _value: &Value,
    ) -> Result<(), String> {
        Err(format!(
            "the {} component edits no runtime field {field}",
            Self::SPEC.name
        ))
    }
}

pub struct Erased<T>(pub T);

impl<T: SceneComponent> Component for Erased<T> {
    fn name(&self) -> &'static str {
        T::SPEC.name
    }

    fn clone_box(&self) -> Box<dyn Component> {
        Box::new(Self(self.0.clone()))
    }

    fn equals(&self, other: &dyn Component) -> bool {
        other
            .as_any()
            .downcast_ref::<T>()
            .is_some_and(|other| self.0 == *other)
    }

    fn read(&self, field: &str) -> Option<Value> {
        SceneComponent::read(&self.0, field)
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        SceneComponent::write(&mut self.0, field, axis, value)
    }

    fn authored(&self) -> Toml {
        SceneComponent::authored(&self.0)
    }

    fn observed(&self) -> Toml {
        SceneComponent::observed(&self.0)
    }

    fn runtime(&self) -> Toml {
        SceneComponent::runtime(&self.0)
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        SceneComponent::validate(&self.0, scene, node)
    }

    fn as_any(&self) -> &dyn Any {
        &self.0
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        &mut self.0
    }

    fn references(&self, kind: dome_scene::ReferenceKind, id: &str) -> bool {
        SceneComponent::references(&self.0, kind, id)
    }

    fn remap(&mut self, prefix: &str) {
        SceneComponent::remap(&mut self.0, prefix)
    }
}

pub trait NodeComponents: Sized {
    fn with<T: SceneComponent>(self, component: T) -> Self;

    fn insert<T: SceneComponent>(&mut self, component: T) -> bool;

    fn component<T: SceneComponent>(&self) -> Option<&T>;

    fn component_mut<T: SceneComponent>(&mut self) -> Option<&mut T>;
}

impl NodeComponents for dome_scene::Node {
    fn with<T: SceneComponent>(mut self, component: T) -> Self {
        self.insert(component);
        self
    }

    fn insert<T: SceneComponent>(&mut self, component: T) -> bool {
        self.insert_component(Box::new(Erased(component))).is_none()
    }

    fn component<T: SceneComponent>(&self) -> Option<&T> {
        self.component_by_name(T::SPEC.name)?
            .as_any()
            .downcast_ref()
    }

    fn component_mut<T: SceneComponent>(&mut self) -> Option<&mut T> {
        self.component_by_name_mut(T::SPEC.name)?
            .as_any_mut()
            .downcast_mut()
    }
}

pub(crate) trait Runtime {
    fn stale(&self, changed: &Changed) -> bool;

    fn refresh(
        &self,
        scene: &Scene,
        node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    );

    fn replace(
        &self,
        scene: &Scene,
        node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    );
}

impl<T: SceneComponent> Runtime for T {
    fn stale(&self, changed: &Changed) -> bool {
        SceneComponent::stale(self, changed)
    }

    fn refresh(
        &self,
        scene: &Scene,
        node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    ) {
        SceneComponent::refresh(self, scene, node, world, entity, assets)
    }

    fn replace(
        &self,
        scene: &Scene,
        node: &str,
        world: &mut World,
        entity: Entity,
        assets: &dyn Assets,
    ) {
        T::remove(world, entity);
        SceneComponent::install(self, scene, node, world, entity, assets)
    }
}

pub(crate) fn runtime<T: SceneComponent>(component: &dyn Component) -> &dyn Runtime {
    component
        .as_any()
        .downcast_ref::<T>()
        .expect("a scene component keeps the type its registry names")
}

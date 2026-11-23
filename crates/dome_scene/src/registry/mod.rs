mod animation;
mod audio_bus;
mod audio_compressor;
mod audio_filter;
mod audio_listener;
mod audio_reverb;
mod audio_source;
mod body;
mod camera;
mod collider;
mod directional_light;
mod environment;
mod hidden;
mod joint;
mod mesh;
mod point_light;
mod skin;
mod transform;

use crate::body::BodyRef;
use crate::change::Changed;
use crate::collider::ColliderRef;
use crate::field::Axis;
use crate::joint::JointRef;
use crate::node::{AnimationRef, MeshRef, SkinRef};
use crate::scene::Scene;
use crate::schema::{ComponentSpec, ReferenceKind};
use crate::skin::SkinData;
use crate::target::valid_id;
use crate::value::{Effect, Value};
pub use audio_bus::AudioBusRef;
pub use audio_compressor::AudioCompressorRef;
pub use audio_filter::AudioFilterRef;
pub use audio_listener::AudioListenerRef;
pub use audio_reverb::AudioReverbRef;
pub use audio_source::AudioSourceRef;
use dome_animation::Clip;
use dome_ecs::{Entity, World};
use dome_render::{Camera, DirectionalLight, Hidden, Mesh, PointLight};
use dome_toml::Toml;
use std::any::Any;
use std::sync::Arc;

pub trait Assets {
    fn mesh(&self, id: &str) -> Mesh;

    fn environment(&self, id: &str) -> dome_render::Environment {
        panic!("the assets hold no environment {id}")
    }

    fn capture(&self, _id: &str) -> Option<dome_render::CaptureProbe> {
        None
    }

    fn material(&self, id: &str) -> dome_render::Material;
    fn skin(&self, id: &str) -> SkinData;
    fn animation(&self, id: &str) -> Arc<Clip>;

    fn mesh_shape(&self, id: &str) -> Arc<dome_physics::TriangleMesh> {
        panic!("the assets hold no mesh shape {id}")
    }

    fn sound(&self, id: &str) -> dome_audio::Source {
        panic!("the assets hold no sound {id}")
    }
}

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

    fn from_toml(value: Toml, what: &str) -> Self;

    fn observe(world: &World, entity: Entity) -> Option<Self>;

    fn remove(world: &mut World, entity: Entity);

    fn references(&self, _kind: ReferenceKind, _id: &str) -> bool {
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

pub(crate) trait ErasedComponent: Send + Sync {
    fn name(&self) -> &'static str;

    fn clone_box(&self) -> Box<dyn ErasedComponent>;

    fn equals(&self, other: &dyn ErasedComponent) -> bool;

    fn read(&self, field: &str) -> Option<Value>;

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String>;

    fn authored(&self) -> Toml;

    fn observed(&self) -> Toml;

    fn runtime(&self) -> Toml;

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String>;

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

    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn references(&self, kind: ReferenceKind, id: &str) -> bool;

    fn remap(&mut self, prefix: &str);
}

impl<T: SceneComponent> ErasedComponent for T {
    fn name(&self) -> &'static str {
        T::SPEC.name
    }

    fn clone_box(&self) -> Box<dyn ErasedComponent> {
        Box::new(self.clone())
    }

    fn equals(&self, other: &dyn ErasedComponent) -> bool {
        other
            .as_any()
            .downcast_ref::<T>()
            .is_some_and(|other| self == other)
    }

    fn read(&self, field: &str) -> Option<Value> {
        SceneComponent::read(self, field)
    }

    fn write(&mut self, field: &str, axis: Option<Axis>, value: &Value) -> Result<Effect, String> {
        SceneComponent::write(self, field, axis, value)
    }

    fn authored(&self) -> Toml {
        SceneComponent::authored(self)
    }

    fn observed(&self) -> Toml {
        SceneComponent::observed(self)
    }

    fn runtime(&self) -> Toml {
        SceneComponent::runtime(self)
    }

    fn validate(&self, scene: &Scene, node: &str) -> Result<(), String> {
        SceneComponent::validate(self, scene, node)
    }

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

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn references(&self, kind: ReferenceKind, id: &str) -> bool {
        SceneComponent::references(self, kind, id)
    }

    fn remap(&mut self, prefix: &str) {
        SceneComponent::remap(self, prefix)
    }
}

type WorldWriter = fn(&mut World, Entity, &str, Option<Axis>, &Value) -> Result<(), String>;

pub(crate) struct ComponentRegistration {
    pub(crate) spec: &'static ComponentSpec,
    create: fn(&Scene) -> Result<Box<dyn ErasedComponent>, String>,
    parse: fn(Toml, &str) -> Box<dyn ErasedComponent>,
    pub(crate) observe: fn(&World, Entity) -> Option<Box<dyn ErasedComponent>>,
    pub(crate) remove: fn(&mut World, Entity),
    pub(crate) write_world: WorldWriter,
}

pub struct Registry {
    registrations: Vec<ComponentRegistration>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            registrations: Vec::new(),
        }
    }

    pub fn register<T: SceneComponent>(&mut self) -> &mut Self {
        let spec = T::SPEC;
        assert!(
            valid_id(spec.name),
            "the component {} holds no letter, digit, _ or -",
            spec.name,
        );
        assert!(
            self.registration(spec.name).is_none(),
            "the registry holds the component {} twice",
            spec.name,
        );
        self.registrations.push(ComponentRegistration {
            spec,
            create: create::<T>,
            parse: parse::<T>,
            observe: observe::<T>,
            remove: T::remove,
            write_world: T::write_world,
        });
        self
    }

    pub fn components(&self) -> impl Iterator<Item = &'static ComponentSpec> + '_ {
        self.registrations
            .iter()
            .map(|registration| registration.spec)
    }

    pub fn runtime_paths(&self) -> Vec<String> {
        self.registrations
            .iter()
            .flat_map(|registration| {
                registration
                    .spec
                    .runtime_fields
                    .iter()
                    .map(move |field| format!("{}.{field}", registration.spec.name))
            })
            .collect()
    }

    pub(crate) fn registration(&self, name: &str) -> Option<&ComponentRegistration> {
        self.registrations
            .iter()
            .find(|registration| registration.spec.name == name)
    }

    pub(crate) fn registrations(&self) -> impl Iterator<Item = &ComponentRegistration> + '_ {
        self.registrations.iter()
    }

    pub(crate) fn create(
        &self,
        scene: &Scene,
        name: &str,
    ) -> Result<Box<dyn ErasedComponent>, String> {
        let registration = self
            .registration(name)
            .ok_or_else(|| format!("the scene holds no component {name}"))?;
        (registration.create)(scene)
    }

    pub(crate) fn parse(&self, name: &str, value: Toml, what: &str) -> Box<dyn ErasedComponent> {
        let registration = self
            .registration(name)
            .unwrap_or_else(|| panic!("{what} holds the unknown component {name}"));
        (registration.parse)(value, what)
    }
}

impl Default for Registry {
    fn default() -> Self {
        let mut registry = Self::new();
        registry
            .register::<dome_math::Transform>()
            .register::<Camera>()
            .register::<DirectionalLight>()
            .register::<crate::environment::EnvironmentRef>()
            .register::<PointLight>()
            .register::<MeshRef>()
            .register::<SkinRef>()
            .register::<AnimationRef>()
            .register::<AudioBusRef>()
            .register::<AudioFilterRef>()
            .register::<AudioCompressorRef>()
            .register::<AudioReverbRef>()
            .register::<AudioSourceRef>()
            .register::<AudioListenerRef>()
            .register::<BodyRef>()
            .register::<ColliderRef>()
            .register::<JointRef>()
            .register::<Hidden>();
        registry
    }
}

fn create<T: SceneComponent>(scene: &Scene) -> Result<Box<dyn ErasedComponent>, String> {
    T::create(scene).map(|component| Box::new(component) as Box<dyn ErasedComponent>)
}

fn parse<T: SceneComponent>(value: Toml, what: &str) -> Box<dyn ErasedComponent> {
    Box::new(T::from_toml(value, what))
}

fn observe<T: SceneComponent>(world: &World, entity: Entity) -> Option<Box<dyn ErasedComponent>> {
    T::observe(world, entity).map(|component| Box::new(component) as Box<dyn ErasedComponent>)
}

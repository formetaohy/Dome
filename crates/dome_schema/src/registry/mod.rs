use crate::component::NodeComponents;
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

use crate::body::BodyRef;
use crate::collider::ColliderRef;
use crate::component::{Erased, Runtime, SceneComponent, runtime};
use crate::joint::JointRef;
use crate::node::{AnimationRef, MeshRef, SkinRef};
pub use audio_bus::AudioBusRef;
pub use audio_compressor::AudioCompressorRef;
pub use audio_filter::AudioFilterRef;
pub use audio_listener::AudioListenerRef;
pub use audio_reverb::AudioReverbRef;
pub use audio_source::AudioSourceRef;
use dome_ecs::{Entity, World};
use dome_render::{Camera, DirectionalLight, Hidden, PointLight};
use dome_scene::Scene;
use dome_scene::Value;
use dome_scene::field::Axis;
use dome_scene::valid_id;
use dome_scene::{Component, ComponentSpec, Schema};
use dome_toml::Toml;

type WorldWriter = fn(&mut World, Entity, &str, Option<Axis>, &Value) -> Result<(), String>;

pub(crate) struct ComponentRegistration {
    pub(crate) spec: &'static ComponentSpec,
    create: fn(&Scene) -> Result<Box<dyn Component>, String>,
    parse: fn(Toml, &str) -> Box<dyn Component>,
    observe: fn(&World, Entity) -> Option<Box<dyn Component>>,
    remove: fn(&mut World, Entity),
    write_world: WorldWriter,
    pub(crate) runtime: for<'a> fn(&'a dyn Component) -> &'a dyn Runtime,
}

pub struct Registry {
    registrations: Vec<ComponentRegistration>,
    specs: Vec<&'static ComponentSpec>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            registrations: Vec::new(),
            specs: Vec::new(),
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
        self.specs.push(spec);
        self.registrations.push(ComponentRegistration {
            spec,
            create: create::<T>,
            parse: parse::<T>,
            observe: observe::<T>,
            remove: T::remove,
            write_world: T::write_world,
            runtime: runtime::<T>,
        });
        self
    }

    pub fn components(&self) -> &[&'static ComponentSpec] {
        &self.specs
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

    pub(crate) fn check(&self, scene: &Scene) {
        assert_eq!(
            self.registrations.len(),
            scene.schema().specs().len(),
            "a synced scene holds the components its registry lists",
        );
        for registration in &self.registrations {
            let spec = scene.schema().spec(registration.spec.name);
            assert!(
                spec.is_some_and(|spec| std::ptr::eq(spec, registration.spec)),
                "the scene and its sync disagree about the {} component",
                registration.spec.name,
            );
        }
    }
    pub(crate) fn registration(&self, name: &str) -> Option<&ComponentRegistration> {
        self.registrations
            .iter()
            .find(|registration| registration.spec.name == name)
    }

    pub(crate) fn registrations(&self) -> &[ComponentRegistration] {
        &self.registrations
    }

    pub(crate) fn observe(
        &self,
        name: &str,
        world: &World,
        entity: Entity,
    ) -> Option<Box<dyn Component>> {
        let registration = self.registration(name)?;
        (registration.observe)(world, entity)
    }

    pub(crate) fn remove(&self, name: &str, world: &mut World, entity: Entity) {
        if let Some(registration) = self.registration(name) {
            (registration.remove)(world, entity);
        }
    }

    pub(crate) fn write_world(
        &self,
        name: &str,
        world: &mut World,
        entity: Entity,
        field: &str,
        axis: Option<Axis>,
        value: &Value,
    ) -> Result<(), String> {
        let registration = self
            .registration(name)
            .ok_or_else(|| format!("the scene holds no component {name}"))?;
        (registration.write_world)(world, entity, field, axis, value)
    }
}

impl Schema for Registry {
    fn specs(&self) -> &[&'static ComponentSpec] {
        &self.specs
    }

    fn spec(&self, name: &str) -> Option<&'static ComponentSpec> {
        self.registration(name)
            .map(|registration| registration.spec)
    }

    fn parse(&self, name: &str, value: Toml, what: &str) -> Box<dyn Component> {
        let registration = self
            .registration(name)
            .unwrap_or_else(|| panic!("{what} holds the unknown component {name}"));
        (registration.parse)(value, what)
    }

    fn create(&self, scene: &Scene, name: &str) -> Result<Box<dyn Component>, String> {
        let registration = self
            .registration(name)
            .ok_or_else(|| format!("the scene holds no component {name}"))?;
        (registration.create)(scene)
    }

    fn validate(&self, scene: &Scene) -> Result<(), String> {
        let links = scene
            .nodes()
            .iter()
            .filter_map(|node| {
                node.component::<AudioBusRef>()
                    .map(|bus| dome_audio::BusLink {
                        id: node.id.as_str(),
                        parent: bus.parent.as_ref().map(|parent| parent.as_str()),
                        send: bus.send.as_ref().map(|send| send.as_str()),
                    })
            })
            .collect::<Vec<_>>();
        dome_audio::validate_buses(&links)
    }
}

impl Default for Registry {
    fn default() -> Self {
        let mut registry = Self::new();
        registry
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

fn create<T: SceneComponent>(scene: &Scene) -> Result<Box<dyn Component>, String> {
    T::create(scene).map(|component| Box::new(Erased(component)) as Box<dyn Component>)
}

fn parse<T: SceneComponent>(value: Toml, what: &str) -> Box<dyn Component> {
    Box::new(Erased(T::from_toml(value, what)))
}

fn observe<T: SceneComponent>(world: &World, entity: Entity) -> Option<Box<dyn Component>> {
    T::observe(world, entity).map(|component| Box::new(Erased(component)) as Box<dyn Component>)
}

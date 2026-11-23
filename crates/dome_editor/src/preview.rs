use crate::action::Action;
use crate::camera;
use crate::debug;
use dome_asset::{Library, Refresh};
use dome_ecs::{Entity, World, propagate};
use dome_gpu::{
    AddressMode, Filter, GpuContext, SamplerDesc, Texture, TextureFormat, TextureUsages,
};
use dome_math::{Quat, Transform, Vec3};
use dome_render::{Camera, DirectionalLight, PointLight, Renderer, Target};
use dome_scene::Scene;
use dome_schema::{Registry, SceneSync};
use dome_ui::Ui;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

const ASSET_UPLOAD_BYTES: u64 = 8 << 20;
pub(crate) const PREVIEW_SAMPLER: SamplerDesc = SamplerDesc {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mip_filter: Filter::Linear,
    address_u: AddressMode::ClampToEdge,
    address_v: AddressMode::ClampToEdge,
    compare: None,
};

pub(crate) struct Preview {
    world: World,
    renderer: Renderer,
    library: Option<Library>,
    binding: SceneSync,
    camera: Entity,
    fallback: Option<Entity>,
    preview_texture: Texture,
    debug: bool,
    wanted: Option<Scene>,
    requested: Option<Scene>,
}

impl Preview {
    pub(crate) fn new(context: &GpuContext, scene: &Scene, registry: Arc<Registry>) -> Self {
        let mut world = World::new();
        let camera = world.spawn();
        let renderer = Renderer::new(context, TextureFormat::Rgba8Unorm);
        let preview_texture = renderer.texture(
            "dome asset preview",
            1,
            1,
            1,
            TextureFormat::Rgba8Unorm,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        renderer.write_texture(&preview_texture, &[&[255, 255, 255, 255]]);
        let mut preview = Self {
            world,
            renderer,
            library: None,
            binding: SceneSync::new(registry),
            camera,
            fallback: None,
            preview_texture,
            debug: false,
            wanted: None,
            requested: None,
        };
        let mut library = Library::new();
        assert!(library.request(scene), "a new preview loads its scene");
        preview.library = Some(library);
        preview.wanted = Some(scene.clone());
        preview.requested = Some(scene.clone());
        preview.settle();
        preview.world.insert(camera, Transform::IDENTITY);
        preview.world.insert(camera, camera::editor());
        preview
    }

    pub(crate) fn refresh(&mut self, scene: &Scene) {
        self.wanted = Some(scene.clone());
        self.start();
    }

    pub(crate) fn poll(&mut self) -> Option<Refresh> {
        self.start();
        let refresh = {
            let library = self
                .library
                .as_mut()
                .expect("a preview holds the assets its scene renders");
            library.poll(&mut self.renderer, ASSET_UPLOAD_BYTES)?
        };
        let failed = self.requested.take();
        if !refresh.applied {
            if self.wanted.as_ref() == failed.as_ref() {
                self.wanted = None;
            }
            return Some(refresh);
        }
        let requested = failed.expect("a polled refresh completes the request the preview made");
        {
            let library = self
                .library
                .as_ref()
                .expect("a preview holds the assets its scene renders");
            self.binding
                .sync(&requested, &mut self.world, library, &refresh.changed);
        }
        if self.wanted.as_ref() == Some(&requested) {
            self.wanted = None;
        }
        let library = self
            .library
            .as_mut()
            .expect("a preview holds the assets its scene renders");
        library.release(&mut self.renderer);
        self.settle();
        Some(refresh)
    }

    fn start(&mut self) {
        if self.wanted.is_none() || self.library.as_ref().is_some_and(Library::loading) {
            return;
        }
        let wanted = self
            .wanted
            .as_ref()
            .expect("a wanted preview scene waits for its assets");
        let library = self
            .library
            .as_mut()
            .expect("a preview holds the assets its scene renders");
        if library.request(wanted) {
            self.requested = Some(wanted.clone());
        }
    }

    fn settle(&mut self) {
        for entity in self.binding.entities().to_vec() {
            self.world.remove::<Camera>(entity);
        }
        self.settle_light();
    }

    pub(crate) fn apply(&mut self, scene: &Scene, index: usize) {
        let Some(library) = &self.library else {
            return;
        };
        if library.loading() {
            self.wanted = Some(scene.clone());
            return;
        }
        self.binding.apply(scene, &mut self.world, library, index);
        if let Some(entity) = self.binding.entity(index) {
            self.world.remove::<Camera>(entity);
        }
    }

    fn settle_light(&mut self) {
        let lit = self
            .world
            .iter::<DirectionalLight>()
            .any(|(entity, _)| Some(entity) != self.fallback)
            || self
                .world
                .iter::<PointLight>()
                .any(|(entity, _)| Some(entity) != self.fallback);
        match (lit, self.fallback) {
            (true, Some(entity)) => {
                self.world.despawn(entity);
                self.fallback = None;
            }
            (false, None) => {
                let entity = self.world.spawn();
                self.world.insert(
                    entity,
                    Transform::from_rotation(Quat::from_rotation_x(-0.8)),
                );
                self.world.insert(
                    entity,
                    DirectionalLight::new(Vec3::ONE, std::f32::consts::PI, 100.0),
                );
                self.fallback = Some(entity);
            }
            _ => {}
        }
    }

    pub(crate) fn register_textures(&self, ui: &mut Ui<Action>, scene: &Scene) {
        let Some(library) = &self.library else {
            return;
        };
        let mut images = BTreeSet::new();
        let mut environments = BTreeSet::new();
        for held in std::iter::once(scene).chain(self.scenes()) {
            for image in held.images() {
                images.insert(image.id.as_str());
            }
            for environment in held.environments() {
                environments.insert(environment.id.as_str());
            }
        }
        let mut textures = Vec::with_capacity(images.len() + environments.len());
        for id in images {
            let (texture, sampler) = match library.image_preview(id) {
                Some(image) => (image.texture().clone(), image.sampler()),
                None => (self.preview_texture.clone(), PREVIEW_SAMPLER),
            };
            textures.push((String::from(id), texture, sampler));
        }
        for id in environments {
            let (texture, sampler) = match library.environment_preview(id) {
                Some(environment) => (environment.panorama.clone(), PREVIEW_SAMPLER),
                None => (self.preview_texture.clone(), PREVIEW_SAMPLER),
            };
            textures.push((format!("environment.{id}"), texture, sampler));
        }
        ui.set_textures(
            textures
                .iter()
                .map(|(name, texture, sampler)| (name.as_str(), texture, *sampler)),
        );
    }

    fn scenes(&self) -> impl Iterator<Item = &Scene> + '_ {
        self.binding
            .applied()
            .into_iter()
            .chain(self.wanted.as_ref())
            .chain(self.requested.as_ref())
    }

    pub(crate) fn set_camera(&mut self, transform: Transform) {
        self.world.insert(self.camera, transform);
    }

    pub(crate) fn advance(&mut self, real: Duration) {
        dome_animation::advance(&mut self.world, real.as_secs_f32());
        propagate(&mut self.world);
    }

    pub(crate) fn pick(&mut self, point: dome_math::Vec2) -> Option<String> {
        let entity = self.renderer.pick(point)?;
        self.world
            .get::<dome_schema::SceneNode>(entity)
            .map(|node| node.id.clone())
    }

    pub(crate) fn entity(&self, id: &str) -> Option<Entity> {
        self.binding.id(id)
    }

    pub(crate) fn set_debug(&mut self, debug: bool) {
        self.debug = debug;
    }

    pub(crate) fn loading(&self) -> bool {
        self.wanted.is_some()
            || self.requested.is_some()
            || self.library.as_ref().is_some_and(Library::loading)
    }

    pub(crate) fn capturing(&self) -> bool {
        self.renderer.capturing()
    }

    pub(crate) fn playing(&self) -> bool {
        self.world
            .iter::<dome_animation::AnimationMachine>()
            .any(|(_, machine)| machine.playing())
    }

    pub(crate) fn render(&mut self, target: &Texture, clear: [f32; 4], highlight: Option<Entity>) {
        let lines = debug::viewport(&self.world, self.debug);
        self.world.insert_resource(lines);
        self.renderer
            .render(&mut self.world, Target::Texture(target), clear, highlight);
        dome_render::install_captures(&mut self.world, self.renderer.take_captures());
    }
}

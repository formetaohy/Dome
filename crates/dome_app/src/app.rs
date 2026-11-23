use crate::time::Time;
use dome_asset::{Library, Watch};
use dome_audio::Audio;
use dome_ecs::{Entity, Schedule, System, World, propagate};
use dome_gpu::{
    GpuContext, GpuRequest, Surface, Texture, TextureFormat, TextureUsages, WindowHandle,
};
use dome_math::{Transform, Vec3};
use dome_render::{Camera, Lines, Renderer, Target};
use dome_scene::{Changed, Scene};
use dome_schema::{Registry, SceneSync};
use dome_window::{Event, Flow, Handler, Input, Window, WindowConfig, run as run_window};
use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct AppConfig {
    pub window: WindowConfig,
    pub gpu: GpuRequest,
    pub fixed_hz: f64,
    pub max_fixed_steps: u32,
    pub scene: Option<PathBuf>,
    pub audio: bool,
    pub debug: bool,
    pub registry: Arc<Registry>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window: WindowConfig::default(),
            gpu: GpuRequest::default(),
            fixed_hz: 60.0,
            max_fixed_steps: 8,
            scene: None,
            audio: true,
            debug: false,
            registry: Arc::new(Registry::default()),
        }
    }
}

pub struct HeadlessConfig {
    pub width: u32,
    pub height: u32,
    pub frames: Option<u64>,
    pub realtime: bool,
    pub screenshot: bool,
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            frames: None,
            realtime: false,
            screenshot: false,
        }
    }
}

pub struct Exit;

pub struct ClearColor(pub [f32; 4]);

impl Default for ClearColor {
    fn default() -> Self {
        Self([0.02, 0.02, 0.03, 1.0])
    }
}

type Startup = Box<dyn FnOnce(&mut World, &mut Renderer)>;

pub struct App {
    world: World,
    fixed: Schedule,
    update: Schedule,
    startup: Vec<Startup>,
    config: AppConfig,
    session: Option<Session>,
    reload: Option<String>,
    started: bool,
    headless: bool,
    startup_pending: bool,
    loading_camera: Option<Entity>,
}

enum Presentation {
    Window(Box<Surface>),
    Offscreen(Texture),
}

struct Session {
    context: GpuContext,
    presentation: Presentation,
    renderer: Renderer,
    shot: Option<Texture>,
    width: u32,
    height: u32,
    format: TextureFormat,
    live: Option<LiveScene>,
}

const ASSET_UPLOAD_BYTES: u64 = 8 << 20;

struct LiveScene {
    path: PathBuf,
    scene: Scene,
    assets: Library,
    sync: SceneSync,
    watch: Watch,
    messages: Vec<String>,
    wanted: Option<Scene>,
    requested: Option<Scene>,
    renew: bool,
}

struct Reload {
    renew: bool,
    error: Option<String>,
}

impl LiveScene {
    fn new(
        path: PathBuf,
        registry: &Arc<Registry>,
        renderer: &mut Renderer,
        world: &mut World,
        blocking: bool,
    ) -> Self {
        let scene = Scene::load(&path, registry.clone());
        let mut assets = Library::new();
        let mut sync = SceneSync::new(registry.clone());
        let mut messages = Vec::new();
        let (wanted, requested) = if blocking {
            let refresh = assets.refresh(&scene, renderer);
            assert!(
                refresh.applied,
                "the library adopts the assets of its scene: {}",
                refresh.message(),
            );
            if !refresh.errors.is_empty() {
                messages.push(refresh.message());
            }
            sync.sync(&scene, world, &assets, &Changed::none());
            (None, None)
        } else {
            assert!(assets.request(&scene), "a new library loads its scene");
            (Some(scene.clone()), Some(scene.clone()))
        };
        let watch = Watch::new(&path, &scene);
        Self {
            path,
            scene,
            assets,
            sync,
            watch,
            messages,
            wanted,
            requested,
            renew: false,
        }
    }

    fn update(
        &mut self,
        world: &mut World,
        registry: &Arc<Registry>,
        renderer: &mut Renderer,
        blocking: bool,
    ) -> Option<Reload> {
        if let Some(change) = self.watch.changed() {
            self.renew = true;
            if change.scene {
                match catch_unwind(AssertUnwindSafe(|| {
                    Scene::load(&self.path, registry.clone())
                })) {
                    Ok(scene) => {
                        self.watch.renew(&scene);
                        self.messages.extend(self.watch.errors().map(String::from));
                        self.wanted = Some(scene);
                    }
                    Err(payload) => {
                        self.messages
                            .push(format!("reload: {}", panic_text(payload)));
                        return Some(self.reloaded());
                    }
                }
            } else {
                if change.model {
                    self.watch.renew(&self.scene);
                    self.messages.extend(self.watch.errors().map(String::from));
                }
                self.wanted = Some(self.scene.clone());
            }
        }
        if !self.assets.loading()
            && let Some(wanted) = self.wanted.clone()
            && self.assets.request(&wanted)
        {
            self.requested = Some(wanted);
        }
        let refresh = if blocking {
            let mut held = None;
            while self.assets.loading() {
                if let Some(refresh) = self.assets.poll(renderer, u64::MAX) {
                    held = Some(refresh);
                    break;
                }
            }
            held?
        } else {
            self.assets.poll(renderer, ASSET_UPLOAD_BYTES)?
        };
        let failed = self.requested.take();
        if !refresh.applied {
            assert!(
                self.sync.applied().is_some(),
                "the library adopts the assets of its scene: {}",
                refresh.message(),
            );
            if self.wanted.as_ref() == failed.as_ref() {
                self.wanted = None;
            }
            self.messages.push(refresh.message());
            return Some(self.reloaded());
        }
        let requested = failed.expect("a polled refresh completes the request the scene made");
        if !refresh.errors.is_empty() {
            self.messages.push(refresh.message());
        }
        let changed =
            self.sync.applied().is_none() || requested != self.scene || refresh.changed.any();
        if changed {
            self.sync
                .sync(&requested, world, &self.assets, &refresh.changed);
            self.scene = requested.clone();
        }
        if self.wanted.as_ref() == Some(&requested) {
            self.wanted = None;
        }
        self.assets.release(renderer);
        Some(self.reloaded())
    }

    fn take_error(&mut self) -> Option<String> {
        joined(&std::mem::take(&mut self.messages))
    }

    fn reloaded(&mut self) -> Reload {
        Reload {
            renew: std::mem::take(&mut self.renew),
            error: self.take_error(),
        }
    }

    fn rewatch(&mut self) {
        self.watch.renew(&self.scene);
        self.messages.extend(self.watch.errors().map(String::from));
    }
}

fn joined(messages: &[String]) -> Option<String> {
    (!messages.is_empty()).then(|| messages.join("; "))
}

fn panic_text(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&str>() {
            Ok(message) => String::from(*message),
            Err(_) => String::from("a panic without a message"),
        },
    }
}

impl Session {
    fn window(window: &Window, request: &GpuRequest) -> Self {
        let request = GpuRequest {
            surface: Some(WindowHandle::of(
                window.window_handle(),
                window.display_handle(),
            )),
            ..request.clone()
        };
        let context = GpuContext::open(&request)
            .unwrap_or_else(|error| panic!("the engine opens its render device: {error}"));
        let (width, height) = window.size();
        assert!(
            width > 0 && height > 0,
            "a window of {width} by {height} presents no surface"
        );
        let surface = context.device().surface(width, height);
        let format = surface.format();
        let renderer = Renderer::new(&context, format);
        Self {
            context,
            presentation: Presentation::Window(Box::new(surface)),
            renderer,
            shot: None,
            width,
            height,
            format,
            live: None,
        }
    }

    fn headless(width: u32, height: u32, request: &GpuRequest) -> Self {
        assert!(
            width > 0 && height > 0,
            "a headless frame of {width} by {height} spans no target"
        );
        assert!(
            request.surface.is_none(),
            "a headless session opens no window surface"
        );
        let context = GpuContext::open(request)
            .unwrap_or_else(|error| panic!("the engine opens its render device: {error}"));
        let format = TextureFormat::Rgba8Unorm;
        let target = Texture::new(
            context.device(),
            "dome headless target",
            width,
            height,
            1,
            format,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        );
        let renderer = Renderer::new(&context, format);
        Self {
            context,
            presentation: Presentation::Offscreen(target),
            renderer,
            shot: None,
            width,
            height,
            format,
            live: None,
        }
    }

    fn load_scene(
        &mut self,
        world: &mut World,
        path: &Path,
        registry: &Arc<Registry>,
        blocking: bool,
    ) {
        self.live = Some(LiveScene::new(
            path.to_path_buf(),
            registry,
            &mut self.renderer,
            world,
            blocking,
        ));
    }

    fn reload(
        &mut self,
        world: &mut World,
        registry: &Arc<Registry>,
        blocking: bool,
    ) -> Option<Reload> {
        let live = self.live.as_mut()?;
        live.update(world, registry, &mut self.renderer, blocking)
    }

    fn rewatch(&mut self) {
        if let Some(live) = &mut self.live {
            live.rewatch();
        }
    }

    fn scene_ready(&self) -> bool {
        self.live
            .as_ref()
            .is_some_and(|live| live.sync.applied().is_some())
    }

    fn draw_window(&mut self, world: &mut World, clear: [f32; 4], width: u32, height: u32) {
        let surface = match &mut self.presentation {
            Presentation::Window(surface) => {
                if surface.width() != width || surface.height() != height {
                    surface.resize(width, height);
                }
                surface.acquire()
            }
            Presentation::Offscreen(_) => panic!("a windowed application presents its surface"),
        };
        let Some(surface) = surface else {
            return;
        };
        self.width = surface.width();
        self.height = surface.height();
        self.renderer
            .render(world, Target::Surface(surface), clear, None);
    }

    fn draw_headless(
        &mut self,
        world: &mut World,
        clear: [f32; 4],
        screenshot: bool,
    ) -> Option<Vec<u8>> {
        let png = if screenshot {
            let width = self.width;
            let height = self.height;
            self.ensure_shot(width, height);
            let shot = self
                .shot
                .as_ref()
                .expect("a captured frame holds its target");
            self.renderer
                .render(world, Target::Texture(shot), clear, None);
            let rgba = shot.read_rgba(self.context.queue());
            Some(dome_image::encode(width, height, &rgba))
        } else {
            None
        };
        if let Presentation::Offscreen(texture) = &self.presentation {
            self.renderer
                .render(world, Target::Texture(texture), clear, None);
        }
        png
    }

    fn ensure_shot(&mut self, width: u32, height: u32) {
        let recreate = self
            .shot
            .as_ref()
            .is_none_or(|texture| texture.width() != width || texture.height() != height);
        if !recreate {
            return;
        }
        self.shot = Some(Texture::new(
            self.context.device(),
            "dome screenshot",
            width,
            height,
            1,
            self.format,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        ));
    }
}

impl App {
    pub fn new() -> Self {
        Self::with_config(AppConfig::default())
    }

    pub fn with_config(config: AppConfig) -> Self {
        assert!(
            config.gpu.surface.is_none(),
            "the application opens the window surface its window names"
        );
        assert!(
            config.max_fixed_steps > 0,
            "an application steps no fixed system"
        );
        let mut world = World::new();
        world.insert_resource(Time::new(config.fixed_hz));
        world.insert_resource(Input::default());
        world.insert_resource(ClearColor::default());
        world.insert_resource(dome_physics::Gravity::default());
        let fixed_seconds = 1.0 / config.fixed_hz as f32;
        let mut fixed = Schedule::new();
        fixed.add(move |world: &mut World| dome_animation::advance(world, fixed_seconds));
        fixed.add(move |world: &mut World| dome_physics::step(world, fixed_seconds));
        Self {
            world,
            fixed,
            update: Schedule::new(),
            startup: Vec::new(),
            config,
            session: None,
            reload: None,
            started: false,
            headless: false,
            startup_pending: false,
            loading_camera: None,
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn reload_error(&self) -> Option<&str> {
        self.reload.as_deref()
    }

    pub fn add_fixed(&mut self, system: impl System) -> &mut Self {
        self.fixed.add(system);
        self
    }

    pub fn add_system(&mut self, system: impl System) -> &mut Self {
        self.update.add(system);
        self
    }

    pub fn add_startup(
        &mut self,
        setup: impl FnOnce(&mut World, &mut Renderer) + 'static,
    ) -> &mut Self {
        self.startup.push(Box::new(setup));
        self
    }

    pub fn run(self) {
        let config = self.config.window.clone();
        run_window(config, self);
    }

    pub fn run_headless(&mut self, config: HeadlessConfig) -> Option<Vec<u8>> {
        self.headless = true;
        assert!(
            config.frames.is_none_or(|frames| frames > 0),
            "a headless run walks at least one frame"
        );
        let mut session = Session::headless(config.width, config.height, &self.config.gpu);
        self.start(&mut session, true);
        let fixed = self
            .world
            .resource::<Time>()
            .expect("an application keeps its time")
            .fixed_delta();
        let started = Instant::now();
        let mut frame = 0u64;
        let mut screenshot = None;
        loop {
            self.reload_scene(&mut session);
            let loading = session
                .live
                .as_ref()
                .is_some_and(|live| live.sync.applied().is_none());
            if loading {
                self.world
                    .resource_mut::<Input>()
                    .expect("an application keeps its input")
                    .begin_frame();
            } else {
                self.step(fixed);
            }
            let exit = self.world.remove_resource::<Exit>().is_some();
            let clear = self
                .world
                .resource::<ClearColor>()
                .expect("an application keeps its clear color")
                .0;
            self.sync_debug();
            frame += 1;
            let last = exit || config.frames.is_some_and(|frames| frame >= frames);
            if let Some(png) =
                session.draw_headless(&mut self.world, clear, config.screenshot && last)
            {
                screenshot = Some(png);
            }
            dome_render::install_captures(&mut self.world, session.renderer.take_captures());
            if last {
                break;
            }
            if config.realtime {
                let next = started + fixed.mul_f64(frame as f64);
                let now = Instant::now();
                if next > now {
                    std::thread::sleep(next - now);
                }
            }
        }
        screenshot
    }

    fn start(&mut self, session: &mut Session, focused: bool) {
        let blocking = self.headless;
        assert!(
            !self.started,
            "a resumed application rebuilds the world a lost surface owned"
        );
        if self.config.audio {
            self.world.insert_resource(Audio::open());
        }
        self.world
            .resource_mut::<Input>()
            .expect("an application keeps its input")
            .handle(Event::Focused(focused));
        if let Some(path) = &self.config.scene {
            session.load_scene(&mut self.world, path, &self.config.registry, blocking);
            self.reload = session.live.as_mut().and_then(LiveScene::take_error);
        }
        if !blocking && self.config.scene.is_some() {
            let camera = self.world.spawn();
            self.world.insert(
                camera,
                Transform::from_translation(Vec3::new(0.0, 1.0, 4.0)),
            );
            self.world
                .insert(camera, Camera::new(std::f32::consts::FRAC_PI_2, 0.1, 100.0));
            self.loading_camera = Some(camera);
        }
        if blocking || self.config.scene.is_none() {
            self.settle_startup(session);
        } else {
            self.startup_pending = !self.startup.is_empty();
        }
        self.started = true;
    }

    fn sync_debug(&mut self) {
        if !self.config.debug {
            let _ = self.world.remove_resource::<Lines>();
            return;
        }
        let mut lines = Lines::new();
        for line in dome_physics::debug_lines(&self.world) {
            lines.push(line.start, line.end, line.color);
        }
        self.world.insert_resource(lines);
    }

    fn step(&mut self, real: Duration) {
        let steps = self
            .world
            .resource_mut::<Time>()
            .expect("an application keeps its time")
            .advance(real, self.config.max_fixed_steps);
        for _ in 0..steps {
            self.fixed.run(&mut self.world);
            propagate(&mut self.world);
            self.world
                .resource_mut::<Time>()
                .expect("an application keeps its time")
                .step();
        }
        self.update.run(&mut self.world);
        propagate(&mut self.world);
        dome_audio::sync(&mut self.world);
        self.world
            .resource_mut::<Time>()
            .expect("an application keeps its time")
            .settle();
        self.world
            .resource_mut::<Input>()
            .expect("an application keeps its input")
            .begin_frame();
    }

    fn set_audio_paused(&mut self, paused: bool) {
        if let Some(audio) = self.world.resource_mut::<Audio>() {
            audio.set_paused(paused);
        }
    }

    fn settle_startup(&mut self, session: &mut Session) {
        for setup in self.startup.drain(..) {
            setup(&mut self.world, &mut session.renderer);
        }
        propagate(&mut self.world);
    }

    fn reload_scene(&mut self, session: &mut Session) {
        let Some(reload) = session.reload(&mut self.world, &self.config.registry, self.headless)
        else {
            return;
        };
        if reload.renew {
            session.rewatch();
        }
        if session.scene_ready()
            && let Some(camera) = self.loading_camera.take()
        {
            self.world.despawn(camera);
        }
        if self.startup_pending && session.scene_ready() {
            self.startup_pending = false;
            self.settle_startup(session);
        }
        self.reload = reload.error;
    }
}

impl Handler for App {
    fn opened(&mut self, window: &Window) {
        let mut session = Session::window(window, &self.config.gpu);
        self.start(&mut session, window.has_focus());
        self.session = Some(session);
    }

    fn frame(&mut self, window: &Window, real: Duration) -> Flow {
        let Some(mut session) = self.session.take() else {
            return Flow::Continuous;
        };
        let flow = self.frame_session(&mut session, window, real);
        self.session = Some(session);
        flow
    }

    fn event(&mut self, _window: &Window, event: Event) {
        self.world
            .resource_mut::<Input>()
            .expect("an application keeps its input")
            .handle(event);
    }

    fn suspended(&mut self, _window: &Window) -> Flow {
        Flow::Exit
    }

    fn closed(&mut self, _window: &Window) {
        self.session = None;
    }
}

impl App {
    fn frame_session(&mut self, session: &mut Session, window: &Window, real: Duration) -> Flow {
        self.reload_scene(session);
        let (width, height) = window.size();
        if window.is_minimized() || width == 0 || height == 0 {
            return Flow::Idle;
        }
        let loading = session
            .live
            .as_ref()
            .is_some_and(|live| live.sync.applied().is_none());
        self.set_audio_paused(loading);
        if loading {
            self.world
                .resource_mut::<Input>()
                .expect("an application keeps its input")
                .begin_frame();
        } else {
            self.step(real);
        }
        let exit = self.world.remove_resource::<Exit>().is_some();
        let clear = self
            .world
            .resource::<ClearColor>()
            .expect("an application keeps its clear color")
            .0;
        self.sync_debug();
        session.draw_window(&mut self.world, clear, width, height);
        dome_render::install_captures(&mut self.world, session.renderer.take_captures());
        if exit { Flow::Exit } else { Flow::Continuous }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

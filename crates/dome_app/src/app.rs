use crate::time::Time;
use dome_asset::{Change, Library, WaitOutcome, Watch};
use dome_audio::Audio;
use dome_ecs::{Entity, Schedule, System, World, propagate, propagate_step, snapshot};
use dome_gpu::{
    Gpu, GpuCondition, GpuEventCursor, GpuFailure, GpuRequest, Surface, Texture, TextureFormat,
    TextureUsages,
};
use dome_math::{Transform, Vec3};
use dome_render::{Camera, Lines, Renderer, Target};
use dome_scene::{Changed, Scene};
use dome_schema::{Registry, SceneSync};
use dome_window::{Event, Flow, Handler, Input, Waker, Window, WindowConfig, run as run_window};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct SceneConfig {
    pub path: PathBuf,
    pub root: PathBuf,
}

pub struct AppConfig {
    pub window: WindowConfig,
    pub gpu: GpuRequest,
    pub fixed_hz: f64,
    pub max_fixed_steps: u32,
    pub scene: Option<SceneConfig>,
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
    pub asset_timeout: Duration,
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            frames: None,
            realtime: false,
            screenshot: false,
            asset_timeout: Duration::from_secs(30),
        }
    }
}

pub struct Exit;

#[derive(Clone, Copy)]
struct AssetLoad {
    blocking: bool,
    timeout: Duration,
}

pub struct ClearColor(pub [f32; 4]);

impl Default for ClearColor {
    fn default() -> Self {
        Self([0.02, 0.02, 0.03, 1.0])
    }
}

type Startup = Box<dyn Fn(&mut World, &mut Renderer)>;
type Observer = Box<dyn Fn(&mut World, Change)>;

pub struct App {
    world: World,
    fixed: Schedule,
    update: Schedule,
    startup: Vec<Startup>,
    reloads: Vec<Observer>,
    config: AppConfig,
    wake: Option<Waker>,
    session: Option<Session>,
    reload: Option<String>,
    started: bool,
    headless: bool,
    startup_pending: bool,
    loading_camera: Option<Entity>,
}

enum Presentation {
    Window(Box<Surface>),
    Offscreen(Box<Texture>),
}

struct Session {
    context: Gpu,
    events: GpuEventCursor,
    presentation: Presentation,
    renderer: Renderer,
    shot: Option<Texture>,
    width: u32,
    height: u32,
    format: TextureFormat,
    live: Option<LiveScene>,
}

struct GpuHealth<'a> {
    context: &'a Gpu,
    events: &'a GpuEventCursor,
}

impl GpuHealth<'_> {
    fn unhealthy(&self) -> bool {
        self.context.poll_events();
        self.events.unhealthy()
    }
}

const ASSET_UPLOAD_BYTES: u64 = 8 << 20;
const RECOVERY_POLL: Duration = Duration::from_millis(2);

struct LiveScene {
    path: PathBuf,
    root: PathBuf,
    scene: Scene,
    assets: Library,
    sync: SceneSync,
    watch: Watch,
    messages: Vec<String>,
    wanted: Option<Scene>,
    requested: Option<Scene>,
    renew: bool,
    change: Change,
    recovering: bool,
}

struct Reload {
    renew: bool,
    error: Option<String>,
}

impl LiveScene {
    fn new(
        file: SceneConfig,
        registry: &Arc<Registry>,
        renderer: &mut Renderer,
        health: &GpuHealth<'_>,
        world: &mut World,
        load: AssetLoad,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let scene = Scene::load(&file.path, file.root.clone(), registry.clone())?;
        let mut assets = Library::new();
        let mut sync = SceneSync::new(registry.clone());
        let mut messages = Vec::new();
        let (wanted, requested) = if load.blocking {
            let outcome = assets.refresh(
                &scene,
                renderer,
                ASSET_UPLOAD_BYTES,
                Instant::now() + load.timeout,
                || health.unhealthy(),
            );
            match outcome {
                WaitOutcome::Ready(refresh) => {
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
                }
                WaitOutcome::Timeout => {
                    return Err(format!(
                        "the scene {} loads its assets within {:?}",
                        file.path.display(),
                        load.timeout,
                    ));
                }
                WaitOutcome::Cancelled => (Some(scene.clone()), Some(scene.clone())),
            }
        } else {
            assert!(assets.request(&scene), "a new library loads its scene");
            (Some(scene.clone()), Some(scene.clone()))
        };
        let watch = Watch::new(&file.path, &scene, wake);
        Ok(Self {
            path: file.path,
            root: file.root,
            scene,
            assets,
            sync,
            watch,
            messages,
            wanted,
            requested,
            renew: false,
            change: Change::default(),
            recovering: false,
        })
    }

    fn unload(&mut self) {
        self.assets = Library::new();
        self.sync.invalidate();
        self.wanted = None;
        self.requested = None;
    }

    fn recover(
        &mut self,
        renderer: &mut Renderer,
        health: &GpuHealth<'_>,
        world: &mut World,
        load: AssetLoad,
    ) -> Result<(), String> {
        self.unload();
        self.recovering = true;
        if !load.blocking {
            assert!(
                self.assets.request(&self.scene),
                "a recovered library loads its scene"
            );
            self.wanted = Some(self.scene.clone());
            self.requested = Some(self.scene.clone());
            return Ok(());
        }
        let outcome = self.assets.refresh(
            &self.scene,
            renderer,
            ASSET_UPLOAD_BYTES,
            Instant::now() + load.timeout,
            || health.unhealthy(),
        );
        match outcome {
            WaitOutcome::Ready(refresh) => {
                assert!(
                    refresh.applied,
                    "a recovered library adopts the assets of its scene: {}",
                    refresh.message(),
                );
                if !refresh.errors.is_empty() {
                    self.messages.push(refresh.message());
                }
                self.sync
                    .sync(&self.scene, world, &self.assets, &Changed::none());
                for (_, probe) in world.iter_mut::<dome_render::CaptureProbe>() {
                    probe.forget();
                }
                self.recovering = false;
                Ok(())
            }
            WaitOutcome::Timeout => Err(format!(
                "the scene {} recovers its assets within {:?}",
                self.path.display(),
                load.timeout,
            )),
            WaitOutcome::Cancelled => Err(format!(
                "the scene {} recovers its assets while its device holds",
                self.path.display(),
            )),
        }
    }

    fn update(
        &mut self,
        world: &mut World,
        registry: &Arc<Registry>,
        renderer: &mut Renderer,
        health: &GpuHealth<'_>,
        load: AssetLoad,
    ) -> Option<Reload> {
        if let Some(report) = self.watch.changed() {
            if let Some(errors) = report.errors
                && !errors.is_empty()
            {
                self.messages.push(errors.join("; "));
            }
            let change = report.change;
            self.renew = true;
            self.change.absorb(change);
            if change.scene || change.prefab {
                match Scene::load(&self.path, self.root.clone(), registry.clone()) {
                    Ok(scene) => self.wanted = Some(scene),
                    Err(error) => {
                        self.messages.push(format!("reload: {error}"));
                        return Some(self.reloaded());
                    }
                }
            } else if change.model || change.asset {
                self.wanted = Some(self.scene.clone());
            }
        }
        if !self.assets.loading()
            && let Some(wanted) = self.wanted.clone()
            && self.assets.request(&wanted)
        {
            self.requested = Some(wanted);
        }
        let refresh = if load.blocking {
            if !self.assets.loading() {
                return None;
            }
            match self.assets.wait(
                renderer,
                ASSET_UPLOAD_BYTES,
                Instant::now() + load.timeout,
                || health.unhealthy(),
            ) {
                WaitOutcome::Ready(refresh) => refresh,
                WaitOutcome::Timeout => panic!(
                    "the scene {} reloads its assets within {:?}",
                    self.path.display(),
                    load.timeout,
                ),
                WaitOutcome::Cancelled => return None,
            }
        } else {
            self.assets.poll(renderer, ASSET_UPLOAD_BYTES)?
        };
        let failed = self.requested.take();
        if !refresh.applied {
            assert!(
                !self.recovering,
                "the scene {} adopts the assets it recovers: {}",
                self.path.display(),
                refresh.message(),
            );
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
        let recovered = std::mem::take(&mut self.recovering);
        let changed = recovered
            || self.sync.applied().is_none()
            || requested != self.scene
            || refresh.changed.any();
        if changed {
            self.sync
                .sync(&requested, world, &self.assets, &refresh.changed);
            self.scene = requested.clone();
            if recovered {
                for (_, probe) in world.iter_mut::<dome_render::CaptureProbe>() {
                    probe.forget();
                }
            }
        }
        if self.wanted.as_ref() == Some(&requested) {
            self.wanted = None;
        }
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

    fn take_change(&mut self) -> Option<Change> {
        let change = std::mem::take(&mut self.change);
        change.any().then_some(change)
    }

    fn rewatch(&mut self) {
        self.watch.renew(&self.scene);
    }
}

fn joined(messages: &[String]) -> Option<String> {
    (!messages.is_empty()).then(|| messages.join("; "))
}

fn phase_of(world: &World) -> f32 {
    world.resource::<Time>().map_or(1.0, |time| time.alpha())
}

impl Session {
    fn window(window: &Window, request: &GpuRequest) -> Self {
        let context = Gpu::open_window(request, window.target())
            .unwrap_or_else(|error| panic!("the engine opens its render device: {error}"));
        let (width, height) = window.size();
        let surface = context.surface(width, height);
        let format = surface.format();
        let events = context.events();
        let renderer = Renderer::new(&context, format);
        Self {
            context,
            events,
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
        let context = Gpu::open(request)
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
        let events = context.events();
        let renderer = Renderer::new(&context, format);
        Self {
            context,
            events,
            presentation: Presentation::Offscreen(Box::new(target)),
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
        file: &SceneConfig,
        registry: &Arc<Registry>,
        load: AssetLoad,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        let health = GpuHealth {
            context: &self.context,
            events: &self.events,
        };
        let live = LiveScene::new(
            SceneConfig {
                path: file.path.clone(),
                root: file.root.clone(),
            },
            registry,
            &mut self.renderer,
            &health,
            world,
            load,
            wake,
        )?;
        self.live = Some(live);
        Ok(())
    }

    fn recover_scene(
        &mut self,
        mut live: LiveScene,
        world: &mut World,
        load: AssetLoad,
    ) -> Result<(), String> {
        let health = GpuHealth {
            context: &self.context,
            events: &self.events,
        };
        live.recover(&mut self.renderer, &health, world, load)?;
        self.live = Some(live);
        Ok(())
    }

    fn reload(
        &mut self,
        world: &mut World,
        registry: &Arc<Registry>,
        load: AssetLoad,
    ) -> Option<Reload> {
        let live = self.live.as_mut()?;
        let health = GpuHealth {
            context: &self.context,
            events: &self.events,
        };
        live.update(world, registry, &mut self.renderer, &health, load)
    }

    fn take_change(&mut self) -> Option<Change> {
        self.live.as_mut()?.take_change()
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

    fn recovering(&self) -> bool {
        self.live.as_ref().is_some_and(|live| live.recovering)
    }

    fn resurface(&mut self, window: &Window) -> bool {
        let (width, height) = window.size();
        let surface = self.context.surface(width, height);
        let adopted = surface.format() == self.format;
        self.presentation = Presentation::Window(Box::new(surface));
        adopted
    }

    fn draw_window(
        &mut self,
        world: &mut World,
        clear: [f32; 4],
        width: u32,
        height: u32,
    ) -> Result<(), GpuFailure> {
        let frame = {
            let Presentation::Window(surface) = &mut self.presentation else {
                panic!("a windowed application presents its surface");
            };
            if surface.width() != width || surface.height() != height {
                surface.resize(width, height);
            }
            surface.acquire()?
        };
        let Some(frame) = frame else {
            return Ok(());
        };
        self.width = frame.width();
        self.height = frame.height();
        if frame.format() != self.format {
            frame.discard();
            return Err(GpuFailure::SurfaceFormat);
        }
        self.renderer.set_interpolation(phase_of(world));
        self.renderer
            .render(world, Target::Surface(Box::new(frame)), clear, None);
        Ok(())
    }

    fn draw_headless(
        &mut self,
        world: &mut World,
        clear: [f32; 4],
        screenshot: bool,
    ) -> Option<Vec<u8>> {
        if screenshot {
            let width = self.width;
            let height = self.height;
            self.ensure_shot(width, height);
            let shot = self
                .shot
                .as_ref()
                .expect("a captured frame holds its target");
            self.renderer.set_interpolation(phase_of(world));
            self.renderer
                .render(world, Target::Texture(shot), clear, None);
            let rgba = shot
                .read_rgba(&self.context.queue())
                .unwrap_or_else(|failure| panic!("{failure}"));
            return Some(dome_image::encode(width, height, &rgba));
        }
        if let Presentation::Offscreen(texture) = &self.presentation {
            self.renderer.set_interpolation(phase_of(world));
            self.renderer
                .render(world, Target::Texture(texture.as_ref()), clear, None);
        }
        None
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

    fn fresh_world(fixed_hz: f64) -> World {
        let mut world = World::new();
        world.insert_resource(Time::new(fixed_hz));
        world.insert_resource(Input::default());
        world.insert_resource(ClearColor::default());
        world.insert_resource(dome_physics::Gravity::default());
        world
    }

    pub fn with_config(config: AppConfig) -> Self {
        assert!(
            config.max_fixed_steps > 0,
            "an application steps no fixed system"
        );
        let world = Self::fresh_world(config.fixed_hz);
        let fixed_seconds = 1.0 / config.fixed_hz as f32;
        let mut fixed = Schedule::new();
        fixed.add(move |world: &mut World| dome_animation::advance(world, fixed_seconds));
        fixed.add(move |world: &mut World| dome_physics::step(world, fixed_seconds));
        Self {
            world,
            fixed,
            update: Schedule::new(),
            startup: Vec::new(),
            reloads: Vec::new(),
            config,
            wake: None,
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
        setup: impl Fn(&mut World, &mut Renderer) + 'static,
    ) -> &mut Self {
        self.startup.push(Box::new(setup));
        self
    }

    pub fn add_reload(&mut self, reload: impl Fn(&mut World, Change) + 'static) -> &mut Self {
        self.reloads.push(Box::new(reload));
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
        assert!(
            config.asset_timeout > Duration::ZERO,
            "a headless run waits for its assets"
        );
        let mut session = Session::headless(config.width, config.height, &self.config.gpu);
        self.start(&mut session, true, config.asset_timeout);
        let fixed = self
            .world
            .resource::<Time>()
            .expect("an application keeps its time")
            .fixed_delta();
        let started = Instant::now();
        let mut frame = 0u64;
        let mut screenshot = None;
        loop {
            match session.events.condition() {
                GpuCondition::Healthy => {}
                GpuCondition::Lost => {
                    session = self.restart_headless(&config, session);
                    continue;
                }
                GpuCondition::Fault(message) => panic!("the graphics device fails: {message}"),
            }
            self.reload_scene(&mut session, config.asset_timeout);
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
                match session.events.condition() {
                    GpuCondition::Healthy => break,
                    GpuCondition::Lost => {
                        session = self.restart_headless(&config, session);
                        continue;
                    }
                    GpuCondition::Fault(message) => {
                        panic!("the graphics device fails: {message}")
                    }
                }
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

    fn restart_headless(&mut self, config: &HeadlessConfig, mut previous: Session) -> Session {
        let mut live = previous.live.take();
        if let Some(live) = &mut live {
            live.unload();
        }
        drop(previous);
        let mut session = Session::headless(config.width, config.height, &self.config.gpu);
        if let Some(live) = live {
            session
                .recover_scene(
                    live,
                    &mut self.world,
                    AssetLoad {
                        blocking: true,
                        timeout: config.asset_timeout,
                    },
                )
                .unwrap_or_else(|error| panic!("{error}"));
        }
        session
    }

    fn start(&mut self, session: &mut Session, focused: bool, asset_timeout: Duration) {
        let load = AssetLoad {
            blocking: self.headless,
            timeout: asset_timeout,
        };
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
        let waker = self.wake.clone();
        let wake = move || {
            if let Some(waker) = &waker {
                waker.wake();
            }
        };
        let mut loaded = false;
        if let Some(file) = &self.config.scene {
            match session.load_scene(&mut self.world, file, &self.config.registry, load, wake) {
                Ok(()) => {
                    loaded = true;
                    self.reload = session.live.as_mut().and_then(LiveScene::take_error);
                }
                Err(error) => {
                    eprintln!("{error}");
                    self.reload = Some(error);
                    self.world.insert_resource(Exit);
                }
            }
        }
        if !load.blocking && loaded {
            let camera = self.world.spawn();
            self.world.insert(
                camera,
                Transform::from_translation(Vec3::new(0.0, 1.0, 4.0)),
            );
            self.world
                .insert(camera, Camera::new(std::f32::consts::FRAC_PI_2, 0.1, 100.0));
            self.loading_camera = Some(camera);
        }
        if loaded && !session.scene_ready() {
            self.startup_pending = !self.startup.is_empty();
        } else {
            self.settle_startup(session);
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
        if steps > 0 {
            self.world
                .resource_mut::<Time>()
                .expect("an application keeps its time")
                .enter_fixed();
        }
        for _ in 0..steps {
            snapshot(&mut self.world);
            self.fixed.run(&mut self.world);
            propagate_step(&mut self.world);
            self.world
                .resource_mut::<Time>()
                .expect("an application keeps its time")
                .consume();
        }
        self.world
            .resource_mut::<Time>()
            .expect("an application keeps its time")
            .enter_update();
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
        for setup in &self.startup {
            setup(&mut self.world, &mut session.renderer);
        }
        propagate(&mut self.world);
    }

    fn reload_scene(&mut self, session: &mut Session, asset_timeout: Duration) {
        let reload = session.reload(
            &mut self.world,
            &self.config.registry,
            AssetLoad {
                blocking: self.headless,
                timeout: asset_timeout,
            },
        );
        let Some(reload) = reload else {
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
        if session.scene_ready()
            && !self.startup_pending
            && let Some(change) = session.take_change()
        {
            for observer in &self.reloads {
                observer(&mut self.world, change);
            }
        }
        if let Some(error) = &reload.error {
            eprintln!("{error}");
        }
        self.reload = reload.error;
    }
}

impl Handler for App {
    fn opened(&mut self, window: &Window, waker: Waker) {
        self.wake = Some(waker);
        let mut session = Session::window(window, &self.config.gpu);
        self.start(&mut session, window.has_focus(), Duration::ZERO);
        self.session = Some(session);
    }

    fn frame(&mut self, window: &Window, real: Duration) -> Flow {
        let Some(mut session) = self.session.take() else {
            return Flow::Continuous;
        };
        match session.events.condition() {
            GpuCondition::Healthy => {}
            GpuCondition::Lost => {
                self.recover_window(window, session);
                return Flow::Continuous;
            }
            GpuCondition::Fault(message) => panic!("the graphics device fails: {message}"),
        }
        match self.frame_session(&mut session, window, real) {
            Ok(flow) => {
                self.session = Some(session);
                flow
            }
            Err(GpuFailure::SurfaceOutdated | GpuFailure::SurfaceLost) => {
                if session.resurface(window) {
                    self.session = Some(session);
                } else {
                    self.recover_window(window, session);
                }
                Flow::Continuous
            }
            Err(GpuFailure::SurfaceFormat) => {
                self.recover_window(window, session);
                Flow::Continuous
            }
            Err(GpuFailure::Fault(message)) => panic!("the graphics device fails: {message}"),
        }
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
    fn recover_window(&mut self, window: &Window, mut previous: Session) {
        let mut live = previous.live.take();
        if let Some(live) = &mut live {
            live.unload();
        }
        drop(previous);
        let mut session = Session::window(window, &self.config.gpu);
        if let Some(live) = live {
            session
                .recover_scene(
                    live,
                    &mut self.world,
                    AssetLoad {
                        blocking: false,
                        timeout: Duration::ZERO,
                    },
                )
                .unwrap_or_else(|error| panic!("{error}"));
        }
        self.session = Some(session);
    }

    fn frame_session(
        &mut self,
        session: &mut Session,
        window: &Window,
        real: Duration,
    ) -> Result<Flow, GpuFailure> {
        self.reload_scene(session, Duration::ZERO);
        let (width, height) = window.size();
        if window.is_minimized() || width == 0 || height == 0 {
            return Ok(Flow::Idle);
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
        if session.recovering() {
            return Ok(Flow::Wait(RECOVERY_POLL));
        }
        session.draw_window(&mut self.world, clear, width, height)?;
        dome_render::install_captures(&mut self.world, session.renderer.take_captures());
        Ok(if exit { Flow::Exit } else { Flow::Continuous })
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as Process};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dome_gpu_device::{GpuContext, GpuRequest, Submission, WindowHandle};
use dome_math::{Quat, Rect, Transform, Vec2, Vec3};
use dome_scene::{Delta, Node, Registry, Scene, Shape, Target, Value, Watch};
use dome_ui::InputKey as Key;
use dome_window::{
    Event, Flow, Handler, KeyCode, ModifiersState, MouseButton, Window, WindowConfig,
    run as run_window,
};

use crate::camera::{self, Orbit};
use crate::gizmo::{self, Grip, Mode, Space};
use crate::history::History;
use crate::message::Message;
use crate::panel::Panels;
use crate::preview::Preview;
use crate::session::Session;
use crate::shell::{self, View};

const SCENE_CLEAR: [f32; 4] = [0.055, 0.068, 0.095, 1.0];

pub struct EditorConfig {
    pub window: WindowConfig,
    pub scene: PathBuf,
    pub registry: Arc<Registry>,
    pub debug: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            window: WindowConfig::new("Dome Editor", 1440, 900),
            scene: PathBuf::from("scene.toml"),
            registry: Arc::new(Registry::default()),
            debug: false,
        }
    }
}

pub fn run(config: EditorConfig) {
    let mut config = config;
    config.window.minimum_size.get_or_insert((720, 480));
    let window = config.window.clone();
    run_window(window, Editor::new(config));
}

struct DragState {
    drag: gizmo::Drag,
    node: usize,
    delta: Option<Delta>,
}

struct PropertyState {
    target: Target,
    path: String,
    delta: Option<Delta>,
}

pub struct Editor {
    scene: Scene,
    path: PathBuf,
    selected: Option<Target>,
    dirty: bool,
    status: String,
    error: Option<String>,
    child: Option<Child>,
    orbit: Orbit,
    mode: Mode,
    space: Space,
    debug: bool,
    panels: Panels,
    collapsed: HashSet<String>,
    session: Option<Session>,
    history: History,
    drag: Option<DragState>,
    property: Option<PropertyState>,
    camera_drag: Option<MouseButton>,
    primary: bool,
    viewport: Rect,
    pointer: Vec2,
    import_path: String,
    prefab_path: String,
    watch: Watch,
    modifiers: ModifiersState,
    origin: Instant,
    hover: Option<Grip>,
    viewport_texture: Option<dome_gpu_device::Texture>,
}

impl Editor {
    fn new(config: EditorConfig) -> Self {
        let path = config.scene;
        let registry = config.registry;
        let exists = path.exists();
        let scene = if exists {
            Scene::load_with(&path, registry.clone())
        } else {
            crate::default::scene(&path, registry.clone())
        };
        let selected = scene
            .nodes()
            .iter()
            .find(|node| node.has_component("mesh"))
            .or_else(|| scene.nodes().first())
            .map(|node| Target::Node(node.id.clone()));
        let watch = Watch::new(&path, &scene);
        Self {
            scene,
            path,
            selected,
            dirty: !exists,
            status: if exists {
                String::from("Loaded")
            } else {
                String::from("New scene")
            },
            error: None,
            child: None,
            orbit: Orbit::default(),
            mode: Mode::Translate,
            space: Space::World,
            debug: config.debug,
            panels: Panels::default(),
            collapsed: HashSet::new(),
            session: None,
            history: History::default(),
            drag: None,
            property: None,
            camera_drag: None,
            primary: false,
            viewport: Rect::ZERO,
            pointer: Vec2::ZERO,
            import_path: String::new(),
            prefab_path: String::new(),
            watch,
            modifiers: ModifiersState::default(),
            origin: Instant::now(),
            hover: None,
            viewport_texture: None,
        }
    }

    fn open(&mut self, window: &Window) {
        let request = GpuRequest {
            surface: Some(WindowHandle::of(
                window.window_handle(),
                window.display_handle(),
            )),
            ..Default::default()
        };
        let context = GpuContext::open(&request)
            .unwrap_or_else(|error| panic!("the editor opens its render device: {error}"));
        let (width, height) = window.size();
        assert!(
            width > 0 && height > 0,
            "a window of {width} by {height} presents no surface"
        );
        let surface = context.device().surface(width, height);
        let mut preview = Preview::new(&context, &self.scene);
        preview.set_debug(self.debug);
        self.session = Some(Session::new(context, surface, preview, &self.scene));
        window.set_title(&format!("Dome Editor - {}", self.path.display()));
    }

    fn selected_node(&self) -> Option<usize> {
        match self.selected.as_ref()? {
            Target::Node(id) => self.scene.node_index(id),
            _ => None,
        }
    }

    fn selection_origin(&self) -> Vec3 {
        self.selected_node()
            .map(|index| self.scene.world_transform(index).translation)
            .unwrap_or(Vec3::ZERO)
    }

    fn basis(&self, index: usize) -> [Vec3; 3] {
        let rotation = if self.mode == Mode::Scale || self.space == Space::Local {
            self.scene.world_transform(index).rotation
        } else {
            Quat::IDENTITY
        };
        [Vec3::X, Vec3::Y, Vec3::Z].map(|axis| rotation.rotate_vec3(axis))
    }

    fn target(&self) -> gizmo::Target {
        gizmo::Target {
            camera: camera::editor(),
            view: self.orbit.view(),
            viewport: self.viewport,
        }
    }

    fn save(&mut self) {
        self.scene.save(&self.path);
        self.rewatch();
        self.dirty = false;
        self.status = format!("Saved {}", self.path.display());
    }

    fn reload(&mut self) {
        let registry = self.scene.registry_arc();
        let loaded = catch_unwind(AssertUnwindSafe(|| Scene::load_with(&self.path, registry)));
        match loaded {
            Ok(scene) => {
                self.scene = scene;
                self.drag = None;
                self.property = None;
                self.history.reset();
                self.dirty = false;
                self.status = format!("Reloaded {}", self.path.display());
                self.rewatch();
                self.clamp();
                self.refresh_preview();
            }
            Err(_) => {
                self.error = Some(format!("{} holds no readable scene", self.path.display()));
            }
        }
    }

    fn reload_if_changed(&mut self) {
        let Some(change) = self.watch.changed() else {
            return;
        };
        if change.scene {
            let registry = self.scene.registry_arc();
            if let Ok(scene) =
                catch_unwind(AssertUnwindSafe(|| Scene::load_with(&self.path, registry)))
            {
                self.scene = scene;
                self.drag = None;
                self.property = None;
                self.history.reset();
                self.dirty = false;
                self.status = String::from("Reloaded from disk");
                self.clamp();
                self.refresh_preview();
            }
        } else if change.model || change.asset {
            self.refresh_preview();
        }
        self.rewatch();
    }

    fn rewatch(&mut self) {
        self.watch.renew(&self.scene);
        let errors = self.watch.errors().collect::<Vec<_>>();
        if !errors.is_empty() {
            self.error = Some(errors.join("; "));
        }
    }

    fn clamp(&mut self) {
        let held = self.selected.clone();
        if let Some(target) = held
            && !self.scene.holds(&target)
        {
            self.selected = None;
        }
    }

    fn apply_edit(&mut self, target: &Target) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        match target {
            Target::Node(id) => match self.scene.node_index(id) {
                Some(index) => session.preview.apply(&self.scene, index),
                None => {
                    session.preview.refresh(&self.scene);
                }
            },
            _ => {
                session.preview.refresh(&self.scene);
            }
        }
    }

    fn refresh_preview(&mut self) {
        if let Some(session) = &mut self.session {
            session.preview.refresh(&self.scene);
            session.register_textures(&self.scene);
        }
    }

    fn record(&mut self, delta: Delta) {
        if delta.is_empty() {
            return;
        }
        self.history.record(delta);
        self.dirty = true;
    }

    fn undo(&mut self) {
        self.finish_drag();
        self.finish_property();
        if self.history.undo(&mut self.scene) {
            self.dirty = true;
            self.status = String::from("Undo");
            self.clamp();
            self.refresh_preview();
        }
    }

    fn redo(&mut self) {
        self.finish_drag();
        self.finish_property();
        if self.history.redo(&mut self.scene) {
            self.dirty = true;
            self.status = String::from("Redo");
            self.clamp();
            self.refresh_preview();
        }
    }

    fn finish_drag(&mut self) {
        if let Some(state) = self.drag.take()
            && let Some(delta) = state.delta
        {
            self.record(delta);
        }
    }

    fn finish_property(&mut self) {
        if let Some(state) = self.property.take()
            && let Some(delta) = state.delta
        {
            self.record(delta);
        }
    }

    fn property_edit(&mut self, target: Target, path: String, value: Value) {
        let continued = self
            .property
            .as_ref()
            .is_some_and(|held| held.target == target && held.path == path);
        if !continued {
            self.finish_property();
        }
        if let Some(previous) = self.property.as_mut().and_then(|held| held.delta.take()) {
            previous.revert(&mut self.scene);
        }
        match self
            .scene
            .edit(|edit| edit.set_property(&target, &path, &value))
        {
            Ok((_, delta)) => {
                self.property = Some(PropertyState {
                    target: target.clone(),
                    path,
                    delta: Some(delta),
                });
                self.dirty = true;
                self.apply_edit(&target);
            }
            Err(error) => {
                self.property = None;
                self.error = Some(error);
            }
        }
    }

    fn interrupt(&mut self) {
        self.finish_drag();
        self.finish_property();
        self.camera_drag = None;
        self.primary = false;
        self.hover = None;
    }

    fn play(&mut self) {
        if self.child.is_some() {
            return;
        }
        self.save();
        let scene = self.path.canonicalize().unwrap_or_else(|error| {
            panic!(
                "the scene {} resolves no path: {error}",
                self.path.display()
            )
        });
        let executable = std::env::current_exe().expect("the editor names its own executable");
        let mut process = Process::new(executable);
        process.arg("--play").arg(&scene);
        if self.debug {
            process.arg("--debug");
        }
        let child = process
            .spawn()
            .unwrap_or_else(|error| panic!("the editor starts its player: {error}"));
        self.child = Some(child);
        self.status = String::from("Playing");
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.status = String::from("Stopped");
        }
    }

    fn poll(&mut self) {
        let exited = self
            .child
            .as_mut()
            .is_some_and(|child| child.try_wait().ok().flatten().is_some());
        if exited {
            self.child = None;
            self.status = String::from("Player exited");
        }
    }

    fn add_node(&mut self, kind: &str) {
        if !matches!(kind, "mesh" | "camera" | "sun" | "lamp" | "empty" | "probe") {
            self.error = Some(format!("the editor adds no {kind}"));
            return;
        }
        let parent = match &self.selected {
            Some(Target::Node(id)) if self.scene.node_index(id).is_some() => Some(id.clone()),
            _ => None,
        };
        let inserted = self.scene.edit(|edit| {
            if kind == "mesh" {
                if edit.meshes().is_empty() {
                    let id = unique_asset(edit, "mesh", "cube");
                    edit.add_mesh(&id, Shape::Cube)?;
                }
                if edit.materials().is_empty() {
                    let id = unique_asset(edit, "material", "grey");
                    edit.add_material(&id)?;
                }
            }
            let mut node = match kind {
                "mesh" => crate::default::mesh(&edit.meshes()[0].id, &edit.materials()[0].id),
                "camera" => crate::default::camera(),
                "sun" => crate::default::sun(),
                "lamp" => crate::default::lamp(),
                "probe" => {
                    let id = unique_asset(edit, "environment", "probe");
                    edit.add_capture_environment(&id)?;
                    crate::default::probe(&id)
                }
                _ => Node::new(""),
            };
            node.id = match &parent {
                Some(parent) => edit.child_id(parent, kind),
                None => unique_node(edit, kind),
            };
            match &parent {
                Some(parent) => edit.insert_child(parent, node),
                None => edit.insert_node(node),
            }
        });
        match inserted {
            Ok((target, delta)) => {
                self.record(delta);
                self.selected = Some(target);
                self.status = format!("Added {kind}");
                self.refresh_preview();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn add_asset(&mut self, kind: &str) {
        let import = self.import_path.clone();
        let created = self.scene.edit(|edit| match kind {
            "mesh" => {
                let id = unique_asset(edit, "mesh", "mesh");
                edit.add_mesh(&id, Shape::Cube)
            }
            "material" => {
                let id = unique_asset(edit, "material", "material");
                edit.add_material(&id)
            }
            "graph" => {
                let id = unique_asset(edit, "graph", "graph");
                edit.add_graph(&id)
            }
            "image" => {
                let id = unique_asset(edit, "image", "image");
                edit.add_image(&id, &import)
            }
            "sound" => {
                let id = unique_asset(edit, "sound", "sound");
                edit.add_sound(&id, &import)
            }
            "environment" => {
                let id = unique_asset(edit, "environment", "environment");
                edit.add_environment(&id, &import)
            }
            other => Err(format!("the editor adds no {other} asset")),
        });
        match created {
            Ok((target, delta)) => {
                self.record(delta);
                self.selected = Some(target);
                self.status = format!("Added {kind}");
                self.refresh_preview();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn import_model(&mut self, path: &str) {
        if path.is_empty() {
            self.error = Some(String::from("the editor imports no path"));
            return;
        }
        let id = model_id(&self.scene, path);
        match self.scene.edit(|edit| edit.import_model(&id, path, None)) {
            Ok((target, delta)) => {
                self.record(delta);
                self.selected = Some(target);
                self.status = format!("Imported {path}");
                self.refresh_preview();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn add_instance(&mut self, path: &str) {
        if path.is_empty() {
            self.error = Some(String::from("the editor instantiates no path"));
            return;
        }
        let name = PathBuf::from(path)
            .file_stem()
            .and_then(|name| name.to_str())
            .map(|name| {
                name.chars()
                    .filter(|held| held.is_ascii_alphanumeric() || *held == '_' || *held == '-')
                    .collect::<String>()
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| String::from("prefab"));
        let parent = match &self.selected {
            Some(Target::Node(id)) if self.scene.node_index(id).is_some() => Some(id.clone()),
            _ => None,
        };
        let inserted = self.scene.edit(|edit| {
            let id = match &parent {
                Some(parent) => edit.child_id(parent, &name),
                None => unique_node(edit, &name),
            };
            match &parent {
                Some(parent) => edit.insert_child(parent, Node::instance(&id, path)),
                None => edit.add_instance(&id, path),
            }
        });
        match inserted {
            Ok((target, delta)) => {
                self.record(delta);
                self.selected = Some(target);
                self.status = format!("Instantiated {path}");
                self.refresh_preview();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn remove(&mut self) {
        self.finish_drag();
        self.finish_property();
        let Some(target) = self.selected.clone() else {
            return;
        };
        let parent = match &target {
            Target::Node(id) => self
                .scene
                .node_index(id)
                .and_then(|index| self.scene.parent_index(index))
                .map(|index| Target::Node(self.scene.nodes()[index].id.clone())),
            _ => None,
        };
        match self.scene.edit(|edit| edit.remove(&target)) {
            Ok(((), delta)) => {
                self.record(delta);
                self.selected = parent;
                self.status = format!("Removed {target}");
                self.refresh_preview();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn apply(&mut self, message: Message) {
        self.error = None;
        match message {
            Message::Save => self.save(),
            Message::Reload => self.reload(),
            Message::Undo => self.undo(),
            Message::Redo => self.redo(),
            Message::Play => self.play(),
            Message::Stop => self.stop(),
            Message::Mode(mode) => self.mode = mode,
            Message::ToggleSpace => self.space = self.space.toggled(),
            Message::Focus => {
                if let Some(index) = self.selected_node() {
                    self.orbit
                        .focus(self.scene.world_transform(index).translation);
                    self.status = String::from("Focused");
                }
            }
            Message::Select(target) => {
                if self.scene.holds(&target) {
                    self.selected = Some(target);
                }
            }
            Message::AddNode(kind) => self.add_node(kind),
            Message::AddAsset(kind) => self.add_asset(kind),
            Message::Remove => self.remove(),
            Message::AddComponent(target, name) => {
                match self
                    .scene
                    .edit(|edit| edit.set_property(&target, &name, &Value::Bool(true)))
                {
                    Ok((_, delta)) => {
                        self.record(delta);
                        self.status = format!("Added {name}");
                        self.refresh_preview();
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::RemoveComponent(target, name) => {
                match self
                    .scene
                    .edit(|edit| edit.set_property(&target, &name, &Value::Bool(false)))
                {
                    Ok((_, delta)) => {
                        self.record(delta);
                        self.status = format!("Removed {name}");
                        self.refresh_preview();
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::Property {
                target,
                path,
                value,
            } => self.property_edit(target, path, value),
            Message::ImportPath(path) => self.import_path = path,
            Message::Import => {
                let path = self.import_path.clone();
                self.import_model(&path);
            }
            Message::PrefabPath(path) => self.prefab_path = path,
            Message::Instance => {
                let path = self.prefab_path.clone();
                self.add_instance(&path);
            }
            Message::Pane(pane) => self.panels.toggle(pane),
            Message::Debug(enabled) => {
                self.debug = enabled;
                if let Some(session) = &mut self.session {
                    session.preview.set_debug(enabled);
                }
            }
            Message::Expand(id) => {
                self.collapsed.remove(&id);
            }
            Message::Collapse(id) => {
                self.collapsed.insert(id);
            }
        }
    }

    fn apply_messages(&mut self, messages: impl IntoIterator<Item = Message>) {
        for message in messages {
            self.apply(message);
        }
        if !self.primary {
            self.finish_property();
        }
    }

    fn frame(&mut self, window: &Window, real: Duration) -> Flow {
        self.poll();
        self.reload_if_changed();
        let (width, height) = window.size();
        if window.is_minimized() || width == 0 || height == 0 {
            return Flow::Continue;
        }
        let scale = window.scale_factor() as f32;
        let logical = Vec2::new(width as f32 / scale, height as f32 / scale);
        let time = self.origin.elapsed().as_secs_f64();
        let messages = {
            let Some(session) = self.session.as_mut() else {
                return Flow::Continue;
            };
            session.preview.poll();
            session.register_textures(&self.scene);
            session.preview.advance(real);
            session.ui.update(logical, scale, time)
        };
        self.apply_messages(messages);
        let camera = self.orbit.view();
        let origin = self.selection_origin();
        let index = self.selected_node();
        let basis = index.map_or([Vec3::X, Vec3::Y, Vec3::Z], |index| self.basis(index));
        self.update_hover();
        let view = View {
            scene: &self.scene,
            selected: self.selected.as_ref(),
            path: &self.path,
            status: &self.status,
            dirty: self.dirty,
            playing: self.child.is_some(),
            mode: self.mode,
            space: self.space,
            panels: self.panels,
            debug: self.debug,
            import_path: &self.import_path,
            prefab_path: &self.prefab_path,
            collapsed: &self.collapsed,
            camera,
            origin,
            basis,
            hover: self.hover,
            active: self.drag.as_ref().map(|drag| drag.drag.grip()),
            error: self.error.as_deref(),
        };
        let viewport = {
            let Some(session) = self.session.as_mut() else {
                return Flow::Continue;
            };
            let element = {
                let ctx = session.ui.build();
                shell::shell(&ctx, &view)
            };
            session.ui.present(element);
            session.ui.bounds("viewport").unwrap_or_default()
        };
        self.viewport = viewport;
        if viewport.is_empty() {
            return Flow::Continue;
        }
        let texture_width = (viewport.width() * scale).round().max(1.0) as u32;
        let texture_height = (viewport.height() * scale).round().max(1.0) as u32;
        let stale = self
            .viewport_texture
            .as_ref()
            .is_none_or(|held| held.width() != texture_width || held.height() != texture_height);
        let session = self
            .session
            .as_mut()
            .expect("a drawn editor holds its session");
        if stale {
            self.viewport_texture = Some(dome_gpu_device::Texture::new(
                session.context.device(),
                "dome editor viewport",
                texture_width,
                texture_height,
                1,
                dome_gpu_device::TextureFormat::Rgba8Unorm,
                dome_gpu_device::TextureUsages::RENDER_ATTACHMENT
                    | dome_gpu_device::TextureUsages::SAMPLED,
            ));
        }
        let scene = self
            .viewport_texture
            .as_ref()
            .expect("a drawn editor holds its viewport texture");
        let highlight = self.selected.as_ref().and_then(|target| match target {
            Target::Node(id) => session.preview.entity(id),
            _ => None,
        });
        session.preview.set_camera(camera);
        session.preview.render(scene, SCENE_CLEAR, highlight);
        if session.surface.width() != width || session.surface.height() != height {
            session.surface.resize(width, height);
        }
        let Some(frame) = session.surface.acquire() else {
            return Flow::Continue;
        };
        let Session {
            context,
            preview: _,
            ui,
            ..
        } = session;
        let mut submission = Submission::new(context.device(), "dome editor");
        ui.render(&mut submission, frame.texture(), Some(scene));
        submission.present(frame);
        submission.submit(context.queue());
        Flow::Continue
    }
    fn update_hover(&mut self) {
        let Some(index) = self.selected_node() else {
            self.hover = None;
            return;
        };
        if !self.viewport.contains(self.pointer) || self.drag.is_some() {
            self.hover = None;
            return;
        }
        let target = self.target();
        let handles = target.handles(
            self.scene.world_transform(index).translation,
            self.basis(index),
            self.mode,
        );
        self.hover = gizmo::hit(&handles, self.pointer);
    }

    fn pointer_viewport(&self) -> Option<Vec2> {
        self.viewport.contains(self.pointer).then_some(self.pointer)
    }

    fn gizmo_point(&mut self, point: Vec2) {
        let Some(mut state) = self.drag.take() else {
            return;
        };
        let transform = state.drag.transform(point);
        if let Some(previous) = state.delta.take() {
            previous.revert(&mut self.scene);
        }
        match self
            .scene
            .edit(|edit| edit.set_transform(state.node, transform))
        {
            Ok(((), delta)) => {
                state.delta = Some(delta);
                self.dirty = true;
                if let Some(session) = &mut self.session {
                    session.preview.apply(&self.scene, state.node);
                }
            }
            Err(error) => self.error = Some(error),
        }
        self.drag = Some(state);
    }

    fn press(&mut self, button: MouseButton, scale: f32) {
        let Some(point) = self.pointer_viewport() else {
            return;
        };
        match button {
            MouseButton::Left if self.modifiers.alt_key() => {
                self.camera_drag = Some(MouseButton::Left);
            }
            MouseButton::Left => {
                if let Some(index) = self.selected_node() {
                    let target = self.target();
                    let handles = target.handles(
                        self.scene.world_transform(index).translation,
                        self.basis(index),
                        self.mode,
                    );
                    if let Some(grip) = gizmo::hit(&handles, point) {
                        let handle = handles
                            .iter()
                            .find(|handle| handle.grip == grip)
                            .expect("a hit gizmo holds its handle");
                        let local = self.scene.transform(index);
                        let parent = self
                            .scene
                            .parent_index(index)
                            .map_or(Transform::IDENTITY, |parent| {
                                self.scene.world_transform(parent)
                            });
                        self.drag = Some(DragState {
                            drag: gizmo::begin(target, handle, self.mode, local, parent, point),
                            node: index,
                            delta: None,
                        });
                        return;
                    }
                }
                self.pick(scale);
            }
            MouseButton::Middle => self.camera_drag = Some(MouseButton::Middle),
            MouseButton::Right => self.camera_drag = Some(MouseButton::Right),
            _ => {}
        }
    }

    fn pick(&mut self, scale: f32) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let point = (self.pointer - self.viewport.min) * scale;
        match session.preview.pick(point) {
            Some(id) => self.selected = Some(Target::Node(id)),
            None => self.selected = None,
        }
    }

    fn release(&mut self, button: MouseButton) {
        if self.camera_drag == Some(button) {
            self.camera_drag = None;
        }
        if button == MouseButton::Left && self.drag.is_some() {
            self.finish_drag();
            self.refresh_preview();
        }
    }

    fn moved(&mut self, delta: Vec2) {
        let Some(point) = self.pointer_viewport() else {
            return;
        };
        if let Some(button) = self.camera_drag {
            match button {
                MouseButton::Middle => self.orbit.pan(delta),
                _ => self.orbit.rotate(delta),
            }
            return;
        }
        if self.drag.is_some() {
            self.gizmo_point(point);
        }
    }

    fn wheel(&mut self, delta: f32) {
        if self.pointer_viewport().is_some() {
            self.orbit.zoom(delta);
        }
    }

    fn key(&mut self, key: Key, repeat: bool) {
        if repeat
            || self
                .session
                .as_ref()
                .is_some_and(|session| session.ui.claims_key(key))
        {
            return;
        }
        self.shortcut(key);
    }

    fn shortcut(&mut self, key: Key) {
        let command = self.modifiers.control_key() || self.modifiers.super_key();
        match key {
            Key::Character('s') if command => self.apply(Message::Save),
            Key::Character('z') if command => {
                if self.modifiers.shift_key() {
                    self.apply(Message::Redo);
                } else {
                    self.apply(Message::Undo);
                }
            }
            Key::Character('y') if command => self.apply(Message::Redo),
            Key::Function(5) => {
                if self.child.is_some() {
                    self.apply(Message::Stop);
                } else {
                    self.apply(Message::Play);
                }
            }
            Key::Character('w') => self.apply(Message::Mode(Mode::Translate)),
            Key::Character('e') => self.apply(Message::Mode(Mode::Rotate)),
            Key::Character('r') => self.apply(Message::Mode(Mode::Scale)),
            Key::Character('q') => self.apply(Message::ToggleSpace),
            Key::Character('f') => self.apply(Message::Focus),
            Key::Delete => self.apply(Message::Remove),
            _ => {}
        }
    }

    fn handle(&mut self, event: &Event, scale: f32, size: (u32, u32)) {
        match event {
            Event::Modifiers(state) => self.modifiers = *state,
            Event::Resized { .. } | Event::ScaleFactorChanged { .. } => {
                if let Some(session) = &mut self.session {
                    session.surface.resize(size.0, size.1);
                }
            }
            Event::Key {
                code,
                pressed: true,
                repeat,
                ..
            } => {
                if let Some(key) = shortcut_key(*code) {
                    self.key(key, *repeat);
                }
            }
            Event::Key { pressed: false, .. } => {}
            Event::CursorMoved { x, y } => {
                let point = Vec2::new(*x / scale, *y / scale);
                let delta = point - self.pointer;
                self.pointer = point;
                self.moved(delta);
            }
            Event::Button { button, pressed } => {
                if *button == MouseButton::Left {
                    self.primary = *pressed;
                }
                if *pressed {
                    self.press(*button, scale);
                } else {
                    self.release(*button);
                }
            }
            Event::Wheel { lines, pixels } => {
                let delta = if *pixels != [0.0; 2] {
                    -pixels[1] / scale
                } else {
                    -lines[1] * 40.0
                };
                self.wheel(delta);
            }
            Event::Focused(false) | Event::CursorLeft => self.interrupt(),
            Event::Focused(true) => {}
        }
    }
}

fn shortcut_key(code: KeyCode) -> Option<Key> {
    Some(match code {
        KeyCode::KeyQ => Key::Character('q'),
        KeyCode::KeyW => Key::Character('w'),
        KeyCode::KeyE => Key::Character('e'),
        KeyCode::KeyR => Key::Character('r'),
        KeyCode::KeyS => Key::Character('s'),
        KeyCode::KeyF => Key::Character('f'),
        KeyCode::KeyY => Key::Character('y'),
        KeyCode::KeyZ => Key::Character('z'),
        KeyCode::Delete => Key::Delete,
        KeyCode::F5 => Key::Function(5),
        _ => return None,
    })
}

impl Handler for Editor {
    fn opened(&mut self, window: &Window) {
        self.open(window);
    }

    fn frame(&mut self, window: &Window, real: Duration) -> Flow {
        self.frame(window, real)
    }

    fn event(&mut self, window: &Window, event: Event) {
        let scale = window.scale_factor() as f32;
        self.handle(&event, scale, window.size());
        if let Some(session) = &mut self.session {
            session.ui.input_mut().window(&event, scale);
        }
    }

    fn suspended(&mut self, _window: &Window) -> Flow {
        Flow::Exit
    }

    fn closed(&mut self, _window: &Window) {
        self.stop();
        self.session = None;
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn unique_node(scene: &Scene, name: &str) -> String {
    let mut index = 0usize;
    loop {
        let candidate = if index == 0 {
            name.to_owned()
        } else {
            format!("{name}_{index}")
        };
        if scene.node_index(&candidate).is_none() {
            return candidate;
        }
        index += 1;
    }
}

fn model_id(scene: &Scene, path: &str) -> String {
    let name = Path::new(path)
        .file_stem()
        .and_then(|name| name.to_str())
        .map(|name| {
            name.chars()
                .map(|piece| {
                    if piece.is_ascii_alphanumeric() || piece == '_' || piece == '-' {
                        piece
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| String::from("model"));
    unique_asset(scene, "model", &name)
}

fn unique_asset(scene: &Scene, kind: &str, name: &str) -> String {
    let mut index = 0usize;
    loop {
        let candidate = if index == 0 {
            name.to_owned()
        } else {
            format!("{name}_{index}")
        };
        let held = match kind {
            "mesh" => scene.mesh_index(&candidate).is_some(),
            "material" => scene.material_index(&candidate).is_some(),
            "image" => scene.image_index(&candidate).is_some(),
            "model" => scene.model_index(&candidate).is_some(),
            "sound" => scene.sound_index(&candidate).is_some(),
            "environment" => scene.environment_index(&candidate).is_some(),
            "graph" => scene.graph_index(&candidate).is_some(),
            other => panic!("the editor names no asset {other}"),
        };
        if !held {
            return candidate;
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: (u32, u32) = (800, 600);

    fn editor() -> Editor {
        Editor::new(EditorConfig {
            scene: PathBuf::from("dome-editor-test-scene-that-does-not-exist.toml"),
            ..EditorConfig::default()
        })
    }

    fn key(code: KeyCode, repeat: bool) -> Event {
        Event::Key {
            code,
            character: None,
            pressed: true,
            repeat,
            text: None,
        }
    }

    fn button(pressed: bool) -> Event {
        Event::Button {
            button: MouseButton::Left,
            pressed,
        }
    }

    fn cursor(position: Vec2, scale: f32) -> Event {
        Event::CursorMoved {
            x: position.x * scale,
            y: position.y * scale,
        }
    }

    #[test]
    fn a_repeated_key_press_removes_one_node() {
        let mut editor = editor();
        let parent = editor
            .scene
            .nodes()
            .first()
            .map(|node| node.id.clone())
            .expect("the editor builds a scene");
        let child = editor.scene.child_id(&parent, "child");
        editor
            .scene
            .edit(|edit| edit.insert_child(&parent, Node::new(child.clone())))
            .expect("the scene gains a child");
        editor.selected = Some(Target::Node(child.clone()));
        let held = editor.scene.nodes().len();

        editor.handle(&key(KeyCode::Delete, false), 1.0, SIZE);
        assert!(
            editor.scene.node_index(&child).is_none(),
            "a delete removes the selected node once",
        );
        assert_eq!(editor.selected, Some(Target::Node(parent.clone())));

        editor.handle(&key(KeyCode::Delete, true), 1.0, SIZE);
        editor.handle(&key(KeyCode::Delete, true), 1.0, SIZE);
        assert_eq!(
            editor.scene.nodes().len(),
            held - 1,
            "a held key removes no second node",
        );
        assert!(editor.scene.node_index(&parent).is_some());
        assert_eq!(editor.selected, Some(Target::Node(parent)));
    }

    #[test]
    fn a_gizmo_drag_follows_the_logical_pointer_and_ends_when_focus_leaves() {
        let mut editor = editor();
        editor.viewport = Rect::from_min_size(Vec2::ZERO, Vec2::new(800.0, 600.0));
        let index = editor.selected_node().expect("the editor starts on a node");
        let before = editor.scene.world_transform(index).translation;
        let target = editor.target();
        let handles = target.handles(before, editor.basis(index), editor.mode);
        let grip = Grip::Axis(gizmo::Axis::X);
        let grab = (0..600u32)
            .flat_map(|y| (0..800u32).map(move |x| Vec2::new(x as f32 + 0.5, y as f32 + 0.5)))
            .find(|point| gizmo::hit(&handles, *point) == Some(grip))
            .expect("the translate gizmo draws a grabbable x axis");
        let handle = handles
            .iter()
            .find(|handle| handle.grip == grip)
            .expect("the gizmo holds the handle it draws");
        let local = editor.scene.transform(index);
        let parent = editor
            .scene
            .parent_index(index)
            .map_or(Transform::IDENTITY, |at| editor.scene.world_transform(at));
        let reference = gizmo::begin(target, handle, editor.mode, local, parent, grab);
        let step = Vec2::new(36.0, -18.0);
        let expected = reference.transform(grab + step).translation;

        editor.handle(&cursor(grab, 2.0), 2.0, SIZE);
        editor.handle(&button(true), 2.0, SIZE);
        assert!(
            editor.drag.is_some(),
            "a hi-dpi press grabs the handle under the pointer",
        );
        editor.handle(&cursor(grab + step, 2.0), 2.0, SIZE);
        let dragged = editor.scene.world_transform(index).translation;
        assert!(
            (dragged - expected).length() < 1.0e-4,
            "the drag travels the logical pointer distance",
        );

        editor.handle(&Event::Focused(false), 2.0, SIZE);
        assert!(editor.drag.is_none(), "a lost focus ends the drag");
        assert!(editor.camera_drag.is_none());
        editor.handle(&cursor(grab + step + step, 2.0), 2.0, SIZE);
        assert_eq!(
            editor.scene.world_transform(index).translation,
            dragged,
            "a drag without a held button moves nothing",
        );

        editor.undo();
        assert_eq!(
            editor.scene.world_transform(index).translation,
            before,
            "the interrupted drag keeps its delta in the history",
        );
        editor.undo();
        assert_eq!(
            editor.scene.world_transform(index).translation,
            before,
            "the interrupted drag records a single step",
        );
    }

    #[test]
    fn a_property_drag_records_one_history_step() {
        let mut editor = editor();
        let target = editor.selected.clone().expect("the editor starts selected");
        let index = editor.selected_node().expect("the editor starts on a node");
        let before = editor.scene.transform(index).translation;

        editor.handle(&button(true), 1.0, SIZE);
        editor.apply_messages((1..=4).map(|step| Message::Property {
            target: target.clone(),
            path: String::from("transform.translation.x"),
            value: Value::Number(step as f32),
        }));
        editor.handle(&button(false), 1.0, SIZE);
        editor.apply_messages(Vec::new());
        assert_eq!(editor.scene.transform(index).translation.x, 4.0);

        editor.undo();
        assert_eq!(
            editor.scene.transform(index).translation,
            before,
            "a property drag lands in the history as one step",
        );
        editor.undo();
        assert_eq!(editor.scene.transform(index).translation, before);
    }

    #[test]
    fn the_library_keeps_the_paths_its_actions_run() {
        let mut editor = editor();
        editor.apply(Message::ImportPath(String::from("models/missing.glb")));
        assert_eq!(editor.import_path, "models/missing.glb");
        editor.apply(Message::Import);
        assert_eq!(
            editor.import_path, "models/missing.glb",
            "an import keeps the path it runs",
        );
        assert!(editor.error.is_some());

        editor.apply(Message::PrefabPath(String::from("prefabs/missing.toml")));
        assert_eq!(editor.prefab_path, "prefabs/missing.toml");
        editor.apply(Message::Instance);
        assert_eq!(
            editor.prefab_path, "prefabs/missing.toml",
            "an instance keeps the path it runs",
        );
        assert!(editor.error.is_some());
    }
}

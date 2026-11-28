use crate::player::Player;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dome_animation::{Comparison, Parameter};
use dome_asset::{Import, Watch};
use dome_ecs::Entity;
use dome_gpu::{Gpu, GpuCondition, GpuFailure, GpuRequest, Submission};
use dome_math::{Quat, Rect, Transform, Vec2, Vec3};
use dome_project::Project;
use dome_render::Pick;
use dome_scene::{Atmosphere, Delta, Node, ReferenceKind, Scene, Shape, Target, Value};
use dome_schema::Registry;
use dome_ui::{InputKey as Key, Ui};
use dome_window::{
    Event, Flow, Handler, KeyCode, ModifiersState, MouseButton, Waker, Window, WindowConfig,
    run as run_window,
};

use crate::action::{
    Action, AssetKind, Confirm, NodeKind, ParameterKind, PathDialog, PathKind, Sheet, Tab,
};
use crate::camera::{self, Orbit};
use crate::document::Document;
use crate::gizmo::{self, Grip, Mode, Space};
use crate::history::History;
use crate::message::Message;
use crate::preview::Preview;
use crate::session::Session;
use crate::stage::{self, State};

const SCENE_CLEAR: [f32; 4] = [0.035, 0.043, 0.063, 1.0];
const PLAYER_INTERVAL: Duration = Duration::from_millis(100);

pub struct EditorConfig {
    pub window: WindowConfig,
    pub project: Project,
    pub registry: Arc<Registry>,
    pub debug: bool,
}

impl EditorConfig {
    pub fn new(project: Project) -> Self {
        Self {
            window: WindowConfig::new("Dome", 1440, 900),
            project,
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

#[derive(Clone, Copy, PartialEq)]
struct Viewport {
    camera: Transform,
    highlight: Option<Entity>,
    size: (u32, u32),
    debug: bool,
    scene: u64,
    assets: u64,
    pose: u64,
}

pub struct Editor {
    scene: Scene,
    project: Project,
    registry: Arc<Registry>,
    path: PathBuf,
    document: Document,
    selected: Option<Target>,
    confirm: Option<Confirm>,
    exiting: bool,
    titled_dirty: Option<bool>,
    message: Option<Message>,
    child: Option<Player>,
    orbit: Orbit,
    mode: Mode,
    space: Space,
    debug: bool,
    outliner_open: bool,
    inspector_open: bool,
    tab: Tab,
    collapsed_branches: HashSet<String>,
    collapsed_sections: HashSet<String>,
    dialog: Option<PathDialog>,
    recent_paths: [String; 5],
    session: Option<Session>,
    history: History,
    drag: Option<DragState>,
    property: Option<PropertyState>,
    camera_drag: Option<MouseButton>,
    primary: bool,
    viewport: Rect,
    pointer: Vec2,
    watch: Option<Watch>,
    modifiers: ModifiersState,
    origin: Instant,
    hover: Option<Grip>,
    viewport_texture: Option<dome_gpu::Texture>,
    viewport_rendered: Option<Viewport>,
    textures_at: Option<u64>,
    ime: bool,
    redraw: bool,
    ui_at: Option<Instant>,
    player_at: Instant,
}

impl Editor {
    fn new(config: EditorConfig) -> Self {
        let path = config.project.entry();
        let root = config.project.root().to_path_buf();
        let registry = config.registry;
        let (scene, document, message) = match Scene::load(&path, root.clone(), registry.clone()) {
            Ok(scene) => {
                let document = Document::open(path.clone(), &scene);
                (scene, document, None)
            }
            Err(error) => {
                let scene = Scene::new(root, registry.clone());
                let document = Document::rejected(path.clone(), &scene);
                (scene, document, Some(Message::error(error)))
            }
        };
        let selected = scene
            .nodes()
            .iter()
            .find(|node| node.has_component("mesh"))
            .or_else(|| scene.nodes().first())
            .map(|node| Target::Node(node.id.clone()));
        Self {
            scene,
            project: config.project,
            registry,
            document,
            path,
            selected,
            confirm: None,
            exiting: false,
            titled_dirty: None,
            message,
            child: None,
            orbit: Orbit::default(),
            mode: Mode::Translate,
            space: Space::World,
            debug: config.debug,
            outliner_open: true,
            inspector_open: true,
            tab: Tab::Scene,
            collapsed_branches: HashSet::new(),
            collapsed_sections: HashSet::new(),
            dialog: None,
            recent_paths: Default::default(),
            session: None,
            history: History::default(),
            drag: None,
            property: None,
            camera_drag: None,
            primary: false,
            viewport: Rect::ZERO,
            pointer: Vec2::ZERO,
            watch: None,
            modifiers: ModifiersState::default(),
            origin: Instant::now(),
            hover: None,
            viewport_texture: None,
            viewport_rendered: None,
            textures_at: None,
            ime: false,
            redraw: true,
            ui_at: None,
            player_at: Instant::now(),
        }
    }

    fn open(&mut self, window: &Window) {
        self.open_with(window, None);
    }

    fn open_with(&mut self, window: &Window, held: Option<Ui<Action>>) {
        let context = Gpu::open_window(&GpuRequest::default(), window.target())
            .unwrap_or_else(|error| panic!("the editor opens its render device: {error}"));
        let (width, height) = window.size();
        let surface = context.surface(width, height);
        let mut preview = Preview::new(&context, &self.scene, Arc::clone(&self.registry));
        preview.set_debug(self.debug);
        self.session = Some(Session::new(context, surface, preview, &self.scene, held));
        self.ime = false;
        self.sync_title(window);
    }

    fn rebuild(&mut self, window: &Window) {
        self.settle_editing();
        self.viewport_texture = None;
        self.viewport_rendered = None;
        self.textures_at = None;
        let held = self.session.take().map(|session| session.ui);
        self.open_with(window, held);
        self.redraw = true;
    }

    fn resurface(&mut self, window: &Window) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.resurface(window, &self.scene) {
            self.textures_at = None;
        }
        self.redraw = true;
    }

    fn settle_editing(&mut self) {
        self.finish_drag();
        self.commit();
        self.finish_property();
    }

    fn fail(&mut self, text: impl Into<String>) {
        self.message = Some(Message::error(text));
    }

    fn notice(&mut self, text: impl Into<String>) {
        self.message = Some(Message::notice(text));
    }

    fn sync_title(&mut self, window: &Window) {
        let dirty = self.document.dirty(&self.scene);
        if self.titled_dirty == Some(dirty) {
            return;
        }
        self.titled_dirty = Some(dirty);
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("main.scene.toml");
        if dirty {
            window.set_title(&format!("Dome — {} — {name} •", self.project.name()));
        } else {
            window.set_title(&format!("Dome — {} — {name}", self.project.name()));
        }
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

    fn save(&mut self) -> bool {
        if self.document.conflicted() {
            self.fail(format!(
                "{} changed on disk: reload it or save over it",
                self.path.display(),
            ));
            return false;
        }
        self.write(false)
    }

    fn overwrite(&mut self) -> bool {
        self.write(true)
    }

    fn write(&mut self, over: bool) -> bool {
        self.settle_editing();
        let written = if over {
            self.document.write_over(&self.scene)
        } else {
            self.document.write(&self.scene)
        };
        if let Err(error) = written {
            self.fail(error);
            return false;
        }
        if let Some(watch) = &mut self.watch {
            watch.adopt(&self.path);
        }
        self.rewatch();
        true
    }

    fn commit(&mut self) {
        let Some(session) = &mut self.session else {
            return;
        };
        let actions = session.ui.commit();
        self.apply_actions(actions);
    }

    fn reread(&mut self) {
        self.settle_editing();
        let registry = self.scene.schema_arc();
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .map_or_else(|| self.path.display().to_string(), String::from);
        match Scene::load(&self.path, self.project.root(), registry) {
            Ok(scene) => {
                let held = self.history.can_undo();
                self.scene = scene;
                self.viewport_rendered = None;
                self.drag = None;
                self.property = None;
                self.history.reset();
                self.document.adopted(&self.scene);
                self.rewatch();
                self.clamp();
                self.refresh_preview();
                self.notice(match held {
                    true => format!("{name} reloads from disk: its undo history starts over"),
                    false => format!("{name} reloads from disk"),
                });
            }
            Err(error) => self.fail(error),
        }
    }

    fn reload_if_changed(&mut self) -> bool {
        let Some(report) = self.watch.as_mut().and_then(Watch::changed) else {
            return false;
        };
        if let Some(errors) = report.errors
            && !errors.is_empty()
        {
            self.fail(errors.join("; "));
        }
        let change = report.change;
        if change.scene || change.prefab {
            if self.document.dirty(&self.scene) {
                if change.scene {
                    self.document.mark_foreign();
                    self.fail(format!(
                        "{} changed on disk: reload it or save over it",
                        self.path.display(),
                    ));
                } else {
                    self.notice("a prefab changed on disk: reload the scene to pick it up");
                }
            } else {
                self.reread();
            }
        } else if change.model || change.asset {
            self.refresh_preview();
        }
        self.rewatch();
        true
    }

    fn rewatch(&mut self) {
        if let Some(watch) = &mut self.watch {
            watch.renew(&self.scene);
        }
    }

    fn request_reload(&mut self) {
        if self.document.dirty(&self.scene) {
            self.confirm = Some(Confirm::Reload);
            return;
        }
        self.reread();
    }

    fn resolve_confirm(&mut self) {
        match self.confirm {
            Some(Confirm::Close) => {
                let written = if self.document.conflicted() {
                    self.overwrite()
                } else {
                    self.save()
                };
                if written {
                    self.confirm = None;
                    self.exiting = true;
                }
            }
            Some(Confirm::Reload) => {
                self.confirm = None;
                self.reread();
            }
            Some(Confirm::Overwrite) => {
                if !self.overwrite() {
                    return;
                }
                self.confirm = None;
            }
            None => {}
        }
    }

    fn discard_confirm(&mut self) {
        if self.confirm == Some(Confirm::Close) {
            self.confirm = None;
            self.exiting = true;
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
            self.textures_at = Some(session.preview.revision());
            session.register_textures(&self.scene);
        }
    }

    fn record(&mut self, delta: Delta) {
        if delta.is_empty() {
            return;
        }
        self.history.record(delta, &self.scene);
    }

    fn undo(&mut self) {
        self.finish_drag();
        self.finish_property();
        if self.history.undo(&mut self.scene) {
            self.clamp();
            self.refresh_preview();
        }
    }

    fn redo(&mut self) {
        self.finish_drag();
        self.finish_property();
        if self.history.redo(&mut self.scene) {
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
        let previous = self.property.as_mut().and_then(|held| held.delta.take());
        if let Some(previous) = &previous {
            previous.revert(&mut self.scene);
        }
        let outcome = self
            .scene
            .edit(|edit| edit.set_property(&target, &path, &value));
        match outcome {
            Ok((_, delta)) => {
                self.apply_edit(&target);
                self.property = Some(PropertyState {
                    target,
                    path,
                    delta: Some(delta),
                });
            }
            Err(error) => {
                match previous {
                    Some(previous) => {
                        previous.apply(&mut self.scene);
                        self.property = Some(PropertyState {
                            target,
                            path,
                            delta: Some(previous),
                        });
                    }
                    None => self.property = None,
                }
                self.fail(error);
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
        if self.child.is_some() || !self.save() {
            return;
        }
        self.child = Some(Player::start(self.project.root(), self.debug));
        self.player_at = Instant::now() + PLAYER_INTERVAL;
    }

    fn stop(&mut self) {
        self.child = None;
    }

    fn poll(&mut self) -> bool {
        let Some(exit) = self.child.as_mut().and_then(Player::exited) else {
            return false;
        };
        self.child = None;
        if exit.failed() {
            let code = exit
                .code
                .map_or_else(|| String::from("no exit code"), |code| code.to_string());
            self.fail(match exit.tail.is_empty() {
                true => format!("the player exited with {code}"),
                false => format!("the player exited with {code}: {}", exit.tail),
            });
        }
        true
    }

    fn parent_for_insert(&self) -> Option<String> {
        let Target::Node(id) = self.selected.as_ref()? else {
            return None;
        };
        let index = self.scene.node_index(id)?;
        self.scene.structurally_editable(index).then(|| id.clone())
    }

    fn create_node(&mut self, kind: NodeKind) {
        let parent = self.parent_for_insert();
        let base = kind.base_id();
        let inserted = self.scene.edit(|edit| {
            if kind == NodeKind::Mesh {
                if edit.meshes().is_empty() {
                    let id = unique_asset(edit, ReferenceKind::Mesh, "cube");
                    edit.add_mesh(&id, Shape::Cube)?;
                }
                if edit.materials().is_empty() {
                    let id = unique_asset(edit, ReferenceKind::Material, "grey");
                    edit.add_material(&id)?;
                }
            }
            let mut node = match kind {
                NodeKind::Mesh => {
                    crate::default::mesh(&edit.meshes()[0].id, &edit.materials()[0].id)
                }
                NodeKind::Camera => crate::default::camera(),
                NodeKind::Sun => crate::default::sun(),
                NodeKind::PointLight => crate::default::lamp(),
                NodeKind::Probe => {
                    let id = unique_asset(edit, ReferenceKind::Environment, "probe");
                    edit.add_capture_environment(&id)?;
                    crate::default::probe(&id)
                }
                NodeKind::Empty => Node::new(""),
            };
            node.id = match &parent {
                Some(parent) => edit.child_id(parent, base),
                None => unique_node(edit, base),
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
                self.tab = Tab::Scene;
                self.refresh_preview();
            }
            Err(error) => self.fail(error),
        }
    }

    fn create_asset(&mut self, kind: AssetKind) {
        let created = self.scene.edit(|edit| match kind {
            AssetKind::Mesh => {
                let id = unique_asset(edit, ReferenceKind::Mesh, "mesh");
                edit.add_mesh(&id, Shape::Cube)
            }
            AssetKind::Material => {
                let id = unique_asset(edit, ReferenceKind::Material, "material");
                edit.add_material(&id)
            }
            AssetKind::Atmosphere => {
                let id = unique_asset(edit, ReferenceKind::Environment, "sky");
                edit.add_atmosphere(&id, Atmosphere::default())
            }
            AssetKind::Graph => {
                let id = unique_asset(edit, ReferenceKind::Graph, "graph");
                edit.add_graph(&id)
            }
        });
        match created {
            Ok((target, delta)) => {
                self.record(delta);
                self.selected = Some(target);
                self.tab = Tab::Asset;
                self.refresh_preview();
            }
            Err(error) => self.fail(error),
        }
    }

    fn open_dialog(&mut self, kind: PathKind) {
        let slot = path_slot(kind);
        self.dialog = Some(PathDialog {
            kind,
            input: self.recent_paths[slot].clone(),
            focus: true,
            invalid: false,
        });
    }

    fn submit_dialog(&mut self, override_input: Option<String>) {
        let Some(mut dialog) = self.dialog.clone() else {
            return;
        };
        if let Some(input) = override_input {
            dialog.input = input;
        }
        let trimmed = dialog.input.trim().to_string();
        let slot = path_slot(dialog.kind);
        self.recent_paths[slot] = trimmed.clone();
        if trimmed.is_empty() {
            if let Some(held) = self.dialog.as_mut() {
                held.focus = true;
                held.invalid = true;
            }
            return;
        }
        let path = format!("{}/{}", dialog.kind.folder().name(), trimmed);
        let parent = self.parent_for_insert();
        let outcome = match dialog.kind {
            PathKind::Model => {
                let id = stem_asset(&self.scene, ReferenceKind::Model, &path, "model");
                self.scene
                    .edit(|edit| edit.import_model(&id, &path, parent.as_deref()))
                    .map(|(target, delta)| (target, delta, Tab::Scene))
            }
            PathKind::Prefab => {
                let stem = sanitized_stem(&path, "prefab");
                self.scene
                    .edit(|edit| {
                        let id = match &parent {
                            Some(parent) => edit.child_id(parent, &stem),
                            None => unique_node(edit, &stem),
                        };
                        match &parent {
                            Some(parent) => edit.insert_child(parent, Node::instance(&id, &path)),
                            None => edit.add_instance(&id, &path),
                        }
                    })
                    .map(|(target, delta)| (target, delta, Tab::Scene))
            }
            PathKind::Image => {
                let id = stem_asset(&self.scene, ReferenceKind::Image, &path, "image");
                self.scene
                    .edit(|edit| edit.add_image(&id, &path))
                    .map(|(target, delta)| (target, delta, Tab::Asset))
            }
            PathKind::Sound => {
                let id = stem_asset(&self.scene, ReferenceKind::Sound, &path, "sound");
                self.scene
                    .edit(|edit| edit.add_sound(&id, &path))
                    .map(|(target, delta)| (target, delta, Tab::Asset))
            }
            PathKind::Panorama => {
                let id = stem_asset(
                    &self.scene,
                    ReferenceKind::Environment,
                    &path,
                    "environment",
                );
                self.scene
                    .edit(|edit| edit.add_environment(&id, &path))
                    .map(|(target, delta)| (target, delta, Tab::Asset))
            }
        };
        match outcome {
            Ok((target, delta, tab)) => {
                self.dialog = None;
                self.record(delta);
                self.selected = Some(target);
                self.tab = tab;
                self.refresh_preview();
            }
            Err(error) => {
                if let Some(held) = self.dialog.as_mut() {
                    held.input = trimmed;
                    held.focus = false;
                    held.invalid = true;
                }
                self.fail(error);
            }
        }
    }

    fn remove_target(&mut self, target: Target) {
        self.finish_drag();
        self.finish_property();
        let fallback = if self.selected.as_ref() == Some(&target) {
            match &target {
                Target::Node(id) => self
                    .scene
                    .node_index(id)
                    .and_then(|index| self.scene.parent_index(index))
                    .map(|index| Target::Node(self.scene.nodes()[index].id.clone())),
                _ => None,
            }
        } else {
            self.selected.clone()
        };
        match self.scene.edit(|edit| edit.remove(&target)) {
            Ok(((), delta)) => {
                self.record(delta);
                self.selected = fallback.filter(|held| self.scene.holds(held));
                self.refresh_preview();
            }
            Err(error) => self.fail(error),
        }
    }

    fn toggle_hidden(&mut self, id: &str) {
        self.finish_property();
        let Some(index) = self.scene.node_index(id) else {
            return;
        };
        let next = !self.scene.nodes()[index].has_component("hidden");
        let target = Target::Node(String::from(id));
        match self
            .scene
            .edit(|edit| edit.set_property(&target, "hidden", &Value::Bool(next)))
        {
            Ok((_, delta)) => {
                self.record(delta);
                self.apply_edit(&target);
            }
            Err(error) => self.fail(error),
        }
    }

    fn unpack_prefab(&mut self, id: &str) {
        self.finish_drag();
        self.finish_property();
        match self.scene.edit(|edit| edit.unpack(id)) {
            Ok(((), delta)) => {
                self.record(delta);
                self.refresh_preview();
            }
            Err(error) => self.fail(error),
        }
    }

    fn add_graph_state(&mut self, graph_id: &str) {
        let Some(clip) = self.scene.animations().first().map(|a| a.id.clone()) else {
            return;
        };
        let Some(graph) = self.scene.graph(graph_id) else {
            return;
        };
        let mut index = graph.asset.states().len();
        let state_id = loop {
            let candidate = format!("state_{index}");
            if graph.asset.state(&candidate).is_none() {
                break candidate;
            }
            index += 1;
        };
        match self
            .scene
            .edit(|edit| edit.add_graph_state(graph_id, &state_id, &clip))
        {
            Ok(((), delta)) => {
                self.record(delta);
                self.refresh_preview();
            }
            Err(error) => self.fail(error),
        }
    }

    fn add_graph_parameter(&mut self, graph_id: &str, kind: ParameterKind) {
        let Some(graph) = self.scene.graph(graph_id) else {
            return;
        };
        let mut index = graph.asset.parameters().len();
        let name = loop {
            let candidate = format!("param_{index}");
            if graph.asset.parameter(&candidate).is_none() {
                break candidate;
            }
            index += 1;
        };
        let default = match kind {
            ParameterKind::Number => Parameter::Number(0.0),
            ParameterKind::Boolean => Parameter::Boolean(false),
        };
        match self
            .scene
            .edit(|edit| edit.add_graph_parameter(graph_id, &name, default))
        {
            Ok(((), delta)) => {
                self.record(delta);
                self.refresh_preview();
            }
            Err(error) => self.fail(error),
        }
    }

    fn add_graph_transition(&mut self, graph_id: &str) {
        let Some(graph) = self.scene.graph(graph_id) else {
            return;
        };
        let states = graph.asset.states();
        if states.len() < 2 {
            return;
        }
        let from = states[0].id().to_owned();
        let to = states[1].id().to_owned();
        match self
            .scene
            .edit(|edit| edit.add_graph_transition(graph_id, &from, &to, 0.2, 1.0))
        {
            Ok(((), delta)) => {
                self.record(delta);
                self.refresh_preview();
            }
            Err(error) => self.fail(error),
        }
    }

    fn add_graph_condition(&mut self, graph_id: &str, transition: usize) {
        let Some(graph) = self.scene.graph(graph_id) else {
            return;
        };
        let Some(param) = graph.asset.parameters().first() else {
            return;
        };
        let name = param.name().to_owned();
        let (comparison, value) = match param.default() {
            Parameter::Number(_) => (Comparison::Above, Parameter::Number(0.0)),
            Parameter::Boolean(_) => (Comparison::Equal, Parameter::Boolean(true)),
        };
        match self
            .scene
            .edit(|edit| edit.add_graph_condition(graph_id, transition, &name, comparison, value))
        {
            Ok(((), delta)) => {
                self.record(delta);
                self.refresh_preview();
            }
            Err(error) => self.fail(error),
        }
    }

    fn apply(&mut self, action: Action) {
        if !matches!(action, Action::EditDialog(_) | Action::DismissMessage) {
            self.message = None;
        }
        match action {
            Action::Save => {
                self.save();
            }
            Action::Overwrite => {
                self.confirm = Some(Confirm::Overwrite);
            }
            Action::Reload => self.request_reload(),
            Action::ResolveConfirm => self.resolve_confirm(),
            Action::DiscardConfirm => self.discard_confirm(),
            Action::DismissConfirm => self.confirm = None,
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Play => self.play(),
            Action::Stop => self.stop(),
            Action::Mode(mode) => self.mode = mode,
            Action::ToggleSpace => self.space = self.space.toggled(),
            Action::Focus => {
                if let Some(index) = self.selected_node() {
                    self.orbit
                        .focus(self.scene.world_transform(index).translation);
                }
            }
            Action::ToggleDebug => {
                self.debug = !self.debug;
                if let Some(session) = &mut self.session {
                    session.preview.set_debug(self.debug);
                }
            }
            Action::ToggleSheet(sheet) => match sheet {
                Sheet::Outliner => self.outliner_open = !self.outliner_open,
                Sheet::Inspector => self.inspector_open = !self.inspector_open,
            },
            Action::SelectTab(tab) => self.tab = tab,
            Action::Select(target) => {
                if self.scene.holds(&target) {
                    self.selected = Some(target);
                }
            }
            Action::CreateNode(kind) => self.create_node(kind),
            Action::CreateAsset(kind) => self.create_asset(kind),
            Action::OpenDialog(kind) => self.open_dialog(kind),
            Action::EditDialog(text) => {
                if let Some(dialog) = self.dialog.as_mut() {
                    dialog.input = text;
                    dialog.focus = false;
                    dialog.invalid = false;
                }
            }
            Action::SubmitPath(text) => self.submit_dialog(Some(text)),
            Action::SubmitDialog => self.submit_dialog(None),
            Action::CloseDialog => self.dialog = None,
            Action::DismissMessage => self.message = None,
            Action::RemoveTarget(target) => self.remove_target(target),
            Action::RemoveSelected => {
                if let Some(target) = self.selected.clone() {
                    self.remove_target(target);
                }
            }
            Action::ToggleHidden(id) => self.toggle_hidden(&id),
            Action::UnpackPrefab(id) => self.unpack_prefab(&id),
            Action::AddComponent(target, name) => {
                match self
                    .scene
                    .edit(|edit| edit.set_property(&target, name, &Value::Bool(true)))
                {
                    Ok((_, delta)) => {
                        self.record(delta);
                        self.collapsed_sections.remove(&format!("{target}.{name}"));
                        self.refresh_preview();
                    }
                    Err(error) => self.fail(error),
                }
            }
            Action::RemoveComponent(target, name) => {
                match self
                    .scene
                    .edit(|edit| edit.set_property(&target, name, &Value::Bool(false)))
                {
                    Ok((_, delta)) => {
                        self.record(delta);
                        self.refresh_preview();
                    }
                    Err(error) => self.fail(error),
                }
            }
            Action::ToggleSection(id) => {
                if !self.collapsed_sections.remove(&id) {
                    self.collapsed_sections.insert(id);
                }
            }
            Action::ToggleBranch(id) => {
                if !self.collapsed_branches.remove(&id) {
                    self.collapsed_branches.insert(id);
                }
            }
            Action::Property {
                target,
                path,
                value,
            } => self.property_edit(target, path, value),
            Action::AddGraphState(graph) => self.add_graph_state(&graph),
            Action::RemoveGraphState(graph, state_id) => {
                match self
                    .scene
                    .edit(|edit| edit.remove_graph_state(&graph, &state_id))
                {
                    Ok(((), delta)) => {
                        self.record(delta);
                        self.refresh_preview();
                    }
                    Err(error) => self.fail(error),
                }
            }
            Action::AddGraphParameter(graph, kind) => self.add_graph_parameter(&graph, kind),
            Action::RemoveGraphParameter(graph, name) => {
                match self
                    .scene
                    .edit(|edit| edit.remove_graph_parameter(&graph, &name))
                {
                    Ok(((), delta)) => {
                        self.record(delta);
                        self.refresh_preview();
                    }
                    Err(error) => self.fail(error),
                }
            }
            Action::AddGraphTransition(graph) => self.add_graph_transition(&graph),
            Action::RemoveGraphTransition(graph, index) => {
                match self
                    .scene
                    .edit(|edit| edit.remove_graph_transition(&graph, index))
                {
                    Ok(((), delta)) => {
                        self.record(delta);
                        self.refresh_preview();
                    }
                    Err(error) => self.fail(error),
                }
            }
            Action::AddGraphCondition(graph, transition) => {
                self.add_graph_condition(&graph, transition);
            }
            Action::RemoveGraphCondition(graph, transition, index) => {
                match self
                    .scene
                    .edit(|edit| edit.remove_graph_condition(&graph, transition, index))
                {
                    Ok(((), delta)) => {
                        self.record(delta);
                        self.refresh_preview();
                    }
                    Err(error) => self.fail(error),
                }
            }
        }
    }

    fn apply_actions(&mut self, actions: impl IntoIterator<Item = Action>) {
        for action in actions {
            self.apply(action);
        }
        if !self.primary {
            self.finish_property();
        }
    }

    fn frame(&mut self, window: &Window, real: Duration) -> Flow {
        if self.exiting {
            return Flow::Exit;
        }
        let lost = match self.session.as_mut() {
            Some(session) => match session.events.condition() {
                GpuCondition::Healthy => false,
                GpuCondition::Lost => true,
                GpuCondition::Fault(message) => {
                    panic!("the graphics device fails: {message}")
                }
            },
            None => false,
        };
        if lost {
            self.rebuild(window);
            return Flow::Continuous;
        }
        let now = Instant::now();
        let mut redraw = std::mem::take(&mut self.redraw) || self.ui_at.is_some_and(|at| at <= now);
        redraw |= self.reload_if_changed();
        if self.child.is_some() && now >= self.player_at {
            self.player_at = now + PLAYER_INTERVAL;
            redraw |= self.poll();
        }
        let (width, height) = window.size();
        if window.is_minimized() || width == 0 || height == 0 {
            return Flow::Idle;
        }
        let scale = window.scale_factor() as f32;
        let logical = Vec2::new(width as f32 / scale, height as f32 / scale);
        let time = self.origin.elapsed().as_secs_f64();
        let asset_error = {
            let Some(session) = self.session.as_mut() else {
                return Flow::Idle;
            };
            let refresh = session.preview.poll();
            redraw |= refresh.is_some();
            refresh
                .filter(|refresh| !refresh.errors.is_empty())
                .map(|refresh| refresh.message())
        };
        if let Some(error) = asset_error {
            self.fail(error);
        }
        self.sync_title(window);
        let busy = self.session.as_ref().is_some_and(|session| {
            session.preview.loading() || session.preview.capturing() || session.preview.playing()
        });
        if !redraw && !busy {
            return self.idle();
        }
        let actions = {
            let Some(session) = self.session.as_mut() else {
                return Flow::Idle;
            };
            let revision = session.preview.revision();
            if self.textures_at != Some(revision) {
                self.textures_at = Some(revision);
                session.register_textures(&self.scene);
            }
            session.preview.advance(real);
            session.ui.update(logical, scale, time)
        };
        self.apply_actions(actions);

        let camera = self.orbit.view();
        let origin = self.selection_origin();
        let index = self.selected_node();
        let basis = index.map_or([Vec3::X, Vec3::Y, Vec3::Z], |at| self.basis(at));
        self.update_hover();

        let state = State {
            scene: &self.scene,
            selected: self.selected.as_ref(),
            dirty: self.document.dirty(&self.scene),
            conflict: self.document.conflicted(),
            playing: self.child.is_some(),
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            mode: self.mode,
            space: self.space,
            debug: self.debug,
            outliner_open: self.outliner_open,
            inspector_open: self.inspector_open,
            tab: self.tab,
            collapsed_branches: &self.collapsed_branches,
            collapsed_sections: &self.collapsed_sections,
            dialog: self.dialog.as_ref(),
            confirm: self.confirm,
            can_save: self.document.writable(),
            message: self.message.as_ref(),
            camera,
            origin,
            basis,
            hover: self.hover,
            active: self.drag.as_ref().map(|drag| drag.drag.grip()),
        };

        let viewport = {
            let Some(session) = self.session.as_mut() else {
                return Flow::Idle;
            };
            let element = {
                let mut ctx = session.ui.build();
                stage::build(&mut ctx, &state)
            };
            session.ui.present(element);
            session.ui.bounds("viewport").unwrap_or_default()
        };
        self.viewport = viewport;
        if viewport.is_empty() {
            let wake = self
                .session
                .as_mut()
                .expect("a drawn editor holds its session")
                .ui
                .finish();
            return self.pace(wake, busy);
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
            self.viewport_texture = Some(dome_gpu::Texture::new(
                session.context.device(),
                "dome editor viewport",
                texture_width,
                texture_height,
                1,
                dome_gpu::TextureFormat::Rgba8Unorm,
                dome_gpu::TextureUsages::RENDER_ATTACHMENT
                    | dome_gpu::TextureUsages::TEXTURE_BINDING,
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
        let viewport = Viewport {
            camera,
            highlight,
            size: (texture_width, texture_height),
            debug: self.debug,
            scene: self.scene.revision(),
            assets: session.preview.revision(),
            pose: session.preview.pose(),
        };
        if self.viewport_rendered != Some(viewport) {
            session.preview.set_camera(camera);
            session.preview.render(scene, SCENE_CLEAR, highlight);
            self.viewport_rendered = Some(viewport);
        }
        if session.surface.width() != width || session.surface.height() != height {
            session.surface.resize(width, height);
        }
        let frame = match session.surface.acquire() {
            Ok(frame) => frame,
            Err(
                GpuFailure::SurfaceOutdated | GpuFailure::SurfaceLost | GpuFailure::SurfaceFormat,
            ) => {
                self.resurface(window);
                return Flow::Continuous;
            }
            Err(GpuFailure::Fault(message)) => panic!("the graphics device fails: {message}"),
        };
        let Some(frame) = frame else {
            let wake = session.ui.finish();
            return self.pace(wake, busy);
        };
        if frame.format() != session.ui.format() {
            frame.discard();
            self.resurface(window);
            return Flow::Continuous;
        }
        let Session { context, ui, .. } = session;
        let mut submission = Submission::new(context.device(), "dome editor");
        ui.render(&mut submission, frame.texture(), Some(scene));
        submission.present(frame);
        submission.submit(&context.queue());
        let wake = ui.finish();
        let caret = ui.edit_caret();
        let editing = ui.editing();
        if editing != self.ime {
            window.set_ime_allowed(editing);
            self.ime = editing;
        }
        if let Some(caret) = caret {
            window.set_ime_cursor_area(
                f64::from(caret.min.x),
                f64::from(caret.min.y),
                f64::from(caret.width()),
                f64::from(caret.height()),
            );
        }
        self.pace(wake, busy)
    }

    fn idle(&self) -> Flow {
        let now = Instant::now();
        let player = self.child.is_some().then_some(self.player_at);
        match (self.ui_at, player) {
            (Some(at), Some(player)) => Flow::Wait(at.min(player).saturating_duration_since(now)),
            (Some(at), None) => Flow::Wait(at.saturating_duration_since(now)),
            (None, Some(player)) => Flow::Wait(player.saturating_duration_since(now)),
            (None, None) => Flow::Idle,
        }
    }

    fn pace(&mut self, wake: Option<Duration>, busy: bool) -> Flow {
        let continuous = busy || wake.is_some_and(|duration| duration.is_zero());
        self.ui_at = match wake {
            Some(duration) if !duration.is_zero() => Some(Instant::now() + duration),
            _ => None,
        };
        if continuous {
            return Flow::Continuous;
        }
        self.idle()
    }

    fn ui_claims_pointer(&self) -> bool {
        self.dialog.is_some()
            || self.session.as_ref().is_some_and(|session| {
                session.ui.modal_open()
                    || session.ui.popup_open_any()
                    || session.ui.pointer_captured()
                    || session.ui.pointer_over_chrome()
                    || session.ui.pointer_over_widget_at(self.pointer)
            })
    }

    fn update_hover(&mut self) {
        let Some(index) = self.selected_node() else {
            self.hover = None;
            return;
        };
        if !self.viewport.contains(self.pointer)
            || self.drag.is_some()
            || self.camera_drag.is_some()
            || self.ui_claims_pointer()
        {
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

    fn gizmo_point(&mut self, point: Vec2) {
        let Some(mut state) = self.drag.take() else {
            return;
        };
        let transform = state.drag.transform(point);
        let previous = state.delta.take();
        if let Some(previous) = &previous {
            previous.revert(&mut self.scene);
        }
        match self
            .scene
            .edit(|edit| edit.set_transform(state.node, transform))
        {
            Ok(((), delta)) => {
                state.delta = Some(delta);
                if let Some(session) = &mut self.session {
                    session.preview.apply(&self.scene, state.node);
                }
            }
            Err(error) => {
                if let Some(previous) = previous {
                    previous.apply(&mut self.scene);
                    state.delta = Some(previous);
                }
                self.fail(error);
            }
        }
        self.drag = Some(state);
    }

    fn press(&mut self, button: MouseButton, scale: f32) {
        if !self.viewport.contains(self.pointer) || self.ui_claims_pointer() {
            return;
        }
        let point = self.pointer;
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
            Pick::Hit(entity) => {
                self.selected = session.preview.node(entity).map(Target::Node);
            }
            Pick::Miss => self.selected = None,
            Pick::Stale => {}
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
        if let Some(button) = self.camera_drag {
            match button {
                MouseButton::Middle => self.orbit.pan(delta),
                _ => self.orbit.rotate(delta),
            }
            return;
        }
        if self.drag.is_some() && !self.viewport.is_empty() {
            self.gizmo_point(self.pointer);
        }
    }

    fn wheel(&mut self, delta: f32) {
        if self.viewport.contains(self.pointer) && !self.ui_claims_pointer() {
            self.orbit.zoom(delta);
        }
    }

    fn key(&mut self, key: Key, repeat: bool) {
        if repeat {
            return;
        }
        if self.confirm.is_some() {
            if key == Key::Escape {
                self.apply(Action::DismissConfirm);
            }
            return;
        }
        if self.dialog.is_some() {
            if key == Key::Escape {
                self.apply(Action::CloseDialog);
            }
            return;
        }
        if self
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
            Key::Character('s') if command => self.apply(Action::Save),
            Key::Character('r') if command => self.apply(Action::Reload),
            Key::Character('z') if command => {
                if self.modifiers.shift_key() {
                    self.apply(Action::Redo);
                } else {
                    self.apply(Action::Undo);
                }
            }
            Key::Character('y') if command => self.apply(Action::Redo),
            Key::Function(5) => {
                if self.child.is_some() {
                    self.apply(Action::Stop);
                } else {
                    self.apply(Action::Play);
                }
            }
            Key::Character('w') if !command => self.apply(Action::Mode(Mode::Translate)),
            Key::Character('e') if !command => self.apply(Action::Mode(Mode::Rotate)),
            Key::Character('r') if !command => self.apply(Action::Mode(Mode::Scale)),
            Key::Character('q') if !command => self.apply(Action::ToggleSpace),
            Key::Character('f') if !command => self.apply(Action::Focus),
            Key::Delete => self.apply(Action::RemoveSelected),
            Key::Escape if self.message.is_some() => self.apply(Action::DismissMessage),
            _ => {}
        }
    }

    fn handle(&mut self, event: &Event, scale: f32, size: (u32, u32)) {
        match event {
            Event::Modifiers(state) => self.modifiers = *state,
            Event::Resized { .. } | Event::ScaleFactorChanged { .. } => {
                if let Some(session) = &mut self.session
                    && (session.surface.width() != size.0 || session.surface.height() != size.1)
                {
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
            Event::Ime(_) => {}
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
        KeyCode::Escape => Key::Escape,
        KeyCode::F5 => Key::Function(5),
        _ => return None,
    })
}

impl Handler for Editor {
    fn opened(&mut self, window: &Window, waker: Waker) {
        self.watch = Some(Watch::new(&self.path, &self.scene, move || waker.wake()));
        self.open(window);
    }

    fn frame(&mut self, window: &Window, real: Duration) -> Flow {
        self.frame(window, real)
    }

    fn event(&mut self, window: &Window, event: Event) {
        self.redraw = true;
        let scale = window.scale_factor() as f32;
        let viewport_captured = self.drag.is_some() || self.camera_drag.is_some();
        self.handle(&event, scale, window.size());
        let forward_to_ui = !viewport_captured
            || matches!(
                event,
                Event::Button { pressed: false, .. }
                    | Event::Focused(false)
                    | Event::CursorLeft
                    | Event::Modifiers(_)
            );
        if forward_to_ui && let Some(session) = &mut self.session {
            session.ui.input_mut().window(&event, scale);
        }
    }

    fn suspended(&mut self, _window: &Window) -> Flow {
        Flow::Exit
    }

    fn closing(&mut self, _window: &Window) -> bool {
        if !self.document.dirty(&self.scene) {
            return true;
        }
        self.confirm = Some(Confirm::Close);
        false
    }

    fn closed(&mut self, _window: &Window) {
        self.stop();
        self.session = None;
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn path_slot(kind: PathKind) -> usize {
    match kind {
        PathKind::Model => 0,
        PathKind::Prefab => 1,
        PathKind::Image => 2,
        PathKind::Sound => 3,
        PathKind::Panorama => 4,
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

fn sanitized_stem(path: &str, fallback: &str) -> String {
    Path::new(path)
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
        .unwrap_or_else(|| String::from(fallback))
}

fn stem_asset(scene: &Scene, kind: ReferenceKind, path: &str, fallback: &str) -> String {
    let base = sanitized_stem(path, fallback);
    unique_asset(scene, kind, &base)
}

fn unique_asset(scene: &Scene, kind: ReferenceKind, name: &str) -> String {
    let mut index = 0usize;
    loop {
        let candidate = if index == 0 {
            name.to_owned()
        } else {
            format!("{name}_{index}")
        };
        if !scene.holds_reference(kind, &candidate) {
            return candidate;
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const SIZE: (u32, u32) = (800, 600);

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Cleanup(PathBuf);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn editor() -> (Editor, Cleanup) {
        let root = std::env::temp_dir().join(format!(
            "dome_editor_test_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        let _ = fs::remove_dir_all(&root);
        let project = Project::create(&root, "Editor Test").expect("the project is created");
        (Editor::new(EditorConfig::new(project)), Cleanup(root))
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
    fn a_project_whose_entry_scene_breaks_opens_with_the_error_it_names() {
        let root = std::env::temp_dir().join(format!(
            "dome_editor_broken_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        let _ = fs::remove_dir_all(&root);
        let project = Project::create(&root, "Broken").expect("the project is created");
        fs::write(project.entry(), "format = \"dome_scene\"\nimages = ")
            .expect("a broken entry scene holds its text");
        let opened = Project::open(&root).expect("the project opens");
        let mut editor = Editor::new(EditorConfig::new(opened));
        let message = editor
            .message
            .as_ref()
            .expect("a broken entry scene opens with its parse error");
        assert!(
            message.text().contains("invalid TOML"),
            "the editor names the broken document: {}",
            message.text(),
        );
        assert!(
            editor.scene.nodes().is_empty(),
            "a broken entry scene opens no node",
        );
        assert!(
            !editor.document.dirty(&editor.scene),
            "a scene that never loaded holds no edit",
        );
        assert!(
            !editor.save(),
            "an editor refuses to save over a scene it never read",
        );
        assert!(
            editor
                .message
                .as_ref()
                .is_some_and(|held| held.text().contains("did not load")),
            "a refused save names the load the editor lost",
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_repeated_key_press_removes_one_node() {
        let (mut editor, _cleanup) = editor();
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
        let (mut editor, _cleanup) = editor();
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
        let (mut editor, _cleanup) = editor();
        let target = editor.selected.clone().expect("the editor starts selected");
        let index = editor.selected_node().expect("the editor starts on a node");
        let before = editor.scene.transform(index).translation;
        assert!(
            !editor.document.dirty(&editor.scene),
            "a new document is clean"
        );

        editor.handle(&button(true), 1.0, SIZE);
        editor.apply_actions((1..=4).map(|step| Action::Property {
            target: target.clone(),
            path: String::from("transform.translation.x"),
            value: Value::Number(step as f32),
        }));
        editor.handle(&button(false), 1.0, SIZE);
        editor.apply_actions(Vec::new());
        assert_eq!(editor.scene.transform(index).translation.x, 4.0);
        assert!(
            editor.document.dirty(&editor.scene),
            "an edit dirties the document"
        );

        editor.undo();
        assert_eq!(
            editor.scene.transform(index).translation,
            before,
            "a property drag lands in the history as one step",
        );
        assert!(
            !editor.document.dirty(&editor.scene),
            "an undo back to the saved transform cleans the document",
        );
        editor.undo();
        assert_eq!(editor.scene.transform(index).translation, before);
    }

    #[test]
    fn a_refused_property_edit_keeps_the_value_before_it() {
        let (mut editor, _cleanup) = editor();
        let target = editor.selected.clone().expect("the editor starts selected");
        let index = editor.selected_node().expect("the editor starts on a node");
        let before = editor.scene.transform(index).scale;

        editor.handle(&button(true), 1.0, SIZE);
        editor.apply_actions([Action::Property {
            target: target.clone(),
            path: String::from("transform.scale.x"),
            value: Value::Number(2.0),
        }]);
        editor.apply_actions([Action::Property {
            target,
            path: String::from("transform.scale.x"),
            value: Value::Number(0.0),
        }]);
        assert_eq!(
            editor.scene.transform(index).scale.x,
            2.0,
            "a refused edit keeps the value the edit before it landed",
        );
        editor.handle(&button(false), 1.0, SIZE);
        editor.apply_actions(Vec::new());

        editor.undo();
        assert_eq!(
            editor.scene.transform(index).scale,
            before,
            "one undo crosses the refused edit to the scale it started with",
        );
    }
}

use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as Process};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dome_animation::{Comparison, Parameter};
use dome_asset::{Import, Watch};
use dome_gpu::{GpuContext, GpuRequest, Submission, WindowHandle};
use dome_math::{Quat, Rect, Transform, Vec2, Vec3};
use dome_scene::{Atmosphere, Delta, Node, ReferenceKind, Scene, Shape, Target, Value};
use dome_schema::Registry;
use dome_ui::InputKey as Key;
use dome_window::{
    Event, Flow, Handler, KeyCode, ModifiersState, MouseButton, Window, WindowConfig,
    run as run_window,
};

use crate::action::{Action, AssetKind, NodeKind, ParameterKind, PathDialog, PathKind, Sheet, Tab};
use crate::camera::{self, Orbit};
use crate::gizmo::{self, Grip, Mode, Space};
use crate::history::History;
use crate::preview::Preview;
use crate::session::Session;
use crate::stage::{self, State};

const SCENE_CLEAR: [f32; 4] = [0.035, 0.043, 0.063, 1.0];

pub struct EditorConfig {
    pub window: WindowConfig,
    pub scene: PathBuf,
    pub registry: Arc<Registry>,
    pub debug: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            window: WindowConfig::new("Dome", 1440, 900),
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
    registry: Arc<Registry>,
    path: PathBuf,
    selected: Option<Target>,
    dirty: bool,
    titled_dirty: Option<bool>,
    error: Option<String>,
    child: Option<Child>,
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
    watch: Watch,
    modifiers: ModifiersState,
    origin: Instant,
    hover: Option<Grip>,
    viewport_texture: Option<dome_gpu::Texture>,
}

impl Editor {
    fn new(config: EditorConfig) -> Self {
        let path = config.scene;
        let registry = config.registry;
        let exists = path.exists();
        let scene = if exists {
            Scene::load(&path, registry.clone())
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
            registry,
            path,
            selected,
            dirty: !exists,
            titled_dirty: None,
            error: None,
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
        let mut preview = Preview::new(&context, &self.scene, Arc::clone(&self.registry));
        preview.set_debug(self.debug);
        self.session = Some(Session::new(context, surface, preview, &self.scene));
        self.sync_title(window);
    }

    fn sync_title(&mut self, window: &Window) {
        if self.titled_dirty == Some(self.dirty) {
            return;
        }
        self.titled_dirty = Some(self.dirty);
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("scene.toml");
        if self.dirty {
            window.set_title(&format!("Dome — {name} •"));
        } else {
            window.set_title(&format!("Dome — {name}"));
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

    fn save(&mut self) {
        self.finish_drag();
        self.finish_property();
        self.scene.save(&self.path);
        self.rewatch();
        self.dirty = false;
    }

    fn reload(&mut self) {
        self.finish_drag();
        self.finish_property();
        let registry = self.scene.schema_arc();
        let loaded = catch_unwind(AssertUnwindSafe(|| Scene::load(&self.path, registry)));
        match loaded {
            Ok(scene) => {
                self.scene = scene;
                self.history.reset();
                self.dirty = false;
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
            let registry = self.scene.schema_arc();
            if let Ok(scene) = catch_unwind(AssertUnwindSafe(|| Scene::load(&self.path, registry)))
            {
                self.scene = scene;
                self.drag = None;
                self.property = None;
                self.history.reset();
                self.dirty = false;
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
            self.clamp();
            self.refresh_preview();
        }
    }

    fn redo(&mut self) {
        self.finish_drag();
        self.finish_property();
        if self.history.redo(&mut self.scene) {
            self.dirty = true;
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
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn poll(&mut self) {
        let exited = self
            .child
            .as_mut()
            .is_some_and(|child| child.try_wait().ok().flatten().is_some());
        if exited {
            self.child = None;
        }
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
            Err(error) => self.error = Some(error),
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
            Err(error) => self.error = Some(error),
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
        let parent = self.parent_for_insert();
        let outcome = match dialog.kind {
            PathKind::Model => {
                let id = stem_asset(&self.scene, ReferenceKind::Model, &trimmed, "model");
                self.scene
                    .edit(|edit| edit.import_model(&id, &trimmed, parent.as_deref()))
                    .map(|(target, delta)| (target, delta, Tab::Scene))
            }
            PathKind::Prefab => {
                let stem = sanitized_stem(&trimmed, "prefab");
                self.scene
                    .edit(|edit| {
                        let id = match &parent {
                            Some(parent) => edit.child_id(parent, &stem),
                            None => unique_node(edit, &stem),
                        };
                        match &parent {
                            Some(parent) => {
                                edit.insert_child(parent, Node::instance(&id, &trimmed))
                            }
                            None => edit.add_instance(&id, &trimmed),
                        }
                    })
                    .map(|(target, delta)| (target, delta, Tab::Scene))
            }
            PathKind::Image => {
                let id = stem_asset(&self.scene, ReferenceKind::Image, &trimmed, "image");
                self.scene
                    .edit(|edit| edit.add_image(&id, &trimmed))
                    .map(|(target, delta)| (target, delta, Tab::Asset))
            }
            PathKind::Sound => {
                let id = stem_asset(&self.scene, ReferenceKind::Sound, &trimmed, "sound");
                self.scene
                    .edit(|edit| edit.add_sound(&id, &trimmed))
                    .map(|(target, delta)| (target, delta, Tab::Asset))
            }
            PathKind::Panorama => {
                let id = stem_asset(
                    &self.scene,
                    ReferenceKind::Environment,
                    &trimmed,
                    "environment",
                );
                self.scene
                    .edit(|edit| edit.add_environment(&id, &trimmed))
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
                self.error = Some(error);
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
            Err(error) => self.error = Some(error),
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
            Err(error) => self.error = Some(error),
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
            Err(error) => self.error = Some(error),
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
            Err(error) => self.error = Some(error),
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
            Err(error) => self.error = Some(error),
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
            Err(error) => self.error = Some(error),
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
            Err(error) => self.error = Some(error),
        }
    }

    fn apply(&mut self, action: Action) {
        if !matches!(action, Action::EditDialog(_) | Action::DismissError) {
            self.error = None;
        }
        match action {
            Action::Save => self.save(),
            Action::Reload => self.reload(),
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
            Action::DismissError => self.error = None,
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
                    Err(error) => self.error = Some(error),
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
                    Err(error) => self.error = Some(error),
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
                    Err(error) => self.error = Some(error),
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
                    Err(error) => self.error = Some(error),
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
                    Err(error) => self.error = Some(error),
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
                    Err(error) => self.error = Some(error),
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
        self.poll();
        self.reload_if_changed();
        let (width, height) = window.size();
        if window.is_minimized() || width == 0 || height == 0 {
            return Flow::Continue;
        }
        let scale = window.scale_factor() as f32;
        let logical = Vec2::new(width as f32 / scale, height as f32 / scale);
        let time = self.origin.elapsed().as_secs_f64();
        let actions = {
            let Some(session) = self.session.as_mut() else {
                return Flow::Continue;
            };
            session.preview.poll();
            session.register_textures(&self.scene);
            session.preview.advance(real);
            session.ui.update(logical, scale, time)
        };
        self.apply_actions(actions);
        self.sync_title(window);

        let camera = self.orbit.view();
        let origin = self.selection_origin();
        let index = self.selected_node();
        let basis = index.map_or([Vec3::X, Vec3::Y, Vec3::Z], |at| self.basis(at));
        self.update_hover();

        let state = State {
            scene: &self.scene,
            selected: self.selected.as_ref(),
            dirty: self.dirty,
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
            error: self.error.as_deref(),
            camera,
            origin,
            basis,
            hover: self.hover,
            active: self.drag.as_ref().map(|drag| drag.drag.grip()),
        };

        let viewport = {
            let Some(session) = self.session.as_mut() else {
                return Flow::Continue;
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
            self.viewport_texture = Some(dome_gpu::Texture::new(
                session.context.device(),
                "dome editor viewport",
                texture_width,
                texture_height,
                1,
                dome_gpu::TextureFormat::Rgba8Unorm,
                dome_gpu::TextureUsages::RENDER_ATTACHMENT | dome_gpu::TextureUsages::SAMPLED,
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
        let Session { context, ui, .. } = session;
        let mut submission = Submission::new(context.device(), "dome editor");
        ui.render(&mut submission, frame.texture(), Some(scene));
        submission.present(frame);
        submission.submit(context.queue());
        Flow::Continue
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
            Key::Escape if self.error.is_some() => self.apply(Action::DismissError),
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
        KeyCode::Escape => Key::Escape,
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
        editor.apply_actions((1..=4).map(|step| Action::Property {
            target: target.clone(),
            path: String::from("transform.translation.x"),
            value: Value::Number(step as f32),
        }));
        editor.handle(&button(false), 1.0, SIZE);
        editor.apply_actions(Vec::new());
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
}

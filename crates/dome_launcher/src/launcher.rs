use crate::action::Action;
use crate::session::Session;
use crate::stage::{self, State};
use dome_gpu::{Gpu, GpuRequest, Submission};
use dome_math::Vec2;
use dome_project::{Catalog, Project};
use dome_window::{Event, Flow, Handler, KeyCode, Waker, Window, WindowConfig, run as run_window};
use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub struct LauncherConfig {
    pub catalog: Catalog,
    pub editor: PathBuf,
}

pub fn run(config: LauncherConfig) {
    let window = WindowConfig {
        minimum_size: Some((760, 480)),
        ..WindowConfig::new("Dome", 1080, 700)
    };
    run_window(window, Launcher::new(config));
}

pub(crate) struct Dialog {
    pub(crate) input: String,
    pub(crate) focus: bool,
    pub(crate) invalid: bool,
}

struct Launcher {
    catalog: Catalog,
    editor: PathBuf,
    dialog: Option<Dialog>,
    error: Option<String>,
    launch: Option<Project>,
    child: Option<Child>,
    session: Option<Session>,
    origin: Instant,
    redraw: bool,
    ime: bool,
    ui_at: Option<Instant>,
}

impl Launcher {
    fn new(config: LauncherConfig) -> Self {
        Self {
            catalog: config.catalog,
            editor: config.editor,
            dialog: None,
            error: None,
            launch: None,
            child: None,
            session: None,
            origin: Instant::now(),
            redraw: true,
            ime: false,
            ui_at: None,
        }
    }

    fn open(&mut self, window: &Window) {
        let context = Gpu::open_window(&GpuRequest::default(), window.target())
            .unwrap_or_else(|error| panic!("the launcher opens its render device: {error}"));
        let (width, height) = window.size();
        assert!(
            width > 0 && height > 0,
            "a window of {width} by {height} presents no surface"
        );
        let surface = context.surface(width, height);
        self.session = Some(Session::new(context, surface));
        self.ime = false;
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Open(index) => {
                let Some(recent) = self.catalog.entries().get(index) else {
                    return;
                };
                let path = recent.path().to_path_buf();
                self.select(&path);
            }
            Action::Forget(index) => {
                if index < self.catalog.entries().len() {
                    self.catalog.forget(index);
                }
            }
            Action::Choose => {
                self.dialog = Some(Dialog {
                    input: String::new(),
                    focus: true,
                    invalid: false,
                });
            }
            Action::EditPath(text) => {
                if let Some(dialog) = self.dialog.as_mut() {
                    dialog.input = text;
                    dialog.invalid = false;
                }
            }
            Action::SubmitPath(text) => self.submit(Some(text)),
            Action::Submit => self.submit(None),
            Action::CloseDialog => self.dialog = None,
            Action::DismissError => self.error = None,
        }
    }

    fn select(&mut self, path: &Path) {
        if !Project::holds(path) {
            self.error = Some(format!("{} holds no project", path.display()));
            return;
        }
        match catch_unwind(AssertUnwindSafe(|| Project::open(path))) {
            Ok(project) => {
                self.catalog.record(&project);
                self.launch = Some(project);
            }
            Err(payload) => self.error = Some(message(payload.as_ref())),
        }
    }

    fn submit(&mut self, input: Option<String>) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        dialog.input = input.unwrap_or_else(|| dialog.input.clone());
        dialog.focus = true;
        dialog.invalid = false;
        let trimmed = dialog.input.trim().to_string();
        if trimmed.is_empty() {
            dialog.invalid = true;
            self.dialog = Some(dialog);
            return;
        }
        let path = PathBuf::from(trimmed);
        if Project::holds(&path) {
            self.select(&path);
            if self.launch.is_none() {
                dialog.invalid = true;
                self.dialog = Some(dialog);
            }
            return;
        }
        if Project::vacant(&path)
            && let Some(name) = path.file_name().and_then(|name| name.to_str())
        {
            match catch_unwind(AssertUnwindSafe(|| Project::create(&path, name))) {
                Ok(project) => {
                    self.catalog.record(&project);
                    self.launch = Some(project);
                }
                Err(payload) => {
                    self.error = Some(message(payload.as_ref()));
                    dialog.invalid = true;
                    self.dialog = Some(dialog);
                }
            }
            return;
        }
        self.error = Some(format!("{} holds no project", path.display()));
        dialog.invalid = true;
        self.dialog = Some(dialog);
    }

    fn spawn(&mut self, project: &Project) {
        let child = Command::new(&self.editor)
            .arg("--project")
            .arg(project.root())
            .spawn()
            .unwrap_or_else(|error| {
                panic!("the launcher starts {}: {error}", self.editor.display());
            });
        self.child = Some(child);
    }

    fn frame(&mut self, window: &Window, _real: Duration) -> Flow {
        if let Some(failure) = self
            .session
            .as_ref()
            .and_then(|session| session.context.failure())
        {
            eprintln!("{failure}");
            return Flow::Exit;
        }
        let now = Instant::now();
        let mut redraw = std::mem::take(&mut self.redraw) || self.ui_at.is_some_and(|at| at <= now);
        let (width, height) = window.size();
        if window.is_minimized() || width == 0 || height == 0 {
            return Flow::Idle;
        }
        if let Some(session) = &mut self.session
            && (session.surface.width() != width || session.surface.height() != height)
        {
            session.surface.resize(width, height);
        }
        let scale = window.scale_factor() as f32;
        let logical = Vec2::new(width as f32 / scale, height as f32 / scale);
        let time = self.origin.elapsed().as_secs_f64();
        let actions = {
            let Some(session) = self.session.as_mut() else {
                return Flow::Idle;
            };
            session.ui.update(logical, scale, time)
        };
        for action in actions {
            self.apply(action);
            redraw = true;
        }
        if let Some(project) = self.launch.take() {
            window.hide();
            self.spawn(&project);
            return Flow::Exit;
        }
        if !redraw {
            return self.idle();
        }
        let state = State {
            catalog: &self.catalog,
            dialog: self.dialog.as_ref(),
            error: self.error.as_deref(),
        };
        let Some(session) = self.session.as_mut() else {
            return Flow::Idle;
        };
        if !session.adopts() {
            session.renew();
        }
        let element = {
            let mut ctx = session.ui.build();
            stage::build(&mut ctx, &state)
        };
        session.ui.present(element);
        let frame = match session.surface.acquire() {
            Ok(frame) => frame,
            Err(failure) => {
                eprintln!("{failure}");
                return Flow::Exit;
            }
        };
        let Some(frame) = frame else {
            let wake = session.ui.finish();
            return self.pace(wake);
        };
        let Session { context, ui, .. } = session;
        let mut submission = Submission::new(context.device(), "dome launcher");
        ui.render(&mut submission, frame.texture(), None);
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
        self.pace(wake)
    }

    fn idle(&self) -> Flow {
        match self.ui_at {
            Some(at) => Flow::Wait(at.saturating_duration_since(Instant::now())),
            None => Flow::Idle,
        }
    }

    fn pace(&mut self, wake: Option<Duration>) -> Flow {
        match wake {
            Some(duration) if duration.is_zero() => {
                self.ui_at = None;
                Flow::Continuous
            }
            Some(duration) => {
                self.ui_at = Some(Instant::now() + duration);
                Flow::Wait(duration)
            }
            None => {
                self.ui_at = None;
                Flow::Idle
            }
        }
    }
}

impl Handler for Launcher {
    fn opened(&mut self, window: &Window, _waker: Waker) {
        self.open(window);
    }

    fn frame(&mut self, window: &Window, real: Duration) -> Flow {
        self.frame(window, real)
    }

    fn event(&mut self, window: &Window, event: Event) {
        self.redraw = true;
        let scale = window.scale_factor() as f32;
        if let Some(session) = &mut self.session {
            session.ui.input_mut().window(&event, scale);
        }
        if let Event::Key {
            code: KeyCode::Escape,
            pressed: true,
            ..
        } = event
        {
            if self.dialog.is_some() {
                self.dialog = None;
            } else if self.error.is_some() {
                self.error = None;
            }
        }
    }

    fn suspended(&mut self, _window: &Window) -> Flow {
        Flow::Exit
    }

    fn closed(&mut self, _window: &Window) {
        self.session = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
    }
}

fn message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    if let Some(text) = payload.downcast_ref::<&str>() {
        return String::from(*text);
    }
    String::from("the launcher fails")
}

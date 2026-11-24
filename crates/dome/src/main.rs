mod script;

use dome_app::{App, AppConfig, HeadlessConfig, SceneConfig};
use dome_editor::{EditorConfig, run as run_editor};
use dome_launcher::{LauncherConfig, run as run_launcher};
use dome_project::{Catalog, Project};
use dome_scene::Folder;
use dome_window::WindowConfig;
use std::path::PathBuf;

enum Command {
    Launcher,
    Editor {
        project: Project,
        debug: bool,
    },
    Play {
        project: Project,
        debug: bool,
    },
    Headless {
        project: Project,
        capture: Capture,
        debug: bool,
    },
}

struct Capture {
    screenshot: Option<PathBuf>,
    frames: Option<u64>,
    realtime: bool,
    width: u32,
    height: u32,
}

impl Capture {
    fn new() -> Self {
        Self {
            screenshot: None,
            frames: None,
            realtime: false,
            width: 1280,
            height: 720,
        }
    }
}

fn main() {
    match parse() {
        Command::Launcher => {
            let editor = std::env::current_exe().expect("the launcher names its own executable");
            run_launcher(LauncherConfig {
                catalog: Catalog::load(),
                editor,
            });
        }
        Command::Editor { project, debug } => run_editor(EditorConfig {
            debug,
            ..EditorConfig::new(project)
        }),
        Command::Play { project, debug } => play(project, debug),
        Command::Headless {
            project,
            capture,
            debug,
        } => headless(project, capture, debug),
    }
}

fn play(project: Project, debug: bool) {
    let scene = project.entry();
    let root = project.root().to_path_buf();
    let scripts = project.folder(Folder::Script);
    let mut app = App::with_config(AppConfig {
        window: WindowConfig::new(project.name(), 1280, 720),
        scene: Some(SceneConfig { path: scene, root }),
        debug,
        ..Default::default()
    });
    let faults = match script::install(&mut app, &scripts) {
        Ok(faults) => faults,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    app.run();
    report(&faults);
}

fn report(faults: &script::Faults) {
    if faults.take().is_some() {
        std::process::exit(1);
    }
}

fn headless(project: Project, capture: Capture, debug: bool) {
    let scene = project.entry();
    let root = project.root().to_path_buf();
    let scripts = project.folder(Folder::Script);
    let mut app = App::with_config(AppConfig {
        scene: Some(SceneConfig { path: scene, root }),
        debug,
        ..Default::default()
    });
    let faults = match script::install(&mut app, &scripts) {
        Ok(faults) => faults,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let shot = app.run_headless(HeadlessConfig {
        width: capture.width,
        height: capture.height,
        frames: capture.frames,
        realtime: capture.realtime,
        screenshot: capture.screenshot.is_some(),
    });
    if let Some(error) = app.reload_error() {
        eprintln!("{error}");
        std::process::exit(1);
    }
    report(&faults);
    if let Some(path) = capture.screenshot {
        let png = shot.unwrap_or_else(|| panic!("a headless run captures the frame it renders"));
        std::fs::write(&path, png)
            .unwrap_or_else(|error| panic!("the run writes {}: {error}", path.display()));
    }
}

fn parse() -> Command {
    let mut items = std::env::args().skip(1);
    let Some(command) = items.next() else {
        return Command::Launcher;
    };
    match command.as_str() {
        "--project" => Command::Editor {
            project: Project::open(project_root(&mut items, "--project")),
            debug: debug_flag(&mut items, "--project"),
        },
        "--play" => Command::Play {
            project: Project::open(project_root(&mut items, "--play")),
            debug: debug_flag(&mut items, "--play"),
        },
        "--headless" => {
            let project = Project::open(project_root(&mut items, "--headless"));
            let mut capture = Capture::new();
            let mut debug = false;
            while let Some(item) = items.next() {
                match item.as_str() {
                    "--debug" => debug = true,
                    "--screenshot" => {
                        let path = items
                            .next()
                            .unwrap_or_else(|| panic!("--screenshot names a file"));
                        capture.screenshot = Some(PathBuf::from(path));
                    }
                    "--frames" => {
                        let frames = items
                            .next()
                            .unwrap_or_else(|| panic!("--frames names a whole number"));
                        let frames = frames.parse::<u64>().unwrap_or_else(|_| {
                            panic!("--frames holds {frames}, which is no whole number")
                        });
                        assert!(frames > 0, "--frames walks at least one frame");
                        capture.frames = Some(frames);
                    }
                    "--width" => capture.width = count(&mut items, "--width"),
                    "--height" => capture.height = count(&mut items, "--height"),
                    "--realtime" => capture.realtime = true,
                    other => panic!("the headless player holds no flag {other}"),
                }
            }
            Command::Headless {
                project,
                capture,
                debug,
            }
        }
        other => panic!("the engine holds no command {other}"),
    }
}

fn project_root(items: &mut impl Iterator<Item = String>, command: &str) -> PathBuf {
    let root = items
        .next()
        .unwrap_or_else(|| panic!("{command} names a project folder"));
    PathBuf::from(root)
}

fn debug_flag(items: &mut impl Iterator<Item = String>, command: &str) -> bool {
    let mut debug = false;
    for item in items {
        match item.as_str() {
            "--debug" => debug = true,
            other => panic!("{command} holds no flag {other}"),
        }
    }
    debug
}

fn count(items: &mut impl Iterator<Item = String>, flag: &str) -> u32 {
    let value = items
        .next()
        .unwrap_or_else(|| panic!("{flag} names a whole number"));
    let count = value
        .parse::<u32>()
        .unwrap_or_else(|_| panic!("{flag} holds {value}, which is no whole number"));
    assert!(count > 0, "{flag} holds a count of no pixel");
    count
}

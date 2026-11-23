use dome_app::{App, AppConfig, HeadlessConfig};
use dome_editor::{EditorConfig, run as run_editor};
use std::path::PathBuf;

struct Arguments {
    scene: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    play: bool,
    headless: bool,
    debug: bool,
    frames: Option<u64>,
    realtime: bool,
    width: u32,
    height: u32,
    sized: bool,
}

impl Default for Arguments {
    fn default() -> Self {
        Self {
            scene: None,
            screenshot: None,
            play: false,
            headless: false,
            debug: false,
            frames: None,
            realtime: false,
            width: 1280,
            height: 720,
            sized: false,
        }
    }
}

fn main() {
    let arguments = parse();
    if arguments.headless {
        let scene = arguments
            .scene
            .clone()
            .unwrap_or_else(|| panic!("a headless run names the scene it runs"));
        let shot = App::with_config(AppConfig {
            scene: Some(scene),
            debug: arguments.debug,
            ..Default::default()
        })
        .run_headless(HeadlessConfig {
            width: arguments.width,
            height: arguments.height,
            frames: arguments.frames,
            realtime: arguments.realtime,
            screenshot: arguments.screenshot.is_some(),
        });
        if let Some(path) = &arguments.screenshot {
            let png =
                shot.unwrap_or_else(|| panic!("a headless run captures the frame it renders"));
            std::fs::write(path, png)
                .unwrap_or_else(|error| panic!("the run writes {}: {error}", path.display()));
        }
    } else if arguments.play {
        let scene = arguments
            .scene
            .unwrap_or_else(|| panic!("a player names the scene it runs"));
        App::with_config(AppConfig {
            scene: Some(scene),
            debug: arguments.debug,
            ..Default::default()
        })
        .run();
    } else {
        run_editor(EditorConfig {
            scene: arguments
                .scene
                .unwrap_or_else(|| PathBuf::from("scene.toml")),
            debug: arguments.debug,
            ..Default::default()
        });
    }
}

fn parse() -> Arguments {
    let mut arguments = Arguments::default();
    let mut items = std::env::args().skip(1);
    while let Some(item) = items.next() {
        match item.as_str() {
            "--play" => arguments.play = true,
            "--headless" => arguments.headless = true,
            "--realtime" => arguments.realtime = true,
            "--debug" => arguments.debug = true,
            "--screenshot" => {
                let path = items
                    .next()
                    .unwrap_or_else(|| panic!("--screenshot names a file"));
                arguments.screenshot = Some(PathBuf::from(path));
            }
            "--frames" => {
                let frames = items
                    .next()
                    .unwrap_or_else(|| panic!("--frames names a whole number"));
                let frames = frames.parse::<u64>().unwrap_or_else(|_| {
                    panic!("--frames holds {frames}, which is no whole number")
                });
                assert!(frames > 0, "--frames walks at least one frame");
                arguments.frames = Some(frames);
            }
            "--width" => {
                arguments.width = count(&mut items, "--width");
                arguments.sized = true;
            }
            "--height" => {
                arguments.height = count(&mut items, "--height");
                arguments.sized = true;
            }
            other if other.starts_with("--") => {
                panic!("the engine holds no flag {other}")
            }
            other => {
                assert!(arguments.scene.is_none(), "the editor opens one scene");
                arguments.scene = Some(PathBuf::from(other));
            }
        }
    }
    assert!(
        !(arguments.headless && arguments.play),
        "--headless already runs the player it frames"
    );
    assert!(
        arguments.headless || !arguments.realtime,
        "--realtime drives the fixed clock a headless run keeps"
    );
    assert!(
        arguments.headless || arguments.frames.is_none(),
        "--frames limits the frames a headless run walks"
    );
    assert!(
        arguments.headless || !arguments.sized,
        "--width and --height size a headless frame"
    );
    assert!(
        arguments.headless || arguments.screenshot.is_none(),
        "--screenshot captures a headless frame"
    );
    arguments
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

use dome_audio::{
    AudioBus, AudioFilter, AudioListener as PlayingListener, AudioSource as Playing, Channels,
    FilterKind, Sound, Source,
};
use dome_ecs::World;
use dome_render::{Material, Mesh};
use dome_scene::{
    Assets, ReferenceKind, Scene, SceneNode, SkinData, Target, Value, observe_runtime,
    read_runtime_property, set_runtime_property,
};

const SCENE: &str = r#"format = "dome_scene"
images = []
sounds = [{ id = "tone", path = "tone.wav" }]
meshes = []
materials = []

[[nodes]]
id = "speaker"

[nodes.transform]
translation = [0.0, 1.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.audio_source]
sound = "tone"
bus = "quiet"
gain = 0.5
looping = true
spatial = true
min_distance = 1.0
max_distance = 20.0
pitch = 1.0
autoplay = true

[[nodes]]
id = "camera"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.audio_listener]

[[nodes]]
id = "master"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.audio_bus]
parent = ""
send = ""
gain_db = 0.0
mute = false
send_db = 0.0

[[nodes]]
id = "quiet"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[nodes.audio_bus]
parent = "master"
send = ""
gain_db = -12.0
mute = false
send_db = 0.0

[nodes.audio_filter]
enabled = true
kind = "low_pass"
frequency = 4000.0
q = 0.707
gain_db = 0.0
"#;

struct Mock;

impl Assets for Mock {
    fn mesh(&self, _id: &str) -> Mesh {
        panic!("the audio scene renders no mesh")
    }

    fn material(&self, _id: &str) -> Material {
        panic!("the audio scene renders no material")
    }

    fn skin(&self, _id: &str) -> SkinData {
        panic!("the audio scene deforms no skin")
    }

    fn animation(&self, _id: &str) -> std::sync::Arc<dome_animation::Clip> {
        panic!("the audio scene plays no animation")
    }

    fn sound(&self, _id: &str) -> Source {
        Source::decoded(std::sync::Arc::new(Sound::new(
            8_000,
            Channels::Mono,
            vec![0.0, 0.5, -0.5, 0.0],
        )))
    }
}

#[test]
fn an_authored_audio_source_installs_and_edits() {
    let mut scene = Scene::parse(SCENE, ".", "the audio scene");
    let mut world = World::new();
    let entities = scene.install(&mut world, &Mock);
    let speaker = entities[0];
    let source = world
        .get::<Playing>(speaker)
        .expect("the speaker holds its audio source");
    assert!(source.is_playing());
    assert_eq!(source.gain(), 0.5);
    assert_eq!(source.bus(), "quiet");
    assert!(source.looping());
    assert!(world.get::<PlayingListener>(entities[1]).is_some());
    let quiet = entities
        .iter()
        .copied()
        .find(|entity| {
            world
                .get::<SceneNode>(*entity)
                .is_some_and(|node| node.id == "quiet")
        })
        .expect("the scene holds the quiet bus");
    let bus = world.get::<AudioBus>(quiet).expect("the quiet bus");
    assert_eq!(bus.parent(), Some("master"));
    assert_eq!(bus.gain_db(), -12.0);
    let filter = world.get::<AudioFilter>(quiet).expect("the quiet filter");
    assert_eq!(filter.kind(), FilterKind::LowPass);
    let sound = Target::Sound(String::from("tone"));
    assert_eq!(scene.property(&sound, "stream"), Some(Value::Bool(false)));
    edit(&mut scene, |edit| {
        edit.set_property(&sound, "stream", &Value::Bool(true))
    })
    .expect("a sound streams");
    assert!(scene.write().contains("stream = true"));
    assert_eq!(source.priority(), 0);
    assert_eq!(
        scene.asset_ids(ReferenceKind::AudioBus),
        vec!["master", "quiet"],
    );
    let target = Target::Node(String::from("speaker"));
    set_runtime_property(
        &mut world,
        scene.registry(),
        &target,
        "audio_source.gain",
        &Value::Number(0.25),
    )
    .expect("the runtime edits the audio gain");
    set_runtime_property(
        &mut world,
        scene.registry(),
        &target,
        "audio_source.playing",
        &Value::Bool(false),
    )
    .expect("the runtime stops the audio source");
    let bus_target = Target::Node(String::from("quiet"));
    set_runtime_property(
        &mut world,
        scene.registry(),
        &bus_target,
        "audio_bus.gain_db",
        &Value::Number(-6.0),
    )
    .expect("the runtime edits the bus gain");
    set_runtime_property(
        &mut world,
        scene.registry(),
        &bus_target,
        "audio_filter.frequency",
        &Value::Number(2000.0),
    )
    .expect("the runtime edits the filter");
    assert_eq!(
        world.get::<AudioBus>(quiet).map(AudioBus::gain_db),
        Some(-6.0)
    );
    assert_eq!(
        world.get::<AudioFilter>(quiet).map(AudioFilter::frequency),
        Some(2000.0),
    );
    assert_eq!(
        read_runtime_property(
            &world,
            scene.registry(),
            &bus_target,
            "audio_filter.frequency"
        ),
        Some(Value::Number(2000.0)),
    );
    let source = world.get::<Playing>(speaker).expect("the speaker");
    assert_eq!(source.gain(), 0.25);
    assert!(!source.is_playing());
    assert_eq!(
        read_runtime_property(&world, scene.registry(), &target, "audio_source.playing"),
        Some(Value::Bool(false)),
    );
    let state =
        dome_toml::Toml::object([("nodes", observe_runtime(&world, scene.registry()))]).write();
    assert!(state.contains("sound = \"tone\""));
    assert!(state.contains("bus = \"quiet\""));
    assert!(state.contains("playing = false"));
    assert!(state.contains("gain_db = -6.0"));
    let written = scene.write();
    assert!(written.contains("[[sounds]]"));
    assert!(written.contains("[nodes.audio_source]"));
    assert!(written.contains("[nodes.audio_bus]"));
    assert!(written.contains("parent = \"master\""));
    assert!(written.contains("[nodes.audio_filter]"));
    assert!(
        edit(&mut scene, |edit| edit
            .remove(&Target::Node(String::from("quiet"))))
        .is_err()
    );
    assert!(
        edit(&mut scene, |edit| edit.set_property(
            &bus_target,
            "audio_bus",
            &Value::Bool(false),
        ))
        .is_err(),
    );
    assert_eq!(
        scene.property(&bus_target, "audio_bus"),
        Some(Value::Bool(true))
    );
    assert!(
        edit(&mut scene, |edit| {
            edit.set_property(
                &Target::Node(String::from("master")),
                "audio_bus.parent",
                &Value::Text(String::from("quiet")),
            )
        })
        .is_err(),
    );
    assert_eq!(
        scene.property(&Target::Node(String::from("master")), "audio_bus.parent"),
        Some(Value::Text(String::new())),
    );
    edit(&mut scene, |edit| {
        edit.set_property(
            &Target::Node(String::from("master")),
            "audio_bus.gain_db",
            &Value::Number(-3.0),
        )
    })
    .expect("a bus gain edits without touching the graph");
    let extra = edit(&mut scene, |edit| edit.add_node("extra")).expect("the scene adds a node");
    edit(&mut scene, |edit| {
        edit.set_property(&extra, "audio_bus", &Value::Bool(true))
    })
    .expect("a new bus joins the root");
    assert_eq!(
        scene.property(&extra, "audio_bus.parent"),
        Some(Value::Text(String::from("master"))),
    );
    assert!(
        edit(&mut scene, |edit| edit.set_property(
            &extra,
            "audio_bus.parent",
            &Value::Text(String::new()),
        ))
        .is_err(),
    );
    assert_eq!(
        scene.property(&extra, "audio_bus.parent"),
        Some(Value::Text(String::from("master"))),
    );
    let loose = edit(&mut scene, |edit| edit.add_node("loose")).expect("the scene adds a node");
    assert!(
        edit(&mut scene, |edit| edit.set_property(
            &loose,
            "audio_filter",
            &Value::Bool(true)
        ))
        .is_err(),
    );
    assert_eq!(
        scene.property(&loose, "audio_filter"),
        Some(Value::Bool(false)),
    );
    let removed = edit(&mut scene, |edit| {
        edit.remove(&Target::Sound(String::from("tone")))
    });
    assert!(removed.is_err());
}

#[test]
fn a_scene_that_differs_only_in_sound_is_not_equal() {
    let scene = Scene::parse(SCENE, ".", "the audio scene");
    let streamed = SCENE.replace(
        "sounds = [{ id = \"tone\", path = \"tone.wav\" }]",
        "sounds = [{ id = \"tone\", path = \"tone.wav\", stream = true }]",
    );
    let other = Scene::parse(&streamed, ".", "the streamed audio scene");
    assert!(!scene.sounds()[0].stream);
    assert!(other.sounds()[0].stream);
    assert_ne!(
        other, scene,
        "a scene differs from the scene of another sound alone",
    );
    assert_eq!(
        Scene::parse(SCENE, ".", "the same audio scene"),
        scene,
        "a scene equals the scene of its own sounds",
    );
}
fn edit<T>(
    scene: &mut dome_scene::Scene,
    change: impl FnOnce(&mut dome_scene::Edit<'_>) -> Result<T, String>,
) -> Result<T, String> {
    scene.edit(change).map(|(value, _)| value)
}

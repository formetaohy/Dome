use dome_math::Vec2;
use dome_ui::{Clipboard, Edit, EditOutcome, Input, ScrollDelta};
use dome_window::{Event as WindowEvent, KeyCode, ModifiersState};

struct Held(String);

impl Clipboard for Held {
    fn read(&mut self) -> Option<String> {
        Some(self.0.clone())
    }

    fn write(&mut self, text: &str) {
        self.0 = String::from(text);
    }
}

fn press(input: &mut Input, code: KeyCode, character: Option<char>, text: Option<&str>) {
    input.window(
        &WindowEvent::Key {
            code,
            character,
            pressed: true,
            repeat: false,
            text: text.map(String::from),
        },
        1.0,
    );
}

fn begin(input: &mut Input, scale: f32) {
    input.begin(Vec2::new(1280.0, 720.0), scale, 1.0);
}

#[test]
fn key_events_reach_the_edit_they_type_into() {
    let mut clipboard = Held(String::from("pasted"));

    let mut input = Input::new();
    press(&mut input, KeyCode::Enter, Some('\r'), Some("\r"));
    begin(&mut input, 1.0);
    let mut edit = Edit::new(1, "door", 0.0);
    assert_eq!(
        edit.poll(&input, &mut clipboard, 0.1),
        Some(EditOutcome::Commit)
    );
    assert_eq!(edit.text(), "door");

    let mut input = Input::new();
    press(&mut input, KeyCode::Tab, Some('\t'), Some("\t"));
    begin(&mut input, 1.0);
    let mut edit = Edit::new(2, "door", 0.0);
    assert_eq!(
        edit.poll(&input, &mut clipboard, 0.1),
        Some(EditOutcome::Advance)
    );
    assert_eq!(edit.text(), "door");

    let mut input = Input::new();
    input.window(&WindowEvent::Modifiers(ModifiersState::CONTROL), 1.0);
    press(&mut input, KeyCode::KeyV, Some('v'), None);
    begin(&mut input, 1.0);
    let mut edit = Edit::new(3, "", 0.0);
    assert_eq!(edit.poll(&input, &mut clipboard, 0.1), None);
    assert_eq!(edit.text(), "pasted");

    let edit = Edit::new(4, "alpha beta ", 0.0);
    assert_eq!(
        edit.word(11),
        (11, 11),
        "a caret at the end of trailing space selects no blank word",
    );
    assert_eq!(
        edit.word(10),
        (10, 10),
        "a caret inside trailing space selects no blank word",
    );
    assert_eq!(
        edit.word(2),
        (0, 5),
        "a caret inside a word selects that whole word",
    );
}

#[test]
fn a_wheel_up_moves_the_content_up_and_keeps_its_horizontal_part() {
    let mut input = Input::new();
    input.window(
        &WindowEvent::Wheel {
            lines: [1.5, 2.0],
            pixels: [0.0; 2],
        },
        1.0,
    );
    begin(&mut input, 1.0);
    assert_eq!(
        input.scroll(),
        ScrollDelta {
            down: -80.0,
            right: 60.0,
        },
        "a wheel up scrolls towards earlier content and a wheel right towards later columns",
    );

    input.window(
        &WindowEvent::Wheel {
            lines: [0.0; 2],
            pixels: [40.0, 80.0],
        },
        2.0,
    );
    begin(&mut input, 2.0);
    assert_eq!(
        input.scroll(),
        ScrollDelta {
            down: -40.0,
            right: 20.0,
        },
        "a pixel scroll converts to the logical content offset it claims",
    );
}

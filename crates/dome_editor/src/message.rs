use dome_scene::{Target, Value};

use crate::gizmo::Mode;
use crate::panel::Pane;

#[derive(Clone, Debug)]
pub enum Message {
    Save,
    Reload,
    Undo,
    Redo,
    Play,
    Stop,
    Mode(Mode),
    ToggleSpace,
    Focus,
    Select(Target),
    AddNode(&'static str),
    AddAsset(&'static str),
    Remove,
    AddComponent(Target, String),
    RemoveComponent(Target, String),
    Property {
        target: Target,
        path: String,
        value: Value,
    },
    ImportPath(String),
    Import,
    PrefabPath(String),
    Instance,
    Pane(Pane),
    Debug(bool),
    Expand(String),
    Collapse(String),
}

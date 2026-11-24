use dome_scene::{Folder, Target, Value};

use crate::gizmo::Mode;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    Mesh,
    Camera,
    Sun,
    PointLight,
    Probe,
    Empty,
}

impl NodeKind {
    pub fn base_id(self) -> &'static str {
        match self {
            Self::Mesh => "mesh",
            Self::Camera => "camera",
            Self::Sun => "sun",
            Self::PointLight => "lamp",
            Self::Probe => "probe",
            Self::Empty => "node",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AssetKind {
    Mesh,
    Material,
    Atmosphere,
    Graph,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathKind {
    Model,
    Prefab,
    Image,
    Sound,
    Panorama,
}

impl PathKind {
    pub fn folder(self) -> Folder {
        match self {
            Self::Model | Self::Image | Self::Sound | Self::Panorama => Folder::Asset,
            Self::Prefab => Folder::Prefab,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PathDialog {
    pub kind: PathKind,
    pub input: String,
    pub focus: bool,
    pub invalid: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tab {
    #[default]
    Scene,
    Asset,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sheet {
    Outliner,
    Inspector,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParameterKind {
    Number,
    Boolean,
}

#[derive(Clone, Debug)]
pub enum Action {
    Save,
    Overwrite,
    Reload,
    Undo,
    Redo,
    Play,
    Stop,
    Mode(Mode),
    ToggleSpace,
    Focus,
    ToggleDebug,
    ToggleSheet(Sheet),
    SelectTab(Tab),
    Select(Target),
    CreateNode(NodeKind),
    CreateAsset(AssetKind),
    OpenDialog(PathKind),
    EditDialog(String),
    SubmitPath(String),
    SubmitDialog,
    CloseDialog,
    DismissError,
    RemoveTarget(Target),
    RemoveSelected,
    ToggleHidden(String),
    UnpackPrefab(String),
    AddComponent(Target, &'static str),
    RemoveComponent(Target, &'static str),
    ToggleSection(String),
    ToggleBranch(String),
    Property {
        target: Target,
        path: String,
        value: Value,
    },
    AddGraphState(String),
    RemoveGraphState(String, String),
    AddGraphParameter(String, ParameterKind),
    RemoveGraphParameter(String, String),
    AddGraphTransition(String),
    RemoveGraphTransition(String, usize),
    AddGraphCondition(String, usize),
    RemoveGraphCondition(String, usize, usize),
}

use std::collections::HashSet;

use dome_math::{Rect, Transform, Vec2, Vec3};
use dome_scene::{ReferenceKind, Scene, Target};
use dome_ui::{
    Align, Axis, BuildContext, Corners, Edges, Element, Flow, Key, Layer, Layout, Length, Motion,
    Role, Size, State as UiState, TextStyle, place,
};

use crate::action::{Action, Confirm, PathDialog, Tab};
use crate::gizmo::{self, Grip, Mode, Space};

const MARGIN: f32 = 12.0;
const BAR_HEIGHT: f32 = 44.0;
const SHEET_TOP: f32 = 68.0;
const OUTLINER_WIDTH: f32 = 264.0;
const INSPECTOR_WIDTH: f32 = 308.0;
const OUTLINER_SPRING: u64 = 0x6f75_746c_696e_6572;
const INSPECTOR_SPRING: u64 = 0x696e_7370_6563_7472;

pub(crate) struct State<'a> {
    pub(crate) scene: &'a Scene,
    pub(crate) selected: Option<&'a Target>,
    pub(crate) dirty: bool,
    pub(crate) conflict: bool,
    pub(crate) playing: bool,
    pub(crate) can_undo: bool,
    pub(crate) can_redo: bool,
    pub(crate) mode: Mode,
    pub(crate) space: Space,
    pub(crate) debug: bool,
    pub(crate) outliner_open: bool,
    pub(crate) inspector_open: bool,
    pub(crate) tab: Tab,
    pub(crate) collapsed_branches: &'a HashSet<String>,
    pub(crate) collapsed_sections: &'a HashSet<String>,
    pub(crate) dialog: Option<&'a PathDialog>,
    pub(crate) confirm: Option<Confirm>,
    pub(crate) can_save: bool,
    pub(crate) message: Option<&'a crate::message::Message>,
    pub(crate) camera: Transform,
    pub(crate) origin: Vec3,
    pub(crate) basis: [Vec3; 3],
    pub(crate) hover: Option<Grip>,
    pub(crate) active: Option<Grip>,
}

pub(crate) fn build(
    ctx: &mut BuildContext<'_, Action>,
    state: &State<'_>,
) -> Element<'static, Action> {
    let screen = ctx.size();
    let full = Rect::from_min_size(Vec2::ZERO, screen);
    let sheet_height = (screen.y - SHEET_TOP - MARGIN).max(120.0);

    let outliner_t = ctx.animate(
        OUTLINER_SPRING,
        if state.outliner_open { 1.0 } else { 0.0 },
        Motion::spatial_default(),
    );
    let inspector_shown = state.inspector_open && state.selected.is_some();
    let inspector_t = ctx.animate(
        INSPECTOR_SPRING,
        if inspector_shown { 1.0 } else { 0.0 },
        Motion::spatial_default(),
    );

    let mut layers = vec![
        viewport(state, full),
        place(
            Rect::from_min_size(Vec2::new(0.0, MARGIN), Vec2::new(screen.x, BAR_HEIGHT)),
            [Element::row()
                .width(Length::Fill)
                .height(Length::Fill)
                .justify(Align::Center)
                .align(Align::Center)
                .children([crate::toolbar::build(ctx, state)])],
        )
        .key(Key::new("stage.top"))
        .layer(Layer::Chrome),
    ];

    if outliner_t > 0.005 {
        let travel = OUTLINER_WIDTH + MARGIN + 8.0;
        let x = MARGIN - (1.0 - outliner_t) * travel;
        let rect = Rect::from_min_size(
            Vec2::new(x, SHEET_TOP),
            Vec2::new(OUTLINER_WIDTH, sheet_height),
        );
        let head = crate::outliner::header(ctx, state);
        let content = crate::outliner::body(ctx, state);
        layers.push(sheet(ctx.theme(), "outliner", rect, head, content));
    }

    if inspector_t > 0.005
        && let Some(target) = state.selected
    {
        let travel = INSPECTOR_WIDTH + MARGIN + 8.0;
        let x = screen.x - MARGIN - INSPECTOR_WIDTH + (1.0 - inspector_t) * travel;
        let rect = Rect::from_min_size(
            Vec2::new(x, SHEET_TOP),
            Vec2::new(INSPECTOR_WIDTH, sheet_height),
        );
        let head = crate::inspector::header(ctx, state, target);
        let content = crate::inspector::body(ctx, state, target);
        layers.push(sheet(ctx.theme(), "inspector", rect, head, content));
    }

    if let Some(dialog) = state.dialog {
        layers.push(crate::dialog::modal(ctx, screen, dialog));
    }

    if let Some(confirm) = state.confirm {
        layers.push(crate::dialog::confirm(ctx, screen, confirm, state.can_save));
    }

    if let Some(message) = state.message {
        layers.push(crate::dialog::toast(ctx, screen, message));
    }

    place(full, layers)
        .key(Key::new("stage"))
        .layer(Layer::Content)
}

fn sheet(
    theme: &dome_ui::Theme,
    name: &str,
    rect: Rect,
    header: Element<'static, Action>,
    body: Element<'static, Action>,
) -> Element<'static, Action> {
    place(
        rect,
        [Element::column()
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Edges::all(10.0))
            .gap(10.0)
            .clipped()
            .background(theme.surface(Role::Sheet, UiState::Rest))
            .children([
                header,
                Element::column()
                    .key(Key::new(format!("{name}.scroll")))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .scroll(Axis::Column, Length::Fill, None)
                    .clipped()
                    .children([body]),
            ])],
    )
    .key(Key::new(name))
    .layer(Layer::Chrome)
}

fn viewport(state: &State<'_>, rect: Rect) -> Element<'static, Action> {
    let node_selected = matches!(state.selected, Some(Target::Node(_)));
    let origin = state.origin;
    let basis = state.basis;
    let mode = state.mode;
    let hover = state.hover;
    let active = state.active;
    let camera = state.camera;
    let signature = viewport_signature(state, node_selected);
    Element::new(Layout {
        flow: Flow::Column,
        size: Size::new(Length::Fill, Length::Fill),
        ..Layout::column()
    })
    .at(rect)
    .key(Key::new("viewport"))
    .layer(Layer::Content)
    .clipped()
    .view(signature, move |_, painter, info| {
        painter.scene(info.rect, Corners::ZERO, [0.0, 0.0, 1.0, 1.0]);
        if !node_selected {
            return;
        }
        let target = gizmo::Target {
            camera: crate::camera::editor(),
            view: camera,
            viewport: info.rect,
        };
        let handles = target.handles(origin, basis, mode);
        gizmo::paint(&handles, hover, active, painter);
    })
}

fn viewport_signature(state: &State<'_>, node_selected: bool) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut feed = |value: u64| {
        hash = (hash ^ value).wrapping_mul(0x0000_0100_0000_01b3);
    };
    feed(node_selected as u64);
    feed(state.origin.x.to_bits() as u64);
    feed(state.origin.y.to_bits() as u64);
    feed(state.origin.z.to_bits() as u64);
    for axis in state.basis {
        feed(axis.x.to_bits() as u64);
        feed(axis.y.to_bits() as u64);
        feed(axis.z.to_bits() as u64);
    }
    feed(state.mode as u64);
    feed(state.hover.map_or(0, grip_code) as u64);
    feed(state.active.map_or(0, grip_code) as u64);
    for value in [
        state.camera.translation.x,
        state.camera.translation.y,
        state.camera.translation.z,
        state.camera.rotation.x,
        state.camera.rotation.y,
        state.camera.rotation.z,
        state.camera.rotation.w,
    ] {
        feed(value.to_bits() as u64);
    }
    hash
}

fn grip_code(grip: Grip) -> u8 {
    match grip {
        Grip::Axis(gizmo::Axis::X) => 1,
        Grip::Axis(gizmo::Axis::Y) => 2,
        Grip::Axis(gizmo::Axis::Z) => 3,
        Grip::Uniform => 4,
    }
}

pub(crate) fn label(value: impl Into<String>, style: TextStyle) -> Element<'static, Action> {
    dome_ui::text(value.into(), style)
}

pub(crate) fn texture_of(target: &Target) -> Option<String> {
    match target {
        Target::Image(id) => Some(id.clone()),
        Target::Environment(id) => Some(format!("environment.{id}")),
        _ => None,
    }
}

pub(crate) fn references(scene: &Scene, kind: ReferenceKind) -> Vec<String> {
    let mut ids: Vec<String> = match kind {
        ReferenceKind::Node => scene.nodes().iter().map(|node| node.id.clone()).collect(),
        ReferenceKind::Graph => scene
            .graphs()
            .iter()
            .map(|entry| entry.id.clone())
            .collect(),
        ReferenceKind::AudioBus => scene
            .nodes()
            .iter()
            .filter(|node| node.has_component("audio_bus"))
            .map(|node| node.id.clone())
            .collect(),
        other => scene
            .asset_ids(other)
            .into_iter()
            .map(String::from)
            .collect(),
    };
    ids.sort();
    ids
}

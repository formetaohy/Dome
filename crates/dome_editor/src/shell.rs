use std::collections::HashSet;
use std::path::Path;

use dome_math::{Transform, Vec3};
use dome_scene::{ReferenceKind, Scene, Target};
use dome_ui::{
    Align, BuildContext, Corners, Edges, Element, Flow, Icon, Key, Layer, Length, Role, TextStyle,
};

use crate::gizmo::{self, Grip, Mode, Space};
use crate::message::Message;
use crate::panel::{Pane, Panels};

pub(crate) struct View<'a> {
    pub(crate) scene: &'a Scene,
    pub(crate) selected: Option<&'a Target>,
    pub(crate) path: &'a Path,
    pub(crate) status: &'a str,
    pub(crate) dirty: bool,
    pub(crate) playing: bool,
    pub(crate) mode: Mode,
    pub(crate) space: Space,
    pub(crate) panels: Panels,
    pub(crate) debug: bool,
    pub(crate) import_path: &'a str,
    pub(crate) prefab_path: &'a str,
    pub(crate) collapsed: &'a HashSet<String>,
    pub(crate) camera: Transform,
    pub(crate) origin: Vec3,
    pub(crate) basis: [Vec3; 3],
    pub(crate) hover: Option<Grip>,
    pub(crate) active: Option<Grip>,
    pub(crate) error: Option<&'a str>,
}

pub(crate) fn shell<'a>(ctx: &BuildContext<'_, Message>, view: &View<'a>) -> Element<'a, Message> {
    let theme = ctx.theme();
    let mut children = vec![toolbar(ctx, view)];
    let mut body = Vec::new();
    if let Some(pane) = view.panels.left {
        body.push(panel(ctx, view, pane));
    }
    body.push(viewport(view));
    if let Some(pane) = view.panels.right {
        body.push(panel(ctx, view, pane));
    }
    children.push(
        Element::row()
            .width(Length::Fill)
            .height(Length::Fill)
            .gap(theme.metrics.gap)
            .children(body),
    );
    children.push(status(ctx, view));
    Element::column()
        .key(Key::new("editor"))
        .layer(Layer::Content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Edges::all(8.0))
        .gap(theme.metrics.gap)
        .background(theme.surface(Role::Sheet, dome_ui::State::Rest))
        .children(children)
}

fn toolbar<'a>(ctx: &BuildContext<'_, Message>, view: &View<'a>) -> Element<'a, Message> {
    let theme = ctx.theme();
    let palette = theme.palette;
    let modes = ["Move", "Rotate", "Scale"];
    let index = match view.mode {
        Mode::Translate => 0,
        Mode::Rotate => 1,
        Mode::Scale => 2,
    };
    let modes = dome_ui::widget::segmented(theme, &modes, index).on_segment(|at| {
        Message::Mode(match at {
            0 => Mode::Translate,
            1 => Mode::Rotate,
            _ => Mode::Scale,
        })
    });
    let space = dome_ui::widget::button(theme, view.space.name(), Role::Standard)
        .press(Message::ToggleSpace);
    let undo =
        dome_ui::widget::icon_button(Icon::Undo, 32.0, Role::Standard, "Undo").press(Message::Undo);
    let redo =
        dome_ui::widget::icon_button(Icon::Redo, 32.0, Role::Standard, "Redo").press(Message::Redo);
    let save = dome_ui::widget::action_button(theme, Icon::Save, "Save", Role::Filled)
        .press(Message::Save);
    let run = if view.playing {
        dome_ui::widget::action_button(theme, Icon::Pause, "Stop", Role::Destructive)
            .press(Message::Stop)
    } else {
        dome_ui::widget::action_button(theme, Icon::Play, "Play", Role::Tonal).press(Message::Play)
    };
    let scene = Element::row().gap(6.0).align(Align::Center).children([
        dome_ui::widget::icon(
            Icon::Cube,
            18.0,
            palette.on_surface_variant,
            Default::default(),
        ),
        text(
            view.path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("scene"),
            TextStyle::new(theme.font(dome_ui::Level::Title), palette.on_surface),
        ),
    ]);
    let mut panels = Vec::new();
    for pane in [Pane::Scene, Pane::Library, Pane::Inspector, Pane::Settings] {
        let icon = match pane {
            Pane::Scene => Icon::Layers,
            Pane::Library => Icon::Folder,
            Pane::Inspector => Icon::Sliders,
            Pane::Settings => Icon::Grid,
        };
        let role = if view.panels.open(pane) {
            Role::PrimaryContainer
        } else {
            Role::Standard
        };
        panels.push(
            dome_ui::widget::icon_button(icon, 32.0, role, pane.name()).press(Message::Pane(pane)),
        );
    }
    let mut lead = vec![modes, space, undo, redo, scene];
    if view.dirty {
        lead.push(dome_ui::widget::badge("*", palette.primary));
    }
    lead.push(Element::row().width(Length::Fill));
    lead.push(save);
    lead.push(run);
    lead.extend(panels);
    Element::row()
        .key(Key::new("toolbar"))
        .layer(Layer::Chrome)
        .width(Length::Fill)
        .height(Length::Fixed(48.0))
        .padding(Edges::symmetric(10.0, 6.0))
        .gap(8.0)
        .align(Align::Center)
        .background(theme.surface(Role::FloatingToolbar, dome_ui::State::Rest))
        .children(lead)
}

fn viewport<'a>(view: &View<'a>) -> Element<'a, Message> {
    let selected = view.selected;
    let origin = view.origin;
    let basis = view.basis;
    let mode = view.mode;
    let hover = view.hover;
    let active = view.active;
    let camera = view.camera;
    let signature = viewport_signature(view);
    Element::new(dome_ui::Layout {
        flow: Flow::Column,
        size: dome_ui::Size::new(Length::Fill, Length::Fill),
        ..dome_ui::Layout::column()
    })
    .key(Key::new("viewport"))
    .layer(Layer::Content)
    .clipped()
    .view(signature, move |_, painter, info| {
        painter.scene(info.rect, Corners::ZERO, [0.0, 0.0, 1.0, 1.0]);
        let Some(_) = selected else {
            return;
        };
        let target = gizmo::Target {
            camera: crate::camera::editor(),
            view: camera,
            viewport: info.rect,
        };
        let handles = target.handles(origin, basis, mode);
        gizmo::paint(&handles, hover, active, painter);
    })
}

fn status<'a>(ctx: &BuildContext<'_, Message>, view: &View<'a>) -> Element<'a, Message> {
    let theme = ctx.theme();
    let palette = theme.palette;
    let style = TextStyle::new(
        theme.font(dome_ui::Level::Caption),
        palette.on_surface_variant,
    );
    let message = if let Some(error) = view.error {
        text(
            error,
            TextStyle {
                color: palette.error,
                ..style
            },
        )
    } else {
        text(view.status, style)
    };
    Element::row()
        .key(Key::new("status"))
        .layer(Layer::Chrome)
        .width(Length::Fill)
        .height(Length::Fixed(22.0))
        .padding(Edges::symmetric(12.0, 0.0))
        .gap(8.0)
        .align(Align::Center)
        .background(theme.surface(Role::Sheet, dome_ui::State::Rest))
        .children([message])
}

fn panel<'a>(ctx: &BuildContext<'_, Message>, view: &View<'a>, pane: Pane) -> Element<'a, Message> {
    let theme = ctx.theme();
    let content = match pane {
        Pane::Scene => crate::hierarchy::panel(ctx, view),
        Pane::Library => crate::library::panel(ctx, view),
        Pane::Inspector => crate::inspector::panel(ctx, view),
        Pane::Settings => crate::settings::panel(ctx, view),
    };
    Element::column()
        .key(Key::new(pane.name()))
        .layer(Layer::Chrome)
        .width(Length::Fixed(pane.width()))
        .height(Length::Fill)
        .padding(Edges::all(12.0))
        .clipped()
        .background(theme.surface(Role::Card, dome_ui::State::Rest))
        .children([Element::column()
            .key(Key::new(format!("{}.body", pane.name())))
            .width(Length::Fill)
            .height(Length::Fill)
            .gap(10.0)
            .scroll(dome_ui::Axis::Column, Length::Fill, None)
            .children([content])])
}

fn viewport_signature(view: &View<'_>) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut feed = |value: u64| {
        hash = (hash ^ value).wrapping_mul(0x0000_0100_0000_01b3);
    };
    feed(view.origin.x.to_bits() as u64);
    feed(view.origin.y.to_bits() as u64);
    feed(view.origin.z.to_bits() as u64);
    for axis in view.basis {
        feed(axis.x.to_bits() as u64);
        feed(axis.y.to_bits() as u64);
        feed(axis.z.to_bits() as u64);
    }
    feed(view.mode as u64);
    feed(view.hover.map_or(0, grip_code) as u64);
    feed(view.active.map_or(0, grip_code) as u64);
    feed(view.selected.is_some() as u64);
    for value in [
        view.camera.translation.x,
        view.camera.translation.y,
        view.camera.translation.z,
        view.camera.rotation.x,
        view.camera.rotation.y,
        view.camera.rotation.z,
        view.camera.rotation.w,
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

pub(crate) fn texture_of(target: &Target) -> Option<String> {
    match target {
        Target::Image(id) => Some(id.clone()),
        Target::Environment(id) => Some(format!("environment.{id}")),
        _ => None,
    }
}

pub(crate) fn text(value: &str, style: TextStyle) -> Element<'static, Message> {
    dome_ui::text(String::from(value), style)
}

pub(crate) fn section<'a>(
    ctx: &BuildContext<'_, Message>,
    title: &str,
    children: impl IntoIterator<Item = Element<'a, Message>>,
) -> Element<'a, Message> {
    let theme = ctx.theme();
    Element::column()
        .width(Length::Fill)
        .gap(6.0)
        .children([dome_ui::widget::section_header(theme, title, None)])
        .children(children)
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

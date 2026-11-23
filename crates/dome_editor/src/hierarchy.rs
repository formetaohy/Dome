use dome_scene::{Scene, Target};
use dome_ui::{
    Align, BuildContext, Edges, Element, Icon, Key, Length, Role, State, TextStyle, widget,
};

use crate::message::Message;
use crate::shell::{View, section, text};

pub(crate) fn panel<'a>(ctx: &BuildContext<'_, Message>, view: &View<'a>) -> Element<'a, Message> {
    let theme = ctx.theme();
    let mut children = vec![header(view)];
    let roots = view
        .scene
        .nodes()
        .iter()
        .enumerate()
        .filter(|(index, _)| view.scene.parent_index(*index).is_none())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if roots.is_empty() {
        children.push(widget::empty_state(theme, Icon::Layers, "No nodes"));
    }
    for index in roots {
        children.push(node(ctx, view, index, 0));
    }
    section(ctx, "Scene", children)
}

fn header<'a>(view: &View<'a>) -> Element<'a, Message> {
    let kinds = [
        (Icon::Cube, "mesh", "Mesh"),
        (Icon::Camera, "camera", "Camera"),
        (Icon::Sparkles, "sun", "Sun"),
        (Icon::Globe, "probe", "Environment probe"),
        (Icon::Plus, "empty", "Empty node"),
    ];
    let mut buttons = Vec::new();
    for (icon, kind, tooltip) in kinds {
        buttons.push(
            widget::icon_button(icon, 28.0, Role::Standard, tooltip).press(Message::AddNode(kind)),
        );
    }
    if let Some(Target::Node(id)) = view.selected {
        let removable = view.scene.node_index(id).is_some();
        buttons.push(Element::row().width(Length::Fill));
        buttons.push(
            widget::icon_button(Icon::Trash, 28.0, Role::Standard, "Remove node")
                .press(Message::Remove)
                .enabled(removable),
        );
    }
    Element::row()
        .width(Length::Fill)
        .height(Length::Fixed(28.0))
        .gap(4.0)
        .align(Align::Center)
        .children(buttons)
}

fn node<'a>(
    ctx: &BuildContext<'_, Message>,
    view: &View<'a>,
    index: usize,
    depth: usize,
) -> Element<'a, Message> {
    let theme = ctx.theme();
    let palette = theme.palette;
    let scene: &Scene = view.scene;
    let id = scene.nodes()[index].id.clone();
    let selected = view
        .selected
        .is_some_and(|target| matches!(target, Target::Node(held) if *held == id));
    let children = scene.children(index).collect::<Vec<_>>();
    let collapsed = view.collapsed.contains(&id);
    let mut row = Vec::new();
    if children.is_empty() {
        row.push(Element::row().width(Length::Fixed(16.0)));
    } else {
        let icon = if collapsed {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        };
        let toggle = id.clone();
        row.push(
            widget::icon(icon, 16.0, palette.on_surface_variant, Default::default())
                .hit_target()
                .on_pointer(move |pointer| {
                    matches!(pointer.phase, dome_ui::Phase::Down).then(|| {
                        if collapsed {
                            Message::Expand(toggle.clone())
                        } else {
                            Message::Collapse(toggle.clone())
                        }
                    })
                }),
        );
    }
    let held = &scene.nodes()[index];
    let symbol = if held.has_component("mesh") {
        Icon::Cube
    } else if held.has_component("camera") {
        Icon::Camera
    } else if held.has_component("directional_light") || held.has_component("point_light") {
        Icon::Sparkles
    } else if held.has_component("environment") {
        Icon::Globe
    } else {
        Icon::Section
    };
    row.push(widget::icon(
        symbol,
        16.0,
        if selected {
            palette.on_primary_container
        } else {
            palette.on_surface_variant
        },
        Default::default(),
    ));
    row.push(
        text(
            &id,
            TextStyle::new(
                theme.font(dome_ui::Level::Body),
                if selected {
                    palette.on_primary_container
                } else {
                    palette.on_surface
                },
            ),
        )
        .width(Length::Fill),
    );
    let background = if selected {
        theme.surface(Role::PrimaryContainer, State::Selected)
    } else {
        dome_ui::Surface::NONE
    };
    let mut column = vec![
        Element::row()
            .key(Key::new(format!("node.{id}")))
            .width(Length::Fill)
            .height(Length::Fixed(28.0))
            .padding(Edges {
                left: 6.0 + depth as f32 * 12.0,
                right: 8.0,
                top: 0.0,
                bottom: 0.0,
            })
            .gap(6.0)
            .align(Align::Center)
            .background(background)
            .hover_highlight(theme.surface(Role::Standard, State::Hover))
            .children(row)
            .press(Message::Select(Target::Node(id.clone()))),
    ];
    if !collapsed {
        for child in children {
            column.push(node(ctx, view, child, depth + 1));
        }
    }
    Element::column()
        .width(Length::Fill)
        .gap(1.0)
        .children(column)
}

use dome_scene::{EnvironmentSource, Node, Scene, Target};
use dome_ui::{
    Align, BuildContext, Corners, Edges, Element, Fill, Icon, IconStyle, Key, Length, Level, Role,
    State as UiState, Surface, TextStyle, popup, widget,
};

use crate::action::{Action, AssetKind, NodeKind, PathKind, Tab};
use crate::stage::{State, label, texture_of};

pub(crate) fn header(
    ctx: &BuildContext<'_, Action>,
    state: &State<'_>,
) -> Element<'static, Action> {
    let theme = ctx.theme();
    let tab_index = match state.tab {
        Tab::Scene => 0,
        Tab::Asset => 1,
    };
    let switcher = widget::tabs(theme, &["Nodes", "Assets"], tab_index)
        .key(Key::new("outliner.tabs"))
        .width(Length::Fill)
        .on_segment(|at| {
            Action::SelectTab(match at {
                0 => Tab::Scene,
                _ => Tab::Asset,
            })
        });

    let add_key = Key::new("outliner.add");
    let open = ctx.popup_open(add_key);
    let add_button = widget::icon_button(
        if open { Icon::Close } else { Icon::Plus },
        32.0,
        Role::PrimaryContainer,
        "Create",
    )
    .key(add_key)
    .shaped(if open {
        Corners::all(10.0)
    } else {
        Corners::capsule()
    })
    .hit_target();

    let menu = match state.tab {
        Tab::Scene => popup(
            add_key,
            204.0,
            [
                menu_action(
                    theme,
                    "add.node.mesh",
                    Icon::Cube,
                    "Mesh",
                    Action::CreateNode(NodeKind::Mesh),
                ),
                menu_action(
                    theme,
                    "add.node.sun",
                    Icon::Sparkles,
                    "Directional Light",
                    Action::CreateNode(NodeKind::Sun),
                ),
                menu_action(
                    theme,
                    "add.node.lamp",
                    Icon::Record,
                    "Point Light",
                    Action::CreateNode(NodeKind::PointLight),
                ),
                menu_action(
                    theme,
                    "add.node.camera",
                    Icon::Camera,
                    "Camera",
                    Action::CreateNode(NodeKind::Camera),
                ),
                menu_action(
                    theme,
                    "add.node.probe",
                    Icon::Globe,
                    "Environment Probe",
                    Action::CreateNode(NodeKind::Probe),
                ),
                menu_action(
                    theme,
                    "add.node.empty",
                    Icon::Layers,
                    "Empty",
                    Action::CreateNode(NodeKind::Empty),
                ),
                menu_action(
                    theme,
                    "add.node.prefab",
                    Icon::Folder,
                    "Prefab…",
                    Action::OpenDialog(PathKind::Prefab),
                ),
                menu_action(
                    theme,
                    "add.node.model",
                    Icon::Export,
                    "Model…",
                    Action::OpenDialog(PathKind::Model),
                ),
            ],
        )
        .key(Key::new("outliner.add.scene_menu")),
        Tab::Asset => {
            let mut items = vec![
                menu_action(
                    theme,
                    "add.asset.mesh",
                    Icon::Cube,
                    "Mesh",
                    Action::CreateAsset(AssetKind::Mesh),
                ),
                menu_action(
                    theme,
                    "add.asset.material",
                    Icon::Sliders,
                    "Material",
                    Action::CreateAsset(AssetKind::Material),
                ),
                menu_action(
                    theme,
                    "add.asset.atmosphere",
                    Icon::Sparkles,
                    "Sky Atmosphere",
                    Action::CreateAsset(AssetKind::Atmosphere),
                ),
                menu_action(
                    theme,
                    "add.asset.panorama",
                    Icon::Globe,
                    "Panorama…",
                    Action::OpenDialog(PathKind::Panorama),
                ),
                menu_action(
                    theme,
                    "add.asset.image",
                    Icon::Camera,
                    "Image…",
                    Action::OpenDialog(PathKind::Image),
                ),
                menu_action(
                    theme,
                    "add.asset.sound",
                    Icon::Channel,
                    "Sound…",
                    Action::OpenDialog(PathKind::Sound),
                ),
            ];
            if !state.scene.animations().is_empty() {
                items.push(menu_action(
                    theme,
                    "add.asset.graph",
                    Icon::Chart,
                    "Animation Graph",
                    Action::CreateAsset(AssetKind::Graph),
                ));
            }
            popup(add_key, 204.0, items).key(Key::new("outliner.add.asset_menu"))
        }
    };

    Element::row()
        .width(Length::Fill)
        .gap(6.0)
        .align(Align::Center)
        .children([switcher, add_button, menu])
}

pub(crate) fn body(ctx: &BuildContext<'_, Action>, state: &State<'_>) -> Element<'static, Action> {
    match state.tab {
        Tab::Scene => nodes_tree(ctx, state),
        Tab::Asset => assets_list(ctx, state),
    }
}

struct VisibleNode {
    index: usize,
    depth: usize,
    has_children: bool,
    collapsed: bool,
}

fn collect_visible(
    scene: &Scene,
    collapsed: &std::collections::HashSet<String>,
) -> Vec<VisibleNode> {
    let mut list = Vec::new();
    for root in scene.roots() {
        walk_visible(scene, collapsed, root, 0, &mut list);
    }
    list
}

fn walk_visible(
    scene: &Scene,
    collapsed: &std::collections::HashSet<String>,
    index: usize,
    depth: usize,
    out: &mut Vec<VisibleNode>,
) {
    let children: Vec<usize> = scene.children(index).collect();
    let id = &scene.nodes()[index].id;
    let is_collapsed = collapsed.contains(id);
    out.push(VisibleNode {
        index,
        depth,
        has_children: !children.is_empty(),
        collapsed: is_collapsed,
    });
    if !is_collapsed {
        for child in children {
            walk_visible(scene, collapsed, child, depth + 1, out);
        }
    }
}

fn nodes_tree(ctx: &BuildContext<'_, Action>, state: &State<'_>) -> Element<'static, Action> {
    let theme = ctx.theme();
    let palette = theme.palette;
    let metrics = theme.metrics;
    let visible = collect_visible(state.scene, state.collapsed_branches);
    if visible.is_empty() {
        return empty_icon(theme, Icon::Layers);
    }
    let count = visible.len();
    let mut rows = Vec::with_capacity(count);
    for (slot, entry) in visible.into_iter().enumerate() {
        let node = &state.scene.nodes()[entry.index];
        let id = node.id.clone();
        let target = Target::Node(id.clone());
        let selected = state.selected == Some(&target);
        let hidden = node.has_component("hidden");
        let is_prefab = node.prefab().is_some();

        let corners = Corners::group_vertical(slot, count, 14.0, metrics.corner_xs, selected);
        let base = if selected {
            theme
                .surface(Role::PrimaryContainer, UiState::Selected)
                .with_corners(corners)
        } else {
            Surface::fill(palette.surface_container).with_corners(corners)
        };
        let hover = Surface {
            fill: Fill::Solid(if selected {
                palette
                    .primary_container
                    .lerp(palette.on_primary_container, 0.14)
            } else {
                palette.surface_container_high
            }),
            ..base
        };

        let mut cells = Vec::with_capacity(4);
        if entry.has_children {
            let branch_id = id.clone();
            let chevron = if entry.collapsed {
                Icon::ChevronRight
            } else {
                Icon::ChevronDown
            };
            cells.push(
                widget::icon_button(chevron, 22.0, Role::Standard, "")
                    .key(Key::new(format!("branch.{id}")))
                    .press(Action::ToggleBranch(branch_id)),
            );
        } else {
            cells.push(Element::row().width(Length::Fixed(22.0)));
        }

        let symbol = node_icon(node);
        let icon_color = if selected {
            palette.on_primary_container
        } else if hidden {
            palette.outline
        } else if is_prefab {
            palette.tertiary
        } else {
            palette.on_surface_variant
        };
        let text_color = if selected {
            palette.on_primary_container
        } else if hidden {
            palette.outline
        } else {
            palette.on_surface
        };

        cells.push(widget::icon(
            symbol,
            16.0,
            icon_color,
            if selected {
                IconStyle::FILLED
            } else {
                IconStyle::REGULAR
            },
        ));
        cells.push(
            label(
                &id,
                TextStyle::new(theme.font(Level::Body), text_color).truncating(),
            )
            .width(Length::Fill),
        );

        if !is_prefab && (hidden || selected) {
            let eye_id = id.clone();
            cells.push(
                widget::icon_button(
                    if hidden { Icon::EyeOff } else { Icon::Eye },
                    24.0,
                    Role::Standard,
                    "Visibility",
                )
                .key(Key::new(format!("eye.{id}")))
                .press(Action::ToggleHidden(eye_id)),
            );
        }

        rows.push(
            Element::row()
                .key(Key::new(format!("node.{id}")))
                .width(Length::Fill)
                .height(Length::Fixed(34.0))
                .padding(Edges {
                    left: 6.0 + entry.depth as f32 * 14.0,
                    right: 6.0,
                    top: 0.0,
                    bottom: 0.0,
                })
                .gap(6.0)
                .align(Align::Center)
                .background(base)
                .hover_highlight(hover)
                .children(cells)
                .press(Action::Select(target)),
        );
    }

    Element::column()
        .width(Length::Fill)
        .gap(metrics.group_gap)
        .children(rows)
}

pub(crate) fn node_icon(node: &Node) -> Icon {
    if node.prefab().is_some() {
        Icon::Folder
    } else if node.has_component("camera") {
        Icon::Camera
    } else if node.has_component("directional_light") {
        Icon::Sparkles
    } else if node.has_component("point_light") {
        Icon::Record
    } else if node.has_component("environment") {
        Icon::Globe
    } else if node.has_component("audio_bus") || node.has_component("audio_source") {
        Icon::Channel
    } else if node.has_component("mesh") {
        Icon::Cube
    } else {
        Icon::Layers
    }
}

#[derive(Clone, Copy)]
enum Category {
    Mesh,
    Material,
    Environment,
    Image,
    Model,
    Sound,
    Graph,
}

impl Category {
    const ALL: [Self; 7] = [
        Self::Mesh,
        Self::Material,
        Self::Environment,
        Self::Image,
        Self::Model,
        Self::Sound,
        Self::Graph,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Mesh => "Meshes",
            Self::Material => "Materials",
            Self::Environment => "Environments",
            Self::Image => "Images",
            Self::Model => "Models",
            Self::Sound => "Sounds",
            Self::Graph => "Graphs",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Mesh => "mesh",
            Self::Material => "material",
            Self::Environment => "environment",
            Self::Image => "image",
            Self::Model => "model",
            Self::Sound => "sound",
            Self::Graph => "graph",
        }
    }

    fn entries(self, scene: &Scene) -> Vec<(String, Target)> {
        match self {
            Self::Mesh => scene
                .meshes()
                .iter()
                .map(|entry| (entry.id.clone(), Target::Mesh(entry.id.clone())))
                .collect(),
            Self::Material => scene
                .materials()
                .iter()
                .map(|entry| (entry.id.clone(), Target::Material(entry.id.clone())))
                .collect(),
            Self::Environment => scene
                .environments()
                .iter()
                .map(|entry| (entry.id.clone(), Target::Environment(entry.id.clone())))
                .collect(),
            Self::Image => scene
                .images()
                .iter()
                .map(|entry| (entry.id.clone(), Target::Image(entry.id.clone())))
                .collect(),
            Self::Model => scene
                .models()
                .iter()
                .map(|entry| (entry.id.clone(), Target::Model(entry.id.clone())))
                .collect(),
            Self::Sound => scene
                .sounds()
                .iter()
                .map(|entry| (entry.id.clone(), Target::Sound(entry.id.clone())))
                .collect(),
            Self::Graph => scene
                .graphs()
                .iter()
                .map(|entry| (entry.id.clone(), Target::Graph(entry.id.clone())))
                .collect(),
        }
    }
}

fn assets_list(ctx: &BuildContext<'_, Action>, state: &State<'_>) -> Element<'static, Action> {
    let theme = ctx.theme();
    let palette = theme.palette;
    let metrics = theme.metrics;
    let mut groups = Vec::new();

    for category in Category::ALL {
        let entries = category.entries(state.scene);
        if entries.is_empty() {
            continue;
        }
        let count = entries.len();
        let mut rows = Vec::with_capacity(count);
        for (slot, (id, target)) in entries.into_iter().enumerate() {
            let selected = state.selected == Some(&target);
            let locked = state.scene.prefab_owner(&target).is_some();
            let corners = Corners::group_vertical(slot, count, 14.0, metrics.corner_xs, selected);
            let base = if selected {
                theme
                    .surface(Role::PrimaryContainer, UiState::Selected)
                    .with_corners(corners)
            } else {
                Surface::fill(palette.surface_container).with_corners(corners)
            };
            let hover = Surface {
                fill: Fill::Solid(if selected {
                    palette
                        .primary_container
                        .lerp(palette.on_primary_container, 0.14)
                } else {
                    palette.surface_container_high
                }),
                ..base
            };
            let lead = asset_lead(theme, state.scene, &target, selected);
            let text_color = if selected {
                palette.on_primary_container
            } else {
                palette.on_surface
            };
            let mut cells = vec![
                lead,
                label(
                    &id,
                    TextStyle::new(theme.font(Level::Body), text_color).truncating(),
                )
                .width(Length::Fill),
            ];
            if locked {
                cells.push(widget::icon(
                    Icon::Lock,
                    13.0,
                    if selected {
                        palette.on_primary_container
                    } else {
                        palette.outline
                    },
                    IconStyle::REGULAR,
                ));
            }
            rows.push(
                Element::row()
                    .key(Key::new(format!("asset.{}.{id}", category.key())))
                    .width(Length::Fill)
                    .height(Length::Fixed(34.0))
                    .padding(Edges::symmetric(10.0, 0.0))
                    .gap(8.0)
                    .align(Align::Center)
                    .background(base)
                    .hover_highlight(hover)
                    .children(cells)
                    .press(Action::Select(target)),
            );
        }
        groups.push(
            Element::column().width(Length::Fill).gap(4.0).children([
                Element::row()
                    .padding(Edges::symmetric(6.0, 0.0))
                    .children([label(
                        category.title().to_uppercase(),
                        TextStyle::new(theme.font(Level::Overline), palette.on_surface_variant),
                    )]),
                Element::column()
                    .width(Length::Fill)
                    .gap(metrics.group_gap)
                    .children(rows),
            ]),
        );
    }

    if groups.is_empty() {
        return empty_icon(theme, Icon::Folder);
    }

    Element::column()
        .width(Length::Fill)
        .gap(12.0)
        .children(groups)
}

fn asset_lead(
    theme: &dome_ui::Theme,
    scene: &Scene,
    target: &Target,
    selected: bool,
) -> Element<'static, Action> {
    let palette = theme.palette;
    let icon_color = if selected {
        palette.on_primary_container
    } else {
        palette.on_surface_variant
    };
    let icon_style = if selected {
        IconStyle::FILLED
    } else {
        IconStyle::REGULAR
    };
    match target {
        Target::Material(id) => {
            let color = scene
                .material(id)
                .map_or([0.8, 0.8, 0.8], |entry| entry.asset.base_color.to_array());
            widget::color_swatch(color, 18.0)
        }
        Target::Image(_) => {
            let name = texture_of(target).unwrap_or_default();
            widget::thumbnail(
                theme,
                &name,
                Length::Fixed(20.0),
                Length::Fixed(20.0),
                Corners::all(6.0),
            )
        }
        Target::Environment(id) => match scene.environment(id).map(|entry| &entry.source) {
            Some(EnvironmentSource::Panorama(_)) => {
                let name = texture_of(target).unwrap_or_default();
                widget::thumbnail(
                    theme,
                    &name,
                    Length::Fixed(20.0),
                    Length::Fixed(20.0),
                    Corners::all(6.0),
                )
            }
            Some(EnvironmentSource::Atmosphere(_)) => {
                widget::icon(Icon::Sparkles, 16.0, palette.tertiary, icon_style)
            }
            _ => widget::icon(Icon::Target, 16.0, icon_color, icon_style),
        },
        Target::Mesh(_) => widget::icon(Icon::Cube, 16.0, icon_color, icon_style),
        Target::Model(_) => widget::icon(Icon::Export, 16.0, icon_color, icon_style),
        Target::Sound(_) => widget::icon(Icon::Channel, 16.0, icon_color, icon_style),
        Target::Graph(_) => widget::icon(Icon::Chart, 16.0, icon_color, icon_style),
        Target::Node(_) => widget::icon(Icon::Layers, 16.0, icon_color, icon_style),
    }
}

fn empty_icon(theme: &dome_ui::Theme, symbol: Icon) -> Element<'static, Action> {
    let palette = theme.palette;
    Element::column()
        .width(Length::Fill)
        .padding(Edges::symmetric(0.0, 28.0))
        .align(Align::Center)
        .children([Element::column()
            .width(Length::Fixed(48.0))
            .height(Length::Fixed(48.0))
            .align(Align::Center)
            .justify(Align::Center)
            .background(Surface::fill(palette.surface_container).with_corners(Corners::all(16.0)))
            .children([widget::icon(
                symbol,
                22.0,
                palette.on_surface_variant,
                IconStyle::REGULAR,
            )])])
}

fn menu_action(
    theme: &dome_ui::Theme,
    key: &str,
    symbol: Icon,
    title: &str,
    action: Action,
) -> Element<'static, Action> {
    widget::action_row(theme, symbol, title, None)
        .key(Key::new(key))
        .press(action)
}

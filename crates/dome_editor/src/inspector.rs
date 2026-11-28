use dome_scene::{ComponentSpec, EnvironmentSource, Scene, Target, Value};
use dome_ui::{
    Align, Band, BuildContext, Corners, Edges, Element, Icon, IconStyle, Key, Length, Level,
    Motion, Role, State as UiState, Surface, TextStyle, popup, widget,
};

use crate::action::Action;
use crate::control::{fields_group, transform_rows};
use crate::outliner::node_icon;
use crate::stage::{State, label, texture_of};

pub(crate) fn header(
    ctx: &BuildContext<'_, Action>,
    state: &State<'_>,
    target: &Target,
) -> Element<'static, Action> {
    let theme = ctx.theme();
    let palette = theme.palette;
    let id = target_id(target);
    let (symbol, is_prefab) = match target {
        Target::Node(node_id) => match state.scene.node_index(node_id) {
            Some(index) => {
                let node = &state.scene.nodes()[index];
                (node_icon(node), node.prefab().is_some())
            }
            None => (Icon::Layers, false),
        },
        Target::Mesh(_) => (Icon::Cube, false),
        Target::Material(_) => (Icon::Sliders, false),
        Target::Image(_) => (Icon::Camera, false),
        Target::Model(_) => (Icon::Export, false),
        Target::Sound(_) => (Icon::Channel, false),
        Target::Graph(_) => (Icon::Chart, false),
        Target::Environment(_) => (Icon::Globe, false),
    };

    let (badge_bg, badge_fg) = if is_prefab {
        (palette.tertiary_container, palette.on_tertiary_container)
    } else {
        (palette.primary_container, palette.on_primary_container)
    };
    let badge = Element::column()
        .width(Length::Fixed(32.0))
        .height(Length::Fixed(32.0))
        .align(Align::Center)
        .justify(Align::Center)
        .background(Surface::fill(badge_bg).with_corners(Corners::all(10.0)))
        .children([widget::icon(symbol, 16.0, badge_fg, IconStyle::FILLED)]);

    let mut actions = Vec::new();
    if let Target::Node(node_id) = target
        && let Some(index) = state.scene.node_index(node_id)
    {
        let node = &state.scene.nodes()[index];
        actions.push(
            widget::icon_button(Icon::Target, 28.0, Role::Standard, "Focus (F)")
                .key(Key::new("inspector.focus"))
                .press(Action::Focus),
        );
        if node.prefab().is_none() {
            let hidden = node.has_component("hidden");
            actions.push(
                widget::icon_button(
                    if hidden { Icon::EyeOff } else { Icon::Eye },
                    28.0,
                    if hidden { Role::Tonal } else { Role::Standard },
                    "Visibility",
                )
                .key(Key::new("inspector.visibility"))
                .press(Action::ToggleHidden(node_id.clone())),
            );
        } else {
            actions.push(
                widget::icon_button(Icon::Layers, 28.0, Role::Tonal, "Unpack Prefab")
                    .key(Key::new("inspector.unpack"))
                    .press(Action::UnpackPrefab(node_id.clone())),
            );
        }
    }
    if can_remove(state.scene, target) {
        actions.push(
            widget::icon_button(Icon::Trash, 28.0, Role::Destructive, "Delete")
                .key(Key::new("inspector.delete"))
                .press(Action::RemoveTarget(target.clone())),
        );
    }

    Element::row()
        .width(Length::Fill)
        .padding(Edges::symmetric(10.0, 8.0))
        .gap(8.0)
        .align(Align::Center)
        .background(Surface::fill(palette.surface_container).with_corners(Corners::all(16.0)))
        .children([
            badge,
            label(
                id,
                TextStyle::new(theme.font(Level::Title), palette.on_surface).truncating(),
            )
            .width(Length::Fill),
            Element::row()
                .gap(2.0)
                .align(Align::Center)
                .children(actions),
        ])
}

pub(crate) fn body(
    ctx: &mut BuildContext<'_, Action>,
    state: &State<'_>,
    target: &Target,
) -> Element<'static, Action> {
    match target {
        Target::Node(id) => inspect_node(ctx, state, target, id),
        other => inspect_asset(ctx, state, other),
    }
}

fn inspect_node(
    ctx: &mut BuildContext<'_, Action>,
    state: &State<'_>,
    target: &Target,
    id: &str,
) -> Element<'static, Action> {
    let theme = ctx.theme().clone();
    let Some(index) = state.scene.node_index(id) else {
        return Element::column();
    };
    let node = &state.scene.nodes()[index];
    let mut blocks = vec![spatial_card(ctx, state.scene, target, index)];

    let mut attached: Vec<&'static ComponentSpec> = Vec::new();
    let mut addable: Vec<&'static ComponentSpec> = Vec::new();
    for spec in state.scene.schema().specs() {
        if spec.name == "hidden" {
            continue;
        }
        let present = node.has_component(spec.name);
        if spec.required || present {
            attached.push(*spec);
        } else if can_add_component(state.scene, index, spec) {
            addable.push(*spec);
        }
    }

    if !attached.is_empty() {
        let count = attached.len();
        let mut cards = Vec::with_capacity(count);
        for (slot, spec) in attached.into_iter().enumerate() {
            cards.push(component_card(
                ctx,
                state,
                target,
                index,
                spec,
                (slot, count),
            ));
        }
        blocks.push(
            Element::column()
                .width(Length::Fill)
                .gap(theme.metrics.group_gap)
                .children(cards),
        );
    }

    if !addable.is_empty() {
        blocks.push(add_component_menu(ctx, target, &addable));
    }

    Element::column()
        .width(Length::Fill)
        .gap(10.0)
        .children(blocks)
}

fn spatial_card(
    ctx: &BuildContext<'_, Action>,
    scene: &Scene,
    target: &Target,
    index: usize,
) -> Element<'static, Action> {
    let theme = ctx.theme();
    let node = &scene.nodes()[index];
    let mut rows = Vec::new();

    if node.owner().is_none() {
        let mut labels = vec![String::from("—")];
        let mut values = vec![String::new()];
        for (candidate_idx, candidate) in scene.nodes().iter().enumerate() {
            if candidate_idx == index || scene.is_ancestor(index, candidate_idx) {
                continue;
            }
            labels.push(candidate.id.clone());
            values.push(candidate.id.clone());
        }
        if values.len() > 1 {
            let current = scene
                .parent_index(index)
                .map(|at| scene.nodes()[at].id.clone())
                .unwrap_or_default();
            let selected = values.iter().position(|id| *id == current).unwrap_or(0);
            let t = target.clone();
            let parent_combo = widget::combo(
                theme,
                Key::new(format!("node.parent.{}", node.id)),
                Length::Fill,
                210.0,
                &labels,
                selected,
                move |at| Action::Property {
                    target: t.clone(),
                    path: String::from("parent"),
                    value: Value::Text(values[at].clone()),
                },
            );
            let lead = Element::row()
                .width(Length::Fixed(22.0))
                .height(Length::Fixed(theme.metrics.control_height))
                .align(Align::Center)
                .justify(Align::Center)
                .tooltip("Parent")
                .children([widget::icon(
                    Icon::Layers,
                    15.0,
                    theme.palette.on_surface_variant,
                    IconStyle::REGULAR,
                )]);
            rows.push(
                Element::row()
                    .width(Length::Fill)
                    .gap(6.0)
                    .align(Align::Center)
                    .children([lead, parent_combo]),
            );
        }
    }

    rows.push(transform_rows(ctx, scene, target, true));

    Element::column()
        .width(Length::Fill)
        .padding(Edges::all(10.0))
        .gap(8.0)
        .background(theme.surface(Role::Card, UiState::Rest))
        .children(rows)
}

fn component_card(
    ctx: &mut BuildContext<'_, Action>,
    state: &State<'_>,
    target: &Target,
    index: usize,
    spec: &'static ComponentSpec,
    slot: (usize, usize),
) -> Element<'static, Action> {
    let theme = ctx.theme().clone();
    let section_id = format!("{target}.{}", spec.name);
    let open = !state.collapsed_sections.contains(&section_id);
    let angle = ctx.animate(
        Key::new(format!("chevron.{section_id}")).id(),
        if open { 1.0 } else { 0.0 },
        Motion::spatial_fast(),
    );

    let disclosure = widget::disclosure_row(
        &theme,
        component_icon(spec.name),
        spec.label,
        "",
        open,
        angle,
        Band::new(Edges::ZERO, Corners::all(theme.metrics.corner_sm)),
    )
    .key(Key::new(format!("section.toggle.{section_id}")))
    .width(Length::Fill)
    .press(Action::ToggleSection(section_id.clone()));

    let removable = can_remove_component(state.scene, index, spec);
    let top = if removable {
        Element::row()
            .width(Length::Fill)
            .gap(4.0)
            .align(Align::Center)
            .children([
                disclosure,
                widget::icon_button(Icon::Trash, 26.0, Role::Destructive, "Remove")
                    .key(Key::new(format!("section.rm.{section_id}")))
                    .press(Action::RemoveComponent(target.clone(), spec.name)),
            ])
    } else {
        disclosure
    };

    let mut content = vec![top];
    if open && !spec.fields.is_empty() {
        content.push(fields_group(
            ctx,
            state.scene,
            target,
            spec.name,
            spec.fields,
            true,
        ));
    }

    widget::group_card(&theme, slot, open, Edges::all(10.0), 8.0, content)
}

fn add_component_menu(
    ctx: &BuildContext<'_, Action>,
    target: &Target,
    specs: &[&'static ComponentSpec],
) -> Element<'static, Action> {
    let theme = ctx.theme();
    let anchor = Key::new(format!("inspector.add_comp.{target}"));
    let open = ctx.popup_open(anchor);
    let button = widget::action_button(theme, Icon::Plus, "Component", Role::Tonal)
        .key(anchor)
        .width(Length::Fill)
        .shaped(if open {
            Corners::all(10.0)
        } else {
            Corners::capsule()
        })
        .anchor();

    let mut items = Vec::with_capacity(specs.len());
    for spec in specs {
        items.push(
            widget::action_row(theme, component_icon(spec.name), spec.label, None)
                .key(Key::new(format!(
                    "inspector.add_comp.{target}.{}",
                    spec.name
                )))
                .press(Action::AddComponent(target.clone(), spec.name)),
        );
    }

    let menu =
        popup(anchor, 288.0, items).key(Key::new(format!("inspector.add_comp.menu.{target}")));
    Element::column()
        .width(Length::Fill)
        .children([button, menu])
}

fn inspect_asset(
    ctx: &BuildContext<'_, Action>,
    state: &State<'_>,
    target: &Target,
) -> Element<'static, Action> {
    let theme = ctx.theme();
    let enabled = state.scene.prefab_owner(target).is_none();
    let mut blocks = Vec::new();

    if let Target::Graph(id) = target {
        return crate::graph::inspect(ctx, state.scene, id, enabled);
    }

    if let Some(texture) = preview_texture(state.scene, target) {
        let height = match target {
            Target::Environment(_) => 92.0,
            _ => 148.0,
        };
        blocks.push(widget::thumbnail(
            theme,
            &texture,
            Length::Fill,
            Length::Fixed(height),
            Corners::all(16.0),
        ));
    }

    match target {
        Target::Material(_) => {
            let base_fields = &[
                dome_scene::MATERIAL_FIELDS[0],
                dome_scene::MATERIAL_FIELDS[4],
                dome_scene::MATERIAL_FIELDS[5],
                dome_scene::MATERIAL_FIELDS[6],
                dome_scene::MATERIAL_FIELDS[7],
            ];
            let detail_fields = &dome_scene::MATERIAL_FIELDS[8..14];
            let alpha_fields = &[
                dome_scene::MATERIAL_FIELDS[2],
                dome_scene::MATERIAL_FIELDS[1],
                dome_scene::MATERIAL_FIELDS[3],
            ];
            let groups = [base_fields.as_slice(), detail_fields, alpha_fields];
            let mut cards = Vec::with_capacity(groups.len());
            for (slot, group) in groups.into_iter().enumerate() {
                cards.push(widget::group_card(
                    theme,
                    (slot, groups.len()),
                    false,
                    Edges::all(10.0),
                    6.0,
                    [fields_group(ctx, state.scene, target, "", group, enabled)],
                ));
            }
            blocks.push(
                Element::column()
                    .width(Length::Fill)
                    .gap(theme.metrics.group_gap)
                    .children(cards),
            );
        }
        Target::Mesh(id) => {
            if let Some(entry) = state.scene.mesh(id) {
                if entry.asset.shape_of().is_some() {
                    blocks.push(asset_card(
                        ctx,
                        state.scene,
                        target,
                        dome_scene::MESH_FIELDS,
                        enabled,
                    ));
                } else if let Some((model, ..)) = entry.asset.model_of() {
                    blocks.push(
                        Element::column()
                            .width(Length::Fill)
                            .padding(Edges::all(12.0))
                            .background(theme.surface(Role::Card, UiState::Rest))
                            .children([widget::labelled(theme, "Source", model.as_str())]),
                    );
                }
            }
        }
        Target::Environment(id) => {
            if let Some(entry) = state.scene.environment(id) {
                match &entry.source {
                    EnvironmentSource::Panorama(_) => {
                        blocks.push(asset_card(
                            ctx,
                            state.scene,
                            target,
                            dome_scene::ENVIRONMENT_PANORAMA_FIELDS,
                            enabled,
                        ));
                    }
                    EnvironmentSource::Atmosphere(_) => {
                        blocks.push(asset_card(
                            ctx,
                            state.scene,
                            target,
                            dome_scene::ENVIRONMENT_ATMOSPHERE_FIELDS,
                            enabled,
                        ));
                    }
                    EnvironmentSource::Capture => {}
                }
            }
        }
        Target::Image(id) => {
            let fields = match state.scene.image(id) {
                Some(entry) if entry.source.path().is_none() => &dome_scene::IMAGE_FIELDS[1..],
                _ => dome_scene::IMAGE_FIELDS,
            };
            blocks.push(asset_card(ctx, state.scene, target, fields, enabled));
        }
        Target::Model(_) => {
            blocks.push(asset_card(
                ctx,
                state.scene,
                target,
                dome_scene::MODEL_FIELDS,
                enabled,
            ));
        }
        Target::Sound(_) => {
            blocks.push(asset_card(
                ctx,
                state.scene,
                target,
                dome_scene::SOUND_FIELDS,
                enabled,
            ));
        }
        Target::Node(_) | Target::Graph(_) => {}
    }

    Element::column()
        .width(Length::Fill)
        .gap(10.0)
        .children(blocks)
}

fn asset_card(
    ctx: &BuildContext<'_, Action>,
    scene: &Scene,
    target: &Target,
    fields: &[dome_scene::Field],
    enabled: bool,
) -> Element<'static, Action> {
    let theme = ctx.theme();
    Element::column()
        .width(Length::Fill)
        .padding(Edges::all(10.0))
        .background(theme.surface(Role::Card, UiState::Rest))
        .children([fields_group(ctx, scene, target, "", fields, enabled)])
}

fn preview_texture(scene: &Scene, target: &Target) -> Option<String> {
    match target {
        Target::Image(_) => texture_of(target),
        Target::Environment(id) => scene
            .environment(id)
            .filter(|entry| entry.panorama().is_some())
            .and_then(|_| texture_of(target)),
        _ => None,
    }
}

fn target_id(target: &Target) -> &str {
    match target {
        Target::Node(id)
        | Target::Mesh(id)
        | Target::Material(id)
        | Target::Image(id)
        | Target::Model(id)
        | Target::Sound(id)
        | Target::Graph(id)
        | Target::Environment(id) => id,
    }
}

fn component_icon(name: &str) -> Icon {
    match name {
        "mesh" => Icon::Cube,
        "camera" => Icon::Camera,
        "directional_light" => Icon::Sparkles,
        "point_light" => Icon::Record,
        "environment" => Icon::Globe,
        "animation" | "skin" => Icon::Film,
        "audio_bus" | "audio_source" | "audio_listener" | "audio_filter" | "audio_compressor"
        | "audio_reverb" => Icon::Channel,
        "body" | "collider" | "joint" => Icon::Layers,
        _ => Icon::Sliders,
    }
}

fn can_add_component(scene: &Scene, index: usize, spec: &ComponentSpec) -> bool {
    let node = &scene.nodes()[index];
    if node.prefab().is_some() {
        return false;
    }
    let Ok(created) = scene.schema().create(scene, spec.name) else {
        return false;
    };
    created.validate(scene, &node.id).is_ok()
}

fn can_remove_component(scene: &Scene, index: usize, spec: &ComponentSpec) -> bool {
    if spec.required {
        return false;
    }
    let node = &scene.nodes()[index];
    if node.prefab().is_some() {
        return false;
    }
    if spec.name == "audio_bus"
        && ["audio_filter", "audio_compressor", "audio_reverb"]
            .into_iter()
            .any(|effect| node.has_component(effect))
    {
        return false;
    }
    true
}

fn can_remove(scene: &Scene, target: &Target) -> bool {
    match target {
        Target::Node(id) => scene
            .node_index(id)
            .is_some_and(|index| scene.structurally_editable(index)),
        other => scene.prefab_owner(other).is_none(),
    }
}

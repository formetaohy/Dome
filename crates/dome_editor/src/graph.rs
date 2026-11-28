use dome_animation::{Comparison, Parameter};
use dome_scene::{GraphEntry, ReferenceKind, Scene, Target, Value};
use dome_ui::{
    Align, BuildContext, Corners, Edges, Element, Icon, IconStyle, Key, Length, Level, Role,
    State as UiState, TextStyle, popup, widget,
};

use crate::action::{Action, ParameterKind};
use crate::stage::{label, references};

const FORM_LABEL: f32 = 76.0;

#[derive(Clone, Copy)]
struct Scope<'a> {
    theme: &'a dome_ui::Theme,
    target: &'a Target,
    entry: &'a GraphEntry,
    enabled: bool,
}

pub(crate) fn inspect(
    ctx: &BuildContext<'_, Action>,
    scene: &Scene,
    id: &str,
    enabled: bool,
) -> Element<'static, Action> {
    let theme = ctx.theme();
    let Some(entry) = scene.graph(id) else {
        return Element::column();
    };
    let target = Target::Graph(String::from(id));
    let scope = Scope {
        theme,
        target: &target,
        entry,
        enabled,
    };
    let mut blocks = vec![
        entry_card(scope),
        states_section(scope, scene),
        parameters_section(ctx, scope),
    ];
    if entry.asset.states().len() >= 2 || !entry.asset.transitions().is_empty() {
        blocks.push(transitions_section(scope));
    }
    Element::column()
        .width(Length::Fill)
        .gap(12.0)
        .children(blocks)
}

fn entry_card(scope: Scope<'_>) -> Element<'static, Action> {
    let Scope {
        theme,
        target,
        entry,
        enabled,
    } = scope;
    let states: Vec<String> = entry
        .asset
        .states()
        .iter()
        .map(|s| String::from(s.id()))
        .collect();
    let selected = states
        .iter()
        .position(|name| name == entry.asset.entry())
        .unwrap_or(0);
    let values = states.clone();
    let t = target.clone();
    let combo = widget::combo(
        theme,
        Key::new(format!("graph.entry.{}", entry.id)),
        Length::Fill,
        200.0,
        &states,
        selected,
        move |at| Action::Property {
            target: t.clone(),
            path: String::from("entry"),
            value: Value::Text(values[at].clone()),
        },
    )
    .enabled(enabled);

    Element::column()
        .width(Length::Fill)
        .padding(Edges::all(12.0))
        .background(theme.surface(Role::Card, UiState::Rest))
        .children([widget::form_row(theme, FORM_LABEL, "Entry", combo)])
}

fn states_section(scope: Scope<'_>, scene: &Scene) -> Element<'static, Action> {
    let Scope {
        theme,
        target,
        entry,
        enabled,
    } = scope;
    let graph_id = entry.id.as_str();
    let clips = references(scene, ReferenceKind::Animation);
    let states = entry.asset.states();
    let count = states.len();
    let mut cards = Vec::with_capacity(count);

    for (slot, state) in states.iter().enumerate() {
        let state_id = String::from(state.id());
        let removable = enabled
            && count > 1
            && entry.asset.entry() != state_id
            && !entry
                .asset
                .transitions()
                .iter()
                .any(|tr| tr.from() == state_id || tr.to() == state_id);

        let mut head = vec![
            widget::icon(
                if entry.asset.entry() == state_id {
                    Icon::Play
                } else {
                    Icon::Film
                },
                15.0,
                theme.palette.primary,
                IconStyle::FILLED,
            ),
            label(
                &state_id,
                TextStyle::new(theme.font(Level::Title), theme.palette.on_surface).truncating(),
            )
            .width(Length::Fill),
        ];
        if removable {
            head.push(
                widget::icon_button(Icon::Trash, 24.0, Role::Destructive, "Remove")
                    .key(Key::new(format!("graph.{graph_id}.state.{state_id}.rm")))
                    .press(Action::RemoveGraphState(
                        String::from(graph_id),
                        state_id.clone(),
                    )),
            );
        }

        let mut rows = vec![
            Element::row()
                .width(Length::Fill)
                .gap(6.0)
                .align(Align::Center)
                .children(head),
        ];

        if !clips.is_empty() {
            let clip_selected = clips
                .iter()
                .position(|c| c == state.clip().as_str())
                .unwrap_or(0);
            let clip_values = clips.clone();
            let t = target.clone();
            let sid = state_id.clone();
            let clip_combo = widget::combo(
                theme,
                Key::new(format!("graph.{graph_id}.state.{state_id}.clip")),
                Length::Fill,
                200.0,
                &clips,
                clip_selected,
                move |at| Action::Property {
                    target: t.clone(),
                    path: format!("state.{sid}.clip"),
                    value: Value::Text(clip_values[at].clone()),
                },
            )
            .enabled(enabled);
            rows.push(widget::form_row(theme, FORM_LABEL, "Clip", clip_combo));
        }

        let speed_val = state.speed();
        let t_speed = target.clone();
        let sid_speed = state_id.clone();
        rows.push(
            widget::slider_readout(
                theme,
                Some("Speed"),
                speed_val,
                0.0..=4.0,
                false,
                &format!("{speed_val:.2}x"),
            )
            .key(Key::new(format!("graph.{graph_id}.state.{state_id}.speed")))
            .enabled(enabled)
            .on_slide(move |next| Action::Property {
                target: t_speed.clone(),
                path: format!("state.{sid_speed}.speed"),
                value: Value::Number(next),
            }),
        );

        let t_loop = target.clone();
        let sid_loop = state_id.clone();
        rows.push(
            widget::toggle_row(theme, "Loop", state.looped())
                .key(Key::new(format!("graph.{graph_id}.state.{state_id}.loop")))
                .enabled(enabled)
                .on_toggle(move |next| Action::Property {
                    target: t_loop.clone(),
                    path: format!("state.{sid_loop}.looped"),
                    value: Value::Bool(next),
                }),
        );

        cards.push(widget::group_card(
            theme,
            (slot, count),
            false,
            Edges::all(10.0),
            6.0,
            rows,
        ));
    }

    let mut top = vec![
        label(
            "STATES",
            TextStyle::new(
                theme.font(Level::Overline),
                theme.palette.on_surface_variant,
            ),
        )
        .width(Length::Fill),
    ];
    if enabled && !clips.is_empty() {
        top.push(
            widget::icon_button(Icon::Plus, 24.0, Role::Tonal, "Add State")
                .key(Key::new(format!("graph.{graph_id}.state.add")))
                .press(Action::AddGraphState(String::from(graph_id))),
        );
    }

    Element::column().width(Length::Fill).gap(4.0).children([
        Element::row()
            .width(Length::Fill)
            .padding(Edges::symmetric(4.0, 0.0))
            .align(Align::Center)
            .children(top),
        Element::column()
            .width(Length::Fill)
            .gap(theme.metrics.group_gap)
            .children(cards),
    ])
}

fn parameters_section(
    ctx: &BuildContext<'_, Action>,
    scope: Scope<'_>,
) -> Element<'static, Action> {
    let Scope {
        theme,
        target,
        entry,
        enabled,
    } = scope;
    let graph_id = entry.id.as_str();
    let add_key = Key::new(format!("graph.{graph_id}.param.add"));
    let open = ctx.popup_open(add_key);
    let mut top = vec![
        label(
            "PARAMETERS",
            TextStyle::new(
                theme.font(Level::Overline),
                theme.palette.on_surface_variant,
            ),
        )
        .width(Length::Fill),
    ];
    if enabled {
        top.push(
            widget::icon_button(
                if open { Icon::Close } else { Icon::Plus },
                24.0,
                Role::Tonal,
                "Add Parameter",
            )
            .key(add_key)
            .hit_target(),
        );
        top.push(
            popup(
                add_key,
                160.0,
                [
                    widget::action_row(theme, Icon::Sliders, "Number", None)
                        .key(Key::new(format!("graph.{graph_id}.param.add.num")))
                        .press(Action::AddGraphParameter(
                            String::from(graph_id),
                            ParameterKind::Number,
                        )),
                    widget::action_row(theme, Icon::Check, "Boolean", None)
                        .key(Key::new(format!("graph.{graph_id}.param.add.bool")))
                        .press(Action::AddGraphParameter(
                            String::from(graph_id),
                            ParameterKind::Boolean,
                        )),
                ],
            )
            .key(Key::new(format!("graph.{graph_id}.param.menu"))),
        );
    }

    let parameters = entry.asset.parameters();
    let mut body = vec![
        Element::row()
            .width(Length::Fill)
            .padding(Edges::symmetric(4.0, 0.0))
            .align(Align::Center)
            .children(top),
    ];

    if !parameters.is_empty() {
        let count = parameters.len();
        let mut cards = Vec::with_capacity(count);
        for (slot, param) in parameters.iter().enumerate() {
            let name = String::from(param.name());
            let used = entry
                .asset
                .transitions()
                .iter()
                .any(|tr| tr.conditions().iter().any(|cond| cond.parameter() == name));
            let t = target.clone();
            let pname = name.clone();
            let control = match param.default() {
                Parameter::Number(val) => widget::drag_value(theme, val, 2)
                    .key(Key::new(format!("graph.{graph_id}.param.{name}.val")))
                    .width(Length::Fixed(84.0))
                    .enabled(enabled)
                    .on_drag_value(0.05, move |next| Action::Property {
                        target: t.clone(),
                        path: format!("parameter.{pname}.value"),
                        value: Value::Number(next),
                    }),
                Parameter::Boolean(val) => widget::toggle(theme, val)
                    .key(Key::new(format!("graph.{graph_id}.param.{name}.val")))
                    .enabled(enabled)
                    .on_toggle(move |next| Action::Property {
                        target: t.clone(),
                        path: format!("parameter.{pname}.value"),
                        value: Value::Bool(next),
                    }),
            };
            let mut row = vec![
                label(
                    &name,
                    TextStyle::new(theme.font(Level::Body), theme.palette.on_surface).truncating(),
                )
                .width(Length::Fill),
                control,
            ];
            if enabled && !used {
                row.push(
                    widget::icon_button(Icon::Trash, 24.0, Role::Destructive, "Remove")
                        .key(Key::new(format!("graph.{graph_id}.param.{name}.rm")))
                        .press(Action::RemoveGraphParameter(String::from(graph_id), name)),
                );
            }
            cards.push(widget::group_card(
                theme,
                (slot, count),
                false,
                Edges::symmetric(10.0, 6.0),
                4.0,
                [Element::row()
                    .width(Length::Fill)
                    .gap(6.0)
                    .align(Align::Center)
                    .children(row)],
            ));
        }
        body.push(
            Element::column()
                .width(Length::Fill)
                .gap(theme.metrics.group_gap)
                .children(cards),
        );
    }

    Element::column()
        .width(Length::Fill)
        .gap(4.0)
        .children(body)
}

fn transitions_section(scope: Scope<'_>) -> Element<'static, Action> {
    let Scope {
        theme,
        target,
        entry,
        enabled,
    } = scope;
    let graph_id = entry.id.as_str();
    let states: Vec<String> = entry
        .asset
        .states()
        .iter()
        .map(|s| String::from(s.id()))
        .collect();
    let transitions = entry.asset.transitions();
    let mut top = vec![
        label(
            "TRANSITIONS",
            TextStyle::new(
                theme.font(Level::Overline),
                theme.palette.on_surface_variant,
            ),
        )
        .width(Length::Fill),
    ];
    if enabled && states.len() >= 2 {
        top.push(
            widget::icon_button(Icon::Plus, 24.0, Role::Tonal, "Add Transition")
                .key(Key::new(format!("graph.{graph_id}.tr.add")))
                .press(Action::AddGraphTransition(String::from(graph_id))),
        );
    }

    let mut body = vec![
        Element::row()
            .width(Length::Fill)
            .padding(Edges::symmetric(4.0, 0.0))
            .align(Align::Center)
            .children(top),
    ];

    if !transitions.is_empty() {
        let count = transitions.len();
        let mut cards = Vec::with_capacity(count);
        for (index, tr) in transitions.iter().enumerate() {
            let from_options: Vec<String> =
                states.iter().filter(|s| *s != tr.to()).cloned().collect();
            let to_options: Vec<String> =
                states.iter().filter(|s| *s != tr.from()).cloned().collect();
            let from_sel = from_options
                .iter()
                .position(|s| s == tr.from())
                .unwrap_or(0);
            let to_sel = to_options.iter().position(|s| s == tr.to()).unwrap_or(0);

            let from_vals = from_options.clone();
            let t_from = target.clone();
            let from_combo = widget::combo(
                theme,
                Key::new(format!("graph.{graph_id}.tr.{index}.from")),
                Length::Fill,
                160.0,
                &from_options,
                from_sel,
                move |at| Action::Property {
                    target: t_from.clone(),
                    path: format!("transition.{index}.from"),
                    value: Value::Text(from_vals[at].clone()),
                },
            )
            .enabled(enabled);

            let to_vals = to_options.clone();
            let t_to = target.clone();
            let to_combo = widget::combo(
                theme,
                Key::new(format!("graph.{graph_id}.tr.{index}.to")),
                Length::Fill,
                160.0,
                &to_options,
                to_sel,
                move |at| Action::Property {
                    target: t_to.clone(),
                    path: format!("transition.{index}.to"),
                    value: Value::Text(to_vals[at].clone()),
                },
            )
            .enabled(enabled);

            let mut route = vec![
                from_combo,
                widget::icon(
                    Icon::ChevronRight,
                    14.0,
                    theme.palette.on_surface_variant,
                    IconStyle::REGULAR,
                ),
                to_combo,
            ];
            if enabled {
                route.push(
                    widget::icon_button(Icon::Trash, 24.0, Role::Destructive, "Remove")
                        .key(Key::new(format!("graph.{graph_id}.tr.{index}.rm")))
                        .press(Action::RemoveGraphTransition(String::from(graph_id), index)),
                );
            }

            let fade_val = tr.fade();
            let t_fade = target.clone();
            let fade_row = widget::slider_readout(
                theme,
                Some("Fade"),
                fade_val,
                0.0..=2.0,
                false,
                &format!("{fade_val:.2}s"),
            )
            .key(Key::new(format!("graph.{graph_id}.tr.{index}.fade")))
            .enabled(enabled)
            .on_slide(move |next| Action::Property {
                target: t_fade.clone(),
                path: format!("transition.{index}.fade"),
                value: Value::Number(next),
            });

            let exit_min = if tr.conditions().is_empty() {
                0.01
            } else {
                0.0
            };
            let exit_val = tr.exit();
            let t_exit = target.clone();
            let exit_row = widget::slider_readout(
                theme,
                Some("Exit"),
                exit_val,
                exit_min..=1.0,
                false,
                &format!("{exit_val:.2}"),
            )
            .key(Key::new(format!("graph.{graph_id}.tr.{index}.exit")))
            .enabled(enabled)
            .on_slide(move |next| Action::Property {
                target: t_exit.clone(),
                path: format!("transition.{index}.exit"),
                value: Value::Number(next.clamp(exit_min, 1.0)),
            });

            let mut rows = vec![
                Element::row()
                    .width(Length::Fill)
                    .gap(4.0)
                    .align(Align::Center)
                    .children(route),
                fade_row,
                exit_row,
            ];

            for (at, cond) in tr.conditions().iter().enumerate() {
                rows.push(condition_row(scope, (index, at), cond));
            }

            if enabled && !entry.asset.parameters().is_empty() {
                rows.push(
                    Element::row()
                        .width(Length::Fill)
                        .justify(Align::End)
                        .children([widget::action_button(
                            theme,
                            Icon::Plus,
                            "Condition",
                            Role::Standard,
                        )
                        .key(Key::new(format!("graph.{graph_id}.tr.{index}.cond.add")))
                        .press(Action::AddGraphCondition(String::from(graph_id), index))]),
                );
            }

            cards.push(widget::group_card(
                theme,
                (index, count),
                false,
                Edges::all(10.0),
                6.0,
                rows,
            ));
        }
        body.push(
            Element::column()
                .width(Length::Fill)
                .gap(theme.metrics.group_gap)
                .children(cards),
        );
    }

    Element::column()
        .width(Length::Fill)
        .gap(4.0)
        .children(body)
}

fn condition_row(
    scope: Scope<'_>,
    indices: (usize, usize),
    cond: &dome_scene::GraphCondition,
) -> Element<'static, Action> {
    let Scope {
        theme,
        target,
        entry,
        enabled,
    } = scope;
    let graph_id = entry.id.as_str();
    let (tr_index, cond_index) = indices;
    let is_bool = matches!(cond.value(), Parameter::Boolean(_));
    let comparisons: &[Comparison] = if is_bool {
        &[Comparison::Equal, Comparison::NotEqual]
    } else {
        &Comparison::ALL
    };
    let comp_labels: Vec<String> = comparisons
        .iter()
        .map(|c| String::from(symbol_of(*c)))
        .collect();
    let comp_sel = comparisons
        .iter()
        .position(|c| *c == cond.comparison())
        .unwrap_or(0);
    let comp_vals: Vec<Comparison> = comparisons.to_vec();
    let t_comp = target.clone();
    let comp_combo = widget::combo(
        theme,
        Key::new(format!("graph.{graph_id}.tr.{tr_index}.c.{cond_index}.cmp")),
        Length::Fixed(62.0),
        90.0,
        &comp_labels,
        comp_sel,
        move |at| Action::Property {
            target: t_comp.clone(),
            path: format!("transition.{tr_index}.condition.{cond_index}.comparison"),
            value: Value::Text(String::from(comp_vals[at].name())),
        },
    )
    .enabled(enabled);

    let t_val = target.clone();
    let val_widget = match cond.value() {
        Parameter::Number(num) => widget::drag_value(theme, num, 2)
            .key(Key::new(format!(
                "graph.{graph_id}.tr.{tr_index}.c.{cond_index}.val"
            )))
            .width(Length::Fixed(64.0))
            .enabled(enabled)
            .on_drag_value(0.05, move |next| Action::Property {
                target: t_val.clone(),
                path: format!("transition.{tr_index}.condition.{cond_index}.value"),
                value: Value::Number(next),
            }),
        Parameter::Boolean(flag) => widget::toggle(theme, flag)
            .key(Key::new(format!(
                "graph.{graph_id}.tr.{tr_index}.c.{cond_index}.val"
            )))
            .enabled(enabled)
            .on_toggle(move |next| Action::Property {
                target: t_val.clone(),
                path: format!("transition.{tr_index}.condition.{cond_index}.value"),
                value: Value::Bool(next),
            }),
    };

    let can_remove = enabled
        && (entry.asset.transitions()[tr_index].conditions().len() > 1
            || entry.asset.transitions()[tr_index].exit() > 0.0);

    let mut cells = vec![
        label(
            cond.parameter(),
            TextStyle::new(theme.label_font(), theme.palette.on_surface_variant).truncating(),
        )
        .width(Length::Fill),
        comp_combo,
        val_widget,
    ];
    if can_remove {
        cells.push(
            widget::icon_button(Icon::Close, 22.0, Role::Destructive, "Remove")
                .key(Key::new(format!(
                    "graph.{graph_id}.tr.{tr_index}.c.{cond_index}.rm"
                )))
                .shaped(Corners::capsule())
                .press(Action::RemoveGraphCondition(
                    String::from(graph_id),
                    tr_index,
                    cond_index,
                )),
        );
    }

    Element::row()
        .width(Length::Fill)
        .gap(4.0)
        .align(Align::Center)
        .children(cells)
}

fn symbol_of(comparison: Comparison) -> &'static str {
    match comparison {
        Comparison::Above => ">",
        Comparison::AboveOrEqual => ">=",
        Comparison::Below => "<",
        Comparison::BelowOrEqual => "<=",
        Comparison::Equal => "==",
        Comparison::NotEqual => "!=",
    }
}

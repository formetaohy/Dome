use dome_math::Vec3;
use dome_scene::{Field, FieldKind, ReferenceKind, Target, Value};
use dome_ui::{Align, BuildContext, Corners, Element, Icon, Key, Length, TextStyle, widget};

use crate::message::Message;
use crate::shell::{View, references, section, text};

const LABEL: f32 = 96.0;

pub(crate) fn panel<'a>(ctx: &BuildContext<'_, Message>, view: &View<'a>) -> Element<'a, Message> {
    let theme = ctx.theme();
    let Some(target) = view.selected else {
        return section(
            ctx,
            "Inspector",
            [widget::empty_state(
                theme,
                Icon::Sliders,
                "Nothing selected",
            )],
        );
    };
    let mut children = vec![identity(ctx, title_of(target))];
    match target {
        Target::Node(id) => {
            let Some(index) = view.scene.node_index(id) else {
                return section(ctx, "Inspector", children);
            };
            children.push(parent(ctx, view, id));
            children.push(transform(ctx, view, target));
            let mut missing = Vec::new();
            for spec in view.scene.schema().specs() {
                let present = view.scene.nodes()[index].has_component(spec.name);
                if spec.required || present {
                    children.push(component(ctx, view, target, spec, present));
                } else {
                    missing.push(*spec);
                }
            }
            if !missing.is_empty() {
                children.push(add_component(ctx, target, &missing));
            }
        }
        other => {
            if let Some(texture) = crate::shell::texture_of(other) {
                children.push(preview(ctx, other, &texture));
            }
            let fields = match other {
                Target::Mesh(_) => dome_scene::MESH_FIELDS,
                Target::Material(_) => dome_scene::MATERIAL_FIELDS,
                Target::Image(_) => dome_scene::IMAGE_FIELDS,
                Target::Model(_) => dome_scene::MODEL_FIELDS,
                Target::Sound(_) => dome_scene::SOUND_FIELDS,
                Target::Environment(_) => dome_scene::ENVIRONMENT_PANORAMA_FIELDS,
                _ => &[],
            };
            children.push(fields_section(ctx, view, other, "", fields));
        }
    }
    section(ctx, "Inspector", children)
}

fn title_of(target: &Target) -> &str {
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

fn preview<'a>(
    ctx: &BuildContext<'_, Message>,
    target: &Target,
    texture: &str,
) -> Element<'a, Message> {
    let theme = ctx.theme();
    let (width, height) = match target {
        Target::Environment(_) => (Length::Fill, Length::Fixed(72.0)),
        _ => (Length::Fixed(120.0), Length::Fixed(120.0)),
    };
    widget::thumbnail(
        theme,
        texture,
        width,
        height,
        Corners::all(theme.metrics.corner_md),
    )
}

fn identity<'a>(ctx: &BuildContext<'_, Message>, id: &str) -> Element<'a, Message> {
    let theme = ctx.theme();
    Element::column()
        .width(Length::Fill)
        .gap(4.0)
        .children([text(
            id,
            TextStyle::new(theme.font(dome_ui::Level::Title), theme.palette.on_surface),
        )])
}

fn parent<'a>(ctx: &BuildContext<'_, Message>, view: &View<'a>, id: &str) -> Element<'a, Message> {
    let theme = ctx.theme();
    let mut labels = vec![String::new()];
    labels.extend(references(view.scene, ReferenceKind::Node));
    let choices = labels.clone();
    let held = view
        .scene
        .node_index(id)
        .and_then(|index| view.scene.parent_index(index))
        .map(|index| view.scene.nodes()[index].id.clone())
        .unwrap_or_default();
    let selected = labels.iter().position(|name| *name == held).unwrap_or(0);
    let target = Target::Node(String::from(id));
    widget::form_row(
        theme,
        LABEL,
        "Parent",
        widget::combo(
            theme,
            Key::new(format!("parent.{id}")),
            Length::Fill,
            220.0,
            &labels,
            selected,
            move |at| Message::Property {
                target: target.clone(),
                path: String::from("parent"),
                value: Value::Text(choices[at].clone()),
            },
        ),
    )
}

fn component<'a>(
    ctx: &BuildContext<'_, Message>,
    view: &View<'a>,
    target: &Target,
    spec: &dome_scene::ComponentSpec,
    present: bool,
) -> Element<'a, Message> {
    let theme = ctx.theme();
    let mut header = vec![text(
        spec.label,
        TextStyle::new(theme.font(dome_ui::Level::Label), theme.palette.on_surface),
    )];
    if !spec.required {
        header.push(Element::row().width(Length::Fill));
        header.push(
            widget::icon(
                Icon::Trash,
                14.0,
                theme.palette.on_surface_variant,
                Default::default(),
            )
            .on_pointer({
                let target = target.clone();
                let name = String::from(spec.name);
                move |pointer| {
                    matches!(pointer.phase, dome_ui::Phase::Down)
                        .then(|| Message::RemoveComponent(target.clone(), name.clone()))
                }
            }),
        );
    }
    Element::column()
        .width(Length::Fill)
        .gap(5.0)
        .children([
            Element::row()
                .width(Length::Fill)
                .align(Align::Center)
                .gap(6.0)
                .children(header),
            fields_section(ctx, view, target, spec.name, spec.fields),
        ])
        .enabled(present)
}

fn add_component<'a>(
    ctx: &BuildContext<'_, Message>,
    target: &Target,
    missing: &[&'static dome_scene::ComponentSpec],
) -> Element<'a, Message> {
    let theme = ctx.theme();
    let target = target.clone();
    let labels = missing
        .iter()
        .map(|spec| String::from(spec.label))
        .collect::<Vec<_>>();
    let names = missing
        .iter()
        .map(|spec| String::from(spec.name))
        .collect::<Vec<_>>();
    let key = Key::new("inspector.add");
    widget::combo(theme, key, Length::Fill, 240.0, &labels, 0, move |at| {
        Message::AddComponent(target.clone(), names[at].clone())
    })
    .tooltip("Add component")
}

fn transform<'a>(
    ctx: &BuildContext<'_, Message>,
    view: &View<'a>,
    target: &Target,
) -> Element<'a, Message> {
    let theme = ctx.theme();
    Element::column().width(Length::Fill).gap(5.0).children([
        text(
            "Transform",
            TextStyle::new(theme.font(dome_ui::Level::Label), theme.palette.on_surface),
        ),
        fields_section(ctx, view, target, "transform", dome_scene::TRANSFORM_FIELDS),
    ])
}

fn fields_section<'a>(
    ctx: &BuildContext<'_, Message>,
    view: &View<'a>,
    target: &Target,
    prefix: &str,
    fields: &[Field],
) -> Element<'a, Message> {
    let mut rows = Vec::new();
    for field in fields {
        let path = if prefix.is_empty() {
            String::from(field.name)
        } else {
            format!("{prefix}.{}", field.name)
        };
        if let Some(row) = value_field(ctx, view, target, &path, field) {
            rows.push(row);
        }
    }
    Element::column()
        .width(Length::Fill)
        .gap(4.0)
        .children(rows)
}

fn value_field<'a>(
    ctx: &BuildContext<'_, Message>,
    view: &View<'a>,
    target: &Target,
    path: &str,
    field: &Field,
) -> Option<Element<'a, Message>> {
    let theme = ctx.theme();
    let style = TextStyle::new(theme.font(dome_ui::Level::Body), theme.palette.on_surface);
    let value = view.scene.property(target, path)?;
    match field.kind {
        FieldKind::Number { range, .. } => {
            let held = value.number().ok()?;
            Some(
                widget::slider_readout(
                    theme,
                    Some(field.label),
                    held,
                    range[0]..=range[1],
                    false,
                    &format!("{held:.2}"),
                )
                .on_slide({
                    let target = target.clone();
                    let path = String::from(path);
                    move |held| Message::Property {
                        target: target.clone(),
                        path: path.clone(),
                        value: Value::Number(held),
                    }
                }),
            )
        }
        FieldKind::Bool => {
            let held = value.boolean().ok()?;
            Some(widget::toggle_row(theme, field.label, held).on_toggle({
                let target = target.clone();
                let path = String::from(path);
                move |held| Message::Property {
                    target: target.clone(),
                    path: path.clone(),
                    value: Value::Bool(held),
                }
            }))
        }
        FieldKind::Text => {
            let held = String::from(value.text().ok()?);
            Some(widget::form_row(
                theme,
                LABEL,
                field.label,
                widget::text_field(&held, style, 180.0, false).on_commit({
                    let target = target.clone();
                    let path = String::from(path);
                    move |held| Message::Property {
                        target: target.clone(),
                        path: path.clone(),
                        value: Value::Text(String::from(held)),
                    }
                }),
            ))
        }
        FieldKind::Choice(options) => {
            let held = String::from(value.text().ok()?);
            let selected = options
                .iter()
                .position(|option| *option == held)
                .unwrap_or(0);
            Some(widget::form_row(
                theme,
                LABEL,
                field.label,
                widget::segmented(theme, options, selected).on_segment({
                    let target = target.clone();
                    let path = String::from(path);
                    let options = options.to_vec();
                    move |at| Message::Property {
                        target: target.clone(),
                        path: path.clone(),
                        value: Value::Text(String::from(options[at])),
                    }
                }),
            ))
        }
        FieldKind::Vector { range, .. } | FieldKind::Rotation { range, .. } => {
            let held = value.vector().ok()?;
            Some(axes(ctx, target, path, field.label, held, range))
        }
        FieldKind::Color | FieldKind::Tint => {
            let held = value.rgba().ok()?;
            let values = Vec3::new(held.x, held.y, held.z);
            Some(axes(ctx, target, path, field.label, values, [0.0, 1.0]))
        }
        FieldKind::Bits { count } => {
            let held = value.number().ok()? as u32;
            let mut rows = vec![text(
                field.label,
                TextStyle::new(
                    theme.font(dome_ui::Level::Label),
                    theme.palette.on_surface_variant,
                ),
            )];
            for bit in 0..u32::from(count) {
                let path = String::from(path);
                let target = target.clone();
                let on = (held >> bit) & 1 == 1;
                rows.push(
                    widget::toggle_row(theme, &format!("bit {bit}"), on).on_toggle(move |value| {
                        let mask = 1u32 << bit;
                        let next = if value {
                            (held & !mask) | mask
                        } else {
                            held & !mask
                        };
                        Message::Property {
                            target: target.clone(),
                            path: path.clone(),
                            value: Value::Number(next as f32),
                        }
                    }),
                );
            }
            Some(
                Element::column()
                    .width(Length::Fill)
                    .gap(2.0)
                    .children(rows),
            )
        }
        FieldKind::Reference(kind) => reference(ctx, view, target, path, field, value, kind),
        FieldKind::OptionalReference(kind) => {
            reference(ctx, view, target, path, field, value, kind)
        }
    }
}

fn axes<'a>(
    ctx: &BuildContext<'_, Message>,
    target: &Target,
    path: &str,
    label: &str,
    value: Vec3,
    range: [f32; 2],
) -> Element<'a, Message> {
    let theme = ctx.theme();
    let mut rows = vec![text(
        label,
        TextStyle::new(
            theme.font(dome_ui::Level::Label),
            theme.palette.on_surface_variant,
        ),
    )];
    for (axis, held) in [("x", value.x), ("y", value.y), ("z", value.z)] {
        let axis_path = format!("{path}.{axis}");
        let target = target.clone();
        rows.push(
            widget::slider_readout(
                theme,
                Some(axis),
                held,
                range[0]..=range[1],
                false,
                &format!("{held:.2}"),
            )
            .on_slide(move |held| Message::Property {
                target: target.clone(),
                path: axis_path.clone(),
                value: Value::Number(held),
            }),
        );
    }
    Element::column()
        .width(Length::Fill)
        .gap(2.0)
        .children(rows)
}

fn reference<'a>(
    ctx: &BuildContext<'_, Message>,
    view: &View<'a>,
    target: &Target,
    path: &str,
    field: &Field,
    value: Value,
    kind: ReferenceKind,
) -> Option<Element<'a, Message>> {
    let theme = ctx.theme();
    let held = String::from(value.text().ok()?);
    let mut labels = references(view.scene, kind);
    if matches!(field.kind, FieldKind::OptionalReference(_)) {
        labels.insert(0, String::new());
    }
    if labels.is_empty() {
        return None;
    }
    let selected = labels.iter().position(|id| *id == held).unwrap_or(0);
    let choices = labels.clone();
    Some(widget::form_row(
        theme,
        LABEL,
        field.label,
        widget::combo(
            theme,
            Key::new(format!("reference.{path}")),
            Length::Fill,
            220.0,
            &labels,
            selected,
            {
                let target = target.clone();
                let path = String::from(path);
                move |at| Message::Property {
                    target: target.clone(),
                    path: path.clone(),
                    value: Value::Text(choices[at].clone()),
                }
            },
        ),
    ))
}

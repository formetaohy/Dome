use dome_math::Vec3;
use dome_scene::{Field, FieldKind, ReferenceKind, Scene, Target, Value};
use dome_ui::{
    Align, BuildContext, Corners, Element, Icon, IconStyle, Key, Length, Level, Role, TextStyle,
    popup, widget,
};

use crate::action::Action;
use crate::gizmo::Axis;
use crate::stage::{label, references};

const FORM_LABEL: f32 = 96.0;

#[derive(Clone, Copy)]
struct AxisSpec<'a> {
    range: [f32; 2],
    speed: f32,
    unit: &'a str,
}

pub(crate) fn transform_rows(
    ctx: &BuildContext<'_, Action>,
    scene: &Scene,
    target: &Target,
    enabled: bool,
) -> Element<'static, Action> {
    let theme = ctx.theme();
    let specs = [
        (
            Icon::Fit,
            "Translation",
            "transform.translation",
            AxisSpec {
                range: [-10000.0, 10000.0],
                speed: 0.01,
                unit: "",
            },
        ),
        (
            Icon::Reset,
            "Rotation",
            "transform.rotation",
            AxisSpec {
                range: [-180.0, 180.0],
                speed: 0.5,
                unit: "°",
            },
        ),
        (
            Icon::WindowMaximize,
            "Scale",
            "transform.scale",
            AxisSpec {
                range: [0.001, 10000.0],
                speed: 0.01,
                unit: "",
            },
        ),
    ];
    let mut rows = Vec::with_capacity(3);
    for (symbol, tip, path, spec) in specs {
        let Some(value) = scene.property(target, path).and_then(|v| v.vector().ok()) else {
            continue;
        };
        let lead = Element::row()
            .width(Length::Fixed(22.0))
            .height(Length::Fixed(theme.metrics.control_height))
            .align(Align::Center)
            .justify(Align::Center)
            .tooltip(tip)
            .children([widget::icon(
                symbol,
                15.0,
                theme.palette.on_surface_variant,
                IconStyle::REGULAR,
            )]);
        let pills = axis_pills(theme, target, path, value, spec, enabled);
        rows.push(
            Element::row()
                .width(Length::Fill)
                .gap(6.0)
                .align(Align::Center)
                .children([lead, pills]),
        );
    }
    Element::column()
        .width(Length::Fill)
        .gap(6.0)
        .children(rows)
}

pub(crate) fn fields_group(
    ctx: &BuildContext<'_, Action>,
    scene: &Scene,
    target: &Target,
    prefix: &str,
    fields: &[Field],
    enabled: bool,
) -> Element<'static, Action> {
    let mut rows = Vec::new();
    for field in fields {
        if !visible(scene, target, prefix, field.name) {
            continue;
        }
        let path = if prefix.is_empty() {
            String::from(field.name)
        } else {
            format!("{prefix}.{}", field.name)
        };
        if let Some(row) = field_control(ctx, scene, target, &path, field, enabled) {
            rows.push(row);
        }
    }
    Element::column()
        .width(Length::Fill)
        .gap(6.0)
        .children(rows)
}

fn visible(scene: &Scene, target: &Target, prefix: &str, name: &str) -> bool {
    let text = |field: &str| -> String {
        let key = if prefix.is_empty() {
            String::from(field)
        } else {
            format!("{prefix}.{field}")
        };
        scene
            .property(target, &key)
            .and_then(|v| v.text().ok().map(String::from))
            .unwrap_or_default()
    };
    let flag = |field: &str| -> bool {
        let key = if prefix.is_empty() {
            String::from(field)
        } else {
            format!("{prefix}.{field}")
        };
        scene
            .property(target, &key)
            .and_then(|v| v.boolean().ok())
            .unwrap_or(false)
    };

    if matches!(target, Target::Material(_)) && prefix.is_empty() {
        return match name {
            "alpha" => text("alpha_mode") != "opaque",
            "alpha_cutoff" => text("alpha_mode") == "mask",
            "normal_scale" => !text("normal_image").is_empty(),
            "occlusion_strength" => !text("occlusion_image").is_empty(),
            _ => true,
        };
    }

    match prefix {
        "environment" => match name {
            "blend" => flag("local"),
            _ => true,
        },
        "collider" => {
            let shape = text("shape");
            match name {
                "mesh" => shape == "mesh",
                "radius" => matches!(shape.as_str(), "sphere" | "capsule"),
                "half_extents" => shape == "cuboid",
                "half_height" => shape == "capsule",
                _ => true,
            }
        }
        "body" => {
            let kind = text("kind");
            match name {
                "kind" => true,
                "linear_velocity" | "angular_velocity" => kind != "static",
                _ => kind == "dynamic",
            }
        }
        "joint" => {
            let kind = text("kind");
            let movable = kind != "fixed";
            match name {
                "axis" => matches!(kind.as_str(), "spherical" | "hinge" | "prismatic"),
                "rest_length" => kind == "distance",
                "cone" => kind == "spherical",
                "limit" | "motor" => movable,
                "lower" | "upper" => movable && flag("limit"),
                "motor_speed" | "motor_force" => movable && flag("motor"),
                _ => true,
            }
        }
        "audio_source" => match name {
            "min_distance" | "max_distance" => flag("spatial"),
            _ => true,
        },
        "audio_bus" => match name {
            "send_db" => !text("send").is_empty(),
            _ => true,
        },
        _ => true,
    }
}

fn field_control(
    ctx: &BuildContext<'_, Action>,
    scene: &Scene,
    target: &Target,
    path: &str,
    field: &Field,
    enabled: bool,
) -> Option<Element<'static, Action>> {
    let theme = ctx.theme();
    let value = scene.property(target, path)?;
    let title = display_label(target, field);
    let unit = unit_symbol(field.units());
    match field.kind {
        FieldKind::Number { range, speed } => {
            let held = value.number().ok()?;
            let dec = decimals(speed);
            let clean = if held.abs() < 0.5 * 10f32.powi(-(dec as i32)) {
                0.0
            } else {
                held
            };
            let span = range[1] - range[0];
            if span <= 360.0 {
                let readout = if unit.is_empty() {
                    format!("{:.*}", dec, clean)
                } else {
                    format!("{:.*}{unit}", dec, clean)
                };
                let t = target.clone();
                let p = String::from(path);
                Some(
                    widget::slider_readout(
                        theme,
                        Some(title),
                        held,
                        range[0]..=range[1],
                        false,
                        &readout,
                    )
                    .key(Key::new(format!("field.{target}.{path}")))
                    .enabled(enabled)
                    .on_slide(move |next| Action::Property {
                        target: t.clone(),
                        path: p.clone(),
                        value: Value::Number(next.clamp(range[0], range[1])),
                    }),
                )
            } else {
                let t = target.clone();
                let p = String::from(path);
                let scrub = widget::drag_value(theme, held, dec)
                    .key(Key::new(format!("field.{target}.{path}")))
                    .width(Length::Fill)
                    .unit(unit)
                    .enabled(enabled)
                    .on_drag_value(speed, move |next| Action::Property {
                        target: t.clone(),
                        path: p.clone(),
                        value: Value::Number(next.clamp(range[0], range[1])),
                    });
                Some(widget::form_row(theme, FORM_LABEL, title, scrub))
            }
        }
        FieldKind::Bool => {
            let held = value.boolean().ok()?;
            let t = target.clone();
            let p = String::from(path);
            Some(
                widget::toggle_row(theme, title, held)
                    .key(Key::new(format!("field.{target}.{path}")))
                    .enabled(enabled)
                    .on_toggle(move |next| Action::Property {
                        target: t.clone(),
                        path: p.clone(),
                        value: Value::Bool(next),
                    }),
            )
        }
        FieldKind::Text => {
            let held = String::from(value.text().ok()?);
            let t = target.clone();
            let p = String::from(path);
            let input = widget::text_field(
                &held,
                TextStyle::new(theme.font(Level::Body), theme.palette.on_surface),
                176.0,
                false,
            )
            .key(Key::new(format!("field.{target}.{path}")))
            .width(Length::Fill)
            .enabled(enabled)
            .on_commit(move |next| Action::Property {
                target: t.clone(),
                path: p.clone(),
                value: Value::Text(String::from(next)),
            });
            Some(widget::form_row(theme, FORM_LABEL, title, input))
        }
        FieldKind::Choice(options) => {
            let held = value
                .text()
                .map(String::from)
                .or_else(|_| value.number().map(|n| format!("{}", n.round() as i64)))
                .ok()?;
            let selected = options
                .iter()
                .position(|option| *option == held)
                .unwrap_or(0);
            let t = target.clone();
            let p = String::from(path);
            let choices: Vec<&'static str> = options.to_vec();
            let control = if options.len() <= 3 {
                widget::segmented_fill(theme, options, selected)
                    .key(Key::new(format!("field.{target}.{path}")))
                    .enabled(enabled)
                    .on_segment(move |at| Action::Property {
                        target: t.clone(),
                        path: p.clone(),
                        value: Value::Text(String::from(choices[at])),
                    })
            } else {
                let labels: Vec<String> = options.iter().map(|s| String::from(*s)).collect();
                widget::combo(
                    theme,
                    Key::new(format!("field.{target}.{path}")),
                    Length::Fill,
                    200.0,
                    &labels,
                    selected,
                    move |at| Action::Property {
                        target: t.clone(),
                        path: p.clone(),
                        value: Value::Text(String::from(choices[at])),
                    },
                )
                .enabled(enabled)
            };
            Some(widget::form_row(theme, FORM_LABEL, title, control))
        }
        FieldKind::Vector { range, speed } | FieldKind::Rotation { range, speed } => {
            let held = value.vector().ok()?;
            let pills = axis_pills(
                theme,
                target,
                path,
                held,
                AxisSpec { range, speed, unit },
                enabled,
            );
            Some(Element::column().width(Length::Fill).gap(4.0).children([
                label(
                    title,
                    TextStyle::new(theme.label_font(), theme.palette.on_surface_variant),
                ),
                pills,
            ]))
        }
        FieldKind::Color | FieldKind::Tint => {
            let rgba = value.rgba().ok()?;
            let rgb = [rgba.x, rgba.y, rgba.z];
            Some(color_row(theme, target, path, title, rgb, enabled))
        }
        FieldKind::Bits { count } => {
            let held = value.number().ok()? as u32;
            Some(bits_grid(theme, target, path, title, held, count, enabled))
        }
        FieldKind::Reference(_) | FieldKind::OptionalReference(_) => {
            reference_row(theme, scene, target, path, field, value, enabled)
        }
    }
}

fn display_label<'a>(target: &Target, field: &'a Field) -> &'a str {
    if matches!(target, Target::Material(_)) {
        return match field.name {
            "base_color_image" => "Albedo Map",
            "metallic_roughness_image" => "Surface Map",
            "normal_image" => "Normal Map",
            "occlusion_image" => "Occlusion Map",
            "emissive_image" => "Emissive Map",
            _ => field.label,
        };
    }
    field.label
}

fn axis_pills(
    theme: &dome_ui::Theme,
    target: &Target,
    path: &str,
    value: Vec3,
    spec: AxisSpec<'_>,
    enabled: bool,
) -> Element<'static, Action> {
    let metrics = theme.metrics;
    let dec = decimals(spec.speed);
    let range = spec.range;
    let axes = [
        (0usize, "x", value.x, Axis::X.color()),
        (1usize, "y", value.y, Axis::Y.color()),
        (2usize, "z", value.z, Axis::Z.color()),
    ];
    let mut items = Vec::with_capacity(3);
    for (slot, suffix, held, accent) in axes {
        let axis_path = format!("{path}.{suffix}");
        let key = Key::new(format!("axis.{target}.{axis_path}"));
        let t = target.clone();
        let corners = Corners::group_horizontal(slot, 3, metrics.corner_sm, 3.0, false);
        items.push(
            widget::drag_value(theme, held, dec)
                .key(key)
                .width(Length::Fill)
                .shaped(corners)
                .accent(accent)
                .unit(spec.unit)
                .enabled(enabled)
                .on_drag_value(spec.speed, move |next| Action::Property {
                    target: t.clone(),
                    path: axis_path.clone(),
                    value: Value::Number(next.clamp(range[0], range[1])),
                }),
        );
    }
    Element::row()
        .width(Length::Fill)
        .gap(metrics.group_gap)
        .align(Align::Center)
        .children(items)
}

fn color_row(
    theme: &dome_ui::Theme,
    target: &Target,
    path: &str,
    title: &str,
    rgb: [f32; 3],
    enabled: bool,
) -> Element<'static, Action> {
    let anchor_key = Key::new(format!("color.swatch.{target}.{path}"));
    let swatch = widget::color_swatch(rgb, 24.0)
        .key(anchor_key)
        .width(Length::Fixed(52.0))
        .height(Length::Fixed(24.0))
        .enabled(enabled)
        .hit_target();

    let picker_target = target.clone();
    let picker_path = String::from(path);
    let picker = widget::color_picker(rgb)
        .key(Key::new(format!("color.picker.{target}.{path}")))
        .on_color(move |picked| Action::Property {
            target: picker_target.clone(),
            path: picker_path.clone(),
            value: Value::Vector(Vec3::from_array(picked)),
        });

    let channels = axis_pills(
        theme,
        target,
        path,
        Vec3::from_array(rgb),
        AxisSpec {
            range: [0.0, 1.0],
            speed: 0.005,
            unit: "",
        },
        enabled,
    )
    .width(Length::Fixed(184.0));

    let popover = popup(anchor_key, 200.0, [picker, channels])
        .key(Key::new(format!("color.popup.{target}.{path}")))
        .persistent();

    let trailing = Element::row()
        .width(Length::Fill)
        .justify(Align::End)
        .align(Align::Center)
        .children([swatch, popover]);

    widget::form_row(theme, FORM_LABEL, title, trailing)
}

fn bits_grid(
    theme: &dome_ui::Theme,
    target: &Target,
    path: &str,
    title: &str,
    held: u32,
    count: u8,
    enabled: bool,
) -> Element<'static, Action> {
    let total = usize::from(count);
    let per_row = 8usize;
    let mut rows = Vec::new();
    let mut start = 0usize;
    while start < total {
        let end = (start + per_row).min(total);
        let len = end - start;
        let mut chips = Vec::with_capacity(len);
        for (offset, bit) in (start..end).enumerate() {
            let active = (held >> bit) & 1 == 1;
            let role = if active {
                Role::PrimaryContainer
            } else {
                Role::SurfaceHigh
            };
            let corners = Corners::group_horizontal(offset, len, 8.0, 3.0, active);
            let t = target.clone();
            let p = String::from(path);
            let next_mask = if active {
                held & !(1u32 << bit)
            } else {
                held | (1u32 << bit)
            };
            chips.push(
                widget::button_width(theme, &format!("{bit}"), role, Length::Fill)
                    .key(Key::new(format!("bit.{target}.{path}.{bit}")))
                    .height(Length::Fixed(24.0))
                    .shaped(corners)
                    .enabled(enabled)
                    .press(Action::Property {
                        target: t,
                        path: p,
                        value: Value::Number(next_mask as f32),
                    }),
            );
        }
        rows.push(
            Element::row()
                .width(Length::Fill)
                .gap(theme.metrics.group_gap)
                .children(chips),
        );
        start = end;
    }
    Element::column()
        .width(Length::Fill)
        .gap(4.0)
        .children([label(
            title,
            TextStyle::new(theme.label_font(), theme.palette.on_surface_variant),
        )])
        .children(rows)
}

fn reference_row(
    theme: &dome_ui::Theme,
    scene: &Scene,
    target: &Target,
    path: &str,
    field: &Field,
    value: Value,
    enabled: bool,
) -> Option<Element<'static, Action>> {
    let title = display_label(target, field);
    let (kind, optional) = match field.kind {
        FieldKind::Reference(kind) => (kind, false),
        FieldKind::OptionalReference(kind) => (kind, true),
        _ => return None,
    };
    let held = String::from(value.text().ok()?);
    let mut ids = references(scene, kind);
    if matches!(kind, ReferenceKind::Node | ReferenceKind::AudioBus)
        && let Target::Node(self_id) = target
    {
        ids.retain(|candidate| candidate != self_id);
    }
    let mut labels = Vec::with_capacity(ids.len() + usize::from(optional));
    let mut values = Vec::with_capacity(ids.len() + usize::from(optional));
    if optional {
        labels.push(String::from("—"));
        values.push(String::new());
    }
    for id in ids {
        labels.push(id.clone());
        values.push(id);
    }
    if labels.is_empty() {
        return None;
    }
    let selected = values.iter().position(|id| *id == held).unwrap_or(0);
    let t = target.clone();
    let p = String::from(path);
    let combo = widget::combo(
        theme,
        Key::new(format!("ref.{target}.{path}")),
        Length::Fill,
        210.0,
        &labels,
        selected,
        move |at| Action::Property {
            target: t.clone(),
            path: p.clone(),
            value: Value::Text(values[at].clone()),
        },
    )
    .enabled(enabled);
    Some(widget::form_row(theme, FORM_LABEL, title, combo))
}

fn decimals(speed: f32) -> usize {
    if speed >= 1.0 {
        0
    } else if speed >= 0.1 {
        1
    } else if speed >= 0.01 {
        2
    } else {
        3
    }
}

fn unit_symbol(units: Option<&'static str>) -> &'static str {
    match units {
        Some("degrees") => "°",
        Some("dB") => " dB",
        Some("Hz") => " Hz",
        Some("ms") => " ms",
        Some("m") => " m",
        _ => "",
    }
}

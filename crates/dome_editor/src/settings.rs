use dome_ui::{BuildContext, Element, Icon, Length, Role, TextStyle, widget};

use crate::message::Message;
use crate::shell::{View, section, text};

pub(crate) fn panel<'a>(ctx: &BuildContext<'_, Message>, view: &View<'a>) -> Element<'a, Message> {
    let theme = ctx.theme();
    let style = TextStyle::new(theme.font(dome_ui::Level::Caption), theme.palette.outline);
    let path = view.path.display().to_string();
    let children = vec![
        widget::toggle_row(theme, "Debug", view.debug).on_toggle(Message::Debug),
        text(&format!("file: {path}"), style),
        text(
            &format!(
                "{} nodes  {} meshes  {} materials",
                view.scene.nodes().len(),
                view.scene.meshes().len(),
                view.scene.materials().len()
            ),
            style,
        ),
        Element::row()
            .width(Length::Fill)
            .height(Length::Fixed(30.0))
            .gap(6.0)
            .children([
                widget::icon(Icon::Save, 15.0, theme.palette.primary, Default::default()),
                text(
                    "Save scene",
                    TextStyle::new(theme.font(dome_ui::Level::Label), theme.palette.primary),
                ),
            ])
            .press(Message::Save),
        Element::row()
            .width(Length::Fill)
            .height(Length::Fixed(30.0))
            .gap(6.0)
            .children([
                widget::icon(
                    Icon::Reset,
                    15.0,
                    theme.palette.on_surface_variant,
                    Default::default(),
                ),
                text(
                    "Reload from disk",
                    TextStyle::new(
                        theme.font(dome_ui::Level::Label),
                        theme.palette.on_surface_variant,
                    ),
                ),
            ])
            .press(Message::Reload),
        Element::row()
            .width(Length::Fill)
            .height(Length::Fixed(30.0))
            .gap(6.0)
            .children([
                widget::icon(
                    Icon::Target,
                    15.0,
                    theme.palette.on_surface_variant,
                    Default::default(),
                ),
                text(
                    "Focus selection  (F)",
                    TextStyle::new(
                        theme.font(dome_ui::Level::Label),
                        theme.palette.on_surface_variant,
                    ),
                ),
            ])
            .press(Message::Focus),
        widget::button(theme, "Settings", Role::Standard),
    ];
    section(ctx, "Settings", children)
}

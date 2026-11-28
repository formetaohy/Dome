use dome_math::{Rect, Vec2};
use dome_ui::{
    Align, BuildContext, Corners, Edges, Element, Icon, IconStyle, Key, Layer, Length, Level, Role,
    State as UiState, Surface, TextStyle, place, widget,
};

use crate::action::{Action, Confirm, PathDialog, PathKind};
use crate::message::{Kind as MessageKind, Message};
use crate::stage::label;

const DIALOG_WIDTH: f32 = 360.0;
const DIALOG_HEIGHT: f32 = 166.0;
const CONFIRM_HEIGHT: f32 = 196.0;

pub(crate) fn toast(
    ctx: &mut BuildContext<'_, Action>,
    screen: Vec2,
    message: &Message,
) -> Element<'static, Action> {
    let theme = ctx.theme();
    let palette = theme.palette;
    let (container, ink, symbol) = match message.kind() {
        MessageKind::Error => (
            palette.error_container,
            palette.on_error_container,
            Icon::Warning,
        ),
        MessageKind::Notice => (
            palette.primary_container,
            palette.on_primary_container,
            Icon::Info,
        ),
    };
    let text_style = TextStyle::new(theme.font(Level::Label), ink).truncating();
    let max_text = (screen.x - 120.0).max(120.0);
    let text_width = (ctx.measure(message.text(), text_style).x + 2.0).min(max_text);
    let pill = Element::row()
        .key(Key::new("toast.pill"))
        .height(Length::Fixed(40.0))
        .padding(Edges::symmetric(14.0, 0.0))
        .gap(10.0)
        .align(Align::Center)
        .blocks_pointer()
        .background(Surface::fill(container).with_corners(Corners::capsule()))
        .children([
            widget::icon(symbol, 18.0, ink, IconStyle::FILLED),
            label(message.text(), text_style).width(Length::Fixed(text_width)),
            widget::icon_button(Icon::Close, 26.0, Role::Standard, "Dismiss")
                .key(Key::new("toast.close"))
                .press(Action::DismissMessage),
        ]);

    place(
        Rect::from_min_size(
            Vec2::new(0.0, (screen.y - 56.0).max(12.0)),
            Vec2::new(screen.x, 44.0),
        ),
        [Element::row()
            .width(Length::Fill)
            .height(Length::Fill)
            .justify(Align::Center)
            .align(Align::Center)
            .children([pill])],
    )
    .key(Key::new("toast"))
    .layer(Layer::Hud)
}

pub(crate) fn confirm(
    ctx: &BuildContext<'_, Action>,
    screen: Vec2,
    confirm: Confirm,
    can_save: bool,
) -> Element<'static, Action> {
    let theme = ctx.theme();
    let palette = theme.palette;
    let heading = Element::row()
        .width(Length::Fill)
        .gap(12.0)
        .align(Align::Center)
        .children([
            Element::column()
                .width(Length::Fixed(36.0))
                .height(Length::Fixed(36.0))
                .align(Align::Center)
                .justify(Align::Center)
                .background(Surface::fill(palette.error_container).with_corners(Corners::all(12.0)))
                .children([widget::icon(
                    Icon::Warning,
                    18.0,
                    palette.on_error_container,
                    IconStyle::FILLED,
                )]),
            label(
                confirm.title(),
                TextStyle::new(theme.font(Level::Headline), palette.on_surface),
            ),
        ]);
    let body = label(
        confirm.body(),
        TextStyle::new(theme.font(Level::Body), palette.on_surface_variant),
    )
    .width(Length::Fill);
    let mut choices = vec![
        widget::button(theme, "Cancel", Role::Standard)
            .key(Key::new("confirm.cancel"))
            .press(Action::DismissConfirm),
    ];
    if let Some(label) = confirm.discard_label() {
        choices.push(
            widget::button(theme, label, Role::Destructive)
                .key(Key::new("confirm.discard"))
                .press(Action::DiscardConfirm),
        );
    }
    if !confirm.saves() || can_save {
        choices.push(
            widget::button(theme, confirm.accept_label(), Role::Filled)
                .key(Key::new("confirm.accept"))
                .press(Action::ResolveConfirm),
        );
    }
    let actions = Element::row()
        .width(Length::Fill)
        .justify(Align::End)
        .align(Align::Center)
        .gap(8.0)
        .children(choices);
    let card_rect = Rect::from_center_size(
        screen * 0.5,
        Vec2::new(
            DIALOG_WIDTH.min((screen.x - 24.0).max(220.0)),
            CONFIRM_HEIGHT,
        ),
    );
    let card = place(
        card_rect,
        [Element::column()
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Edges::all(20.0))
            .gap(14.0)
            .background(theme.surface(Role::Dialog, UiState::Rest))
            .blocks_pointer()
            .children([heading, body, actions])],
    )
    .key(Key::new("confirm.card"));
    place(Rect::from_min_size(Vec2::ZERO, screen), [card])
        .key(Key::new("confirm"))
        .layer(Layer::Hud)
        .background(Surface::fill(palette.scrim))
        .blocks_pointer()
        .modal()
}

pub(crate) fn modal(
    ctx: &BuildContext<'_, Action>,
    screen: Vec2,
    dialog: &PathDialog,
) -> Element<'static, Action> {
    let theme = ctx.theme();
    let palette = theme.palette;
    let (symbol, title) = match dialog.kind {
        PathKind::Model => (Icon::Export, "Model"),
        PathKind::Prefab => (Icon::Folder, "Prefab"),
        PathKind::Image => (Icon::Camera, "Image"),
        PathKind::Sound => (Icon::Channel, "Sound"),
        PathKind::Panorama => (Icon::Globe, "Panorama"),
    };
    let badge = Element::column()
        .width(Length::Fixed(36.0))
        .height(Length::Fixed(36.0))
        .align(Align::Center)
        .justify(Align::Center)
        .background(Surface::fill(palette.secondary_container).with_corners(Corners::all(12.0)))
        .children([widget::icon(
            symbol,
            18.0,
            palette.on_secondary_container,
            IconStyle::FILLED,
        )]);
    let heading = Element::row()
        .width(Length::Fill)
        .gap(12.0)
        .align(Align::Center)
        .children([
            badge,
            label(
                title,
                TextStyle::new(theme.font(Level::Headline), palette.on_surface),
            ),
            Element::row()
                .height(Length::Fixed(24.0))
                .padding(Edges::symmetric(11.0, 0.0))
                .align(Align::Center)
                .background(
                    Surface::fill(palette.surface_container_highest)
                        .with_corners(Corners::capsule()),
                )
                .children([label(
                    format!("{}/", dialog.kind.folder().name()),
                    TextStyle::new(theme.font(Level::Monospace), palette.on_surface_variant),
                )]),
        ]);

    let field_width = DIALOG_WIDTH - 40.0;
    let field = widget::text_field(
        &dialog.input,
        TextStyle::new(theme.font(Level::Body), palette.on_surface),
        field_width,
        false,
    )
    .key(Key::new("dialog.path"))
    .editing(dialog.focus)
    .error(dialog.invalid)
    .on_field(
        |value| Action::EditDialog(String::from(value)),
        |value| Action::SubmitPath(String::from(value)),
    )
    .on_cancel(|| Action::CloseDialog);
    let field = match dialog.focus {
        true => field.select_all(),
        false => field,
    };

    let actions = Element::row()
        .width(Length::Fill)
        .justify(Align::End)
        .align(Align::Center)
        .gap(8.0)
        .children([
            widget::button(theme, "Cancel", Role::Standard)
                .key(Key::new("dialog.cancel"))
                .press(Action::CloseDialog),
            widget::action_button(theme, Icon::Check, "Open", Role::Filled)
                .key(Key::new("dialog.confirm"))
                .press(Action::SubmitDialog),
        ]);

    let card_rect = Rect::from_center_size(
        screen * 0.5,
        Vec2::new(
            DIALOG_WIDTH.min((screen.x - 24.0).max(220.0)),
            DIALOG_HEIGHT,
        ),
    );
    let card = place(
        card_rect,
        [Element::column()
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Edges::all(20.0))
            .gap(14.0)
            .background(theme.surface(Role::Dialog, UiState::Rest))
            .blocks_pointer()
            .children([heading, field, actions])],
    )
    .key(Key::new("dialog.card"));

    place(Rect::from_min_size(Vec2::ZERO, screen), [card])
        .key(Key::new("dialog"))
        .layer(Layer::Hud)
        .background(Surface::fill(palette.scrim))
        .blocks_pointer()
        .modal()
}

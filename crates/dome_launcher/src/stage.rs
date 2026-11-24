use std::path::Path;

use dome_math::{Rect, Vec2};
use dome_project::{Catalog, Project};
use dome_ui::{
    Align, Axis, BuildContext, Corners, Cursor, Edges, Element, Icon, IconStyle, Key, Layer,
    Length, Role, State as UiState, Surface, TextStyle, place, text, widget,
};

use crate::action::Action;
use crate::launcher::Dialog;

const ROW: f32 = 78.0;
const CARD: f32 = 68.0;

pub(crate) struct State<'a> {
    pub(crate) catalog: &'a Catalog,
    pub(crate) dialog: Option<&'a Dialog>,
    pub(crate) error: Option<&'a str>,
}

pub(crate) fn build(
    ctx: &mut BuildContext<'_, Action>,
    state: &State<'_>,
) -> Element<'static, Action> {
    let screen = ctx.size();
    let full = Rect::from_min_size(Vec2::ZERO, screen);
    let mut layers = vec![body(ctx, state)];
    if let Some(dialog) = state.dialog {
        layers.push(modal(ctx, screen, dialog));
    }
    if let Some(error) = state.error {
        layers.push(toast(ctx, screen, error));
    }
    place(full, layers).key(Key::new("launcher"))
}

fn body(ctx: &mut BuildContext<'_, Action>, state: &State<'_>) -> Element<'static, Action> {
    let theme = *ctx.theme();
    let palette = theme.palette;
    let rows = state.catalog.entries().len();
    let content = if rows == 0 {
        Element::column()
            .width(Length::Fill)
            .height(Length::Fill)
            .align(Align::Center)
            .justify(Align::Center)
            .children([widget::empty_state(&theme, Icon::Folder, "No projects yet")])
    } else {
        ctx.scroll(
            Key::new("launcher.projects"),
            Axis::Column,
            Length::Fill,
            ROW,
            rows,
            |window| {
                (window.first..window.last)
                    .map(|index| {
                        let recent = &state.catalog.entries()[index];
                        Element::column()
                            .width(Length::Fill)
                            .height(Length::Fixed(ROW))
                            .children([card(&theme, index, recent.name(), recent.path())])
                    })
                    .collect()
            },
        )
        .height(Length::Fill)
    };
    Element::column()
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Edges::symmetric(32.0, 26.0))
        .gap(18.0)
        .background(Surface::fill(palette.surface_dim))
        .children([header(&theme), content, footer(&theme)])
}

fn header(theme: &dome_ui::Theme) -> Element<'static, Action> {
    let palette = theme.palette;
    Element::row()
        .width(Length::Fill)
        .gap(12.0)
        .align(Align::Center)
        .children([
            Element::row()
                .width(Length::Fixed(46.0))
                .height(Length::Fixed(46.0))
                .align(Align::Center)
                .justify(Align::Center)
                .background(
                    Surface::fill(palette.tertiary_container).with_corners(Corners::all(14.0)),
                )
                .children([widget::icon(
                    Icon::Cube,
                    22.0,
                    palette.on_tertiary_container,
                    IconStyle::FILLED,
                )]),
            Element::column().gap(1.0).children([
                text(
                    "Dome",
                    TextStyle::new(theme.font(dome_ui::Level::Display), palette.on_surface),
                ),
                text(
                    "PROJECTS",
                    TextStyle::new(theme.font(dome_ui::Level::Overline), palette.outline),
                ),
            ]),
        ])
}

fn footer(theme: &dome_ui::Theme) -> Element<'static, Action> {
    Element::row()
        .width(Length::Fill)
        .justify(Align::End)
        .align(Align::Center)
        .children([
            widget::action_button(theme, Icon::Folder, "Open folder", Role::Filled)
                .key(Key::new("launcher.choose"))
                .height(Length::Fixed(44.0))
                .press(Action::Choose),
        ])
}

fn card(theme: &dome_ui::Theme, index: usize, name: &str, path: &Path) -> Element<'static, Action> {
    let palette = theme.palette;
    let base = theme.surface(Role::Card, UiState::Rest);
    let hover = Surface {
        fill: dome_ui::Fill::Solid(palette.surface_container_high),
        ..base
    };
    Element::row()
        .key(Key::index("launcher.project", index))
        .width(Length::Fill)
        .height(Length::Fixed(CARD))
        .padding(Edges::symmetric(16.0, 0.0))
        .gap(14.0)
        .align(Align::Center)
        .background(base)
        .hover_highlight(hover)
        .cursor(Cursor::Pointing)
        .press(Action::Open(index))
        .children([
            Element::row()
                .width(Length::Fixed(44.0))
                .height(Length::Fixed(44.0))
                .align(Align::Center)
                .justify(Align::Center)
                .background(
                    Surface::fill(palette.secondary_container).with_corners(Corners::all(13.0)),
                )
                .children([widget::icon(
                    Icon::Folder,
                    20.0,
                    palette.on_secondary_container,
                    IconStyle::FILLED,
                )]),
            Element::column().width(Length::Fill).gap(2.0).children([
                text(
                    String::from(name),
                    TextStyle::new(theme.font(dome_ui::Level::Title), palette.on_surface)
                        .truncating(),
                ),
                text(
                    path.display().to_string(),
                    TextStyle::new(theme.font(dome_ui::Level::Caption), palette.outline)
                        .truncating(),
                ),
            ]),
            widget::icon_button(Icon::Close, 28.0, Role::Standard, "Forget project")
                .key(Key::index("launcher.forget", index))
                .press(Action::Forget(index)),
        ])
}

fn modal(
    ctx: &mut BuildContext<'_, Action>,
    screen: Vec2,
    dialog: &Dialog,
) -> Element<'static, Action> {
    let theme = *ctx.theme();
    let palette = theme.palette;
    let width = (screen.x - 96.0).clamp(360.0, 560.0);
    let verdict = verdict(&dialog.input);
    let (symbol, tint, status, label) = match verdict {
        Verdict::Empty => (
            Icon::Folder,
            palette.outline,
            String::from("Choose a project folder"),
            "Open",
        ),
        Verdict::Open => (
            Icon::Folder,
            palette.success,
            format!("Open {}", folder_name(&dialog.input)),
            "Open",
        ),
        Verdict::Create => (
            Icon::Plus,
            palette.tertiary,
            format!("Create {}", folder_name(&dialog.input)),
            "Create",
        ),
        Verdict::Refuse => (
            Icon::Warning,
            palette.warning,
            String::from("Folder holds other files"),
            "Open",
        ),
    };
    let usable = matches!(verdict, Verdict::Open | Verdict::Create);
    let heading = Element::row()
        .width(Length::Fill)
        .gap(12.0)
        .align(Align::Center)
        .children([
            Element::row()
                .width(Length::Fixed(36.0))
                .height(Length::Fixed(36.0))
                .align(Align::Center)
                .justify(Align::Center)
                .background(
                    Surface::fill(palette.secondary_container).with_corners(Corners::all(12.0)),
                )
                .children([widget::icon(
                    Icon::Folder,
                    18.0,
                    palette.on_secondary_container,
                    IconStyle::FILLED,
                )]),
            text(
                "Project folder",
                TextStyle::new(theme.font(dome_ui::Level::Headline), palette.on_surface),
            ),
        ]);
    let field = widget::text_field(
        &dialog.input,
        TextStyle::new(theme.font(dome_ui::Level::Body), palette.on_surface),
        width - 48.0,
        false,
    )
    .key(Key::new("launcher.path"))
    .editing(dialog.focus)
    .error(dialog.invalid)
    .on_field(
        |value| Action::EditPath(String::from(value)),
        |value| Action::SubmitPath(String::from(value)),
    )
    .on_cancel(|| Action::CloseDialog);
    let status = Element::row()
        .width(Length::Fill)
        .height(Length::Fixed(20.0))
        .gap(8.0)
        .align(Align::Center)
        .children([
            widget::icon(symbol, 14.0, tint, IconStyle::REGULAR),
            text(
                status,
                TextStyle::new(theme.font(dome_ui::Level::Caption), tint),
            ),
        ]);
    let actions = Element::row()
        .width(Length::Fill)
        .justify(Align::End)
        .align(Align::Center)
        .gap(8.0)
        .children([
            widget::button(&theme, "Cancel", Role::Standard)
                .key(Key::new("launcher.cancel"))
                .press(Action::CloseDialog),
            widget::action_button(&theme, Icon::Check, label, Role::Filled)
                .key(Key::new("launcher.submit"))
                .enabled(usable)
                .press(Action::Submit),
        ]);
    let card = place(
        Rect::from_center_size(screen * 0.5, Vec2::new(width, 232.0)),
        [Element::column()
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Edges::all(24.0))
            .gap(14.0)
            .background(theme.surface(Role::Dialog, UiState::Rest))
            .blocks_pointer()
            .children([heading, field, status, actions])],
    )
    .key(Key::new("launcher.dialog.card"));
    place(Rect::from_min_size(Vec2::ZERO, screen), [card])
        .key(Key::new("launcher.dialog"))
        .layer(Layer::Hud)
        .background(Surface::fill(palette.scrim))
        .blocks_pointer()
        .modal()
}

fn toast(
    ctx: &mut BuildContext<'_, Action>,
    screen: Vec2,
    message: &str,
) -> Element<'static, Action> {
    let theme = *ctx.theme();
    let palette = theme.palette;
    let style = TextStyle::new(
        theme.font(dome_ui::Level::Label),
        palette.on_error_container,
    )
    .truncating();
    let max_text = (screen.x - 120.0).max(120.0);
    let text_width = (ctx.measure(message, style).x + 2.0).min(max_text);
    let pill = Element::row()
        .key(Key::new("launcher.toast.pill"))
        .height(Length::Fixed(40.0))
        .padding(Edges::symmetric(14.0, 0.0))
        .gap(10.0)
        .align(Align::Center)
        .blocks_pointer()
        .background(Surface::fill(palette.error_container).with_corners(Corners::capsule()))
        .children([
            widget::icon(
                Icon::Warning,
                18.0,
                palette.on_error_container,
                IconStyle::FILLED,
            ),
            text(String::from(message), style).width(Length::Fixed(text_width)),
            widget::icon_button(Icon::Close, 26.0, Role::Standard, "Dismiss")
                .key(Key::new("launcher.toast.close"))
                .press(Action::DismissError),
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
    .key(Key::new("launcher.toast"))
    .layer(Layer::Hud)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Empty,
    Open,
    Create,
    Refuse,
}

fn verdict(input: &str) -> Verdict {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Verdict::Empty;
    }
    let path = Path::new(trimmed);
    if Project::holds(path) {
        Verdict::Open
    } else if Project::vacant(path) && path.file_name().is_some() {
        Verdict::Create
    } else {
        Verdict::Refuse
    }
}

fn folder_name(input: &str) -> String {
    Path::new(input.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| String::from(input.trim()), String::from)
}

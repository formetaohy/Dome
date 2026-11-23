use dome_scene::Target;
use dome_ui::{
    Align, BuildContext, Corners, Element, Icon, Key, Length, Role, State, TextStyle, widget,
};

use crate::message::Message;
use crate::shell::{View, section, text};

pub(crate) fn panel<'a>(ctx: &BuildContext<'_, Message>, view: &View<'a>) -> Element<'a, Message> {
    let mut children = vec![create(ctx, view)];
    children.push(import(ctx, view));
    for asset in Asset::ALL {
        children.push(list(ctx, view, asset));
    }
    Element::column()
        .width(Length::Fill)
        .gap(8.0)
        .children([section(ctx, "Library", children)])
}

#[derive(Clone, Copy)]
enum Asset {
    Mesh,
    Material,
    Image,
    Model,
    Sound,
    Environment,
    Graph,
}

impl Asset {
    const ALL: [Self; 7] = [
        Self::Mesh,
        Self::Material,
        Self::Image,
        Self::Model,
        Self::Sound,
        Self::Environment,
        Self::Graph,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Mesh => "mesh",
            Self::Material => "material",
            Self::Image => "image",
            Self::Model => "model",
            Self::Sound => "sound",
            Self::Environment => "environment",
            Self::Graph => "graph",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Mesh => "Meshes",
            Self::Material => "Materials",
            Self::Image => "Images",
            Self::Model => "Models",
            Self::Sound => "Sounds",
            Self::Environment => "Environments",
            Self::Graph => "Graphs",
        }
    }

    fn entries(self, view: &View<'_>) -> Vec<(String, Target)> {
        let scene = view.scene;
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
            Self::Environment => scene
                .environments()
                .iter()
                .map(|entry| (entry.id.clone(), Target::Environment(entry.id.clone())))
                .collect(),
            Self::Graph => scene
                .graphs()
                .iter()
                .map(|entry| (entry.id.clone(), Target::Graph(entry.id.clone())))
                .collect(),
        }
    }
}

fn texture_of(target: &Target) -> Option<String> {
    crate::shell::texture_of(target)
}

fn create<'a>(_ctx: &BuildContext<'_, Message>, _view: &View<'a>) -> Element<'a, Message> {
    let kinds = [
        (Icon::Cube, "mesh", "Mesh"),
        (Icon::Sliders, "material", "Material"),
        (Icon::Folder, "image", "Image"),
        (Icon::Film, "sound", "Sound"),
        (Icon::Globe, "environment", "Environment"),
        (Icon::Chart, "graph", "Graph"),
    ];
    let mut buttons = Vec::new();
    for (icon, kind, tooltip) in kinds {
        buttons.push(
            widget::icon_button(icon, 28.0, Role::Standard, tooltip).press(Message::AddAsset(kind)),
        );
    }
    Element::row()
        .width(Length::Fill)
        .gap(4.0)
        .children(buttons)
}

fn import<'a>(ctx: &BuildContext<'_, Message>, view: &View<'a>) -> Element<'a, Message> {
    let theme = ctx.theme();
    let style = TextStyle::new(theme.font(dome_ui::Level::Body), theme.palette.on_surface);
    let width = 150.0;
    let import = widget::text_field(view.import_path, style, width, false)
        .on_commit(|value| Message::ImportPath(String::from(value)));
    let button = widget::button(theme, "Import", Role::Tonal).press(Message::Import);
    let instance_field = widget::text_field(view.prefab_path, style, width, false)
        .on_commit(|value| Message::PrefabPath(String::from(value)));
    let instance = widget::button(theme, "Instance", Role::Tonal).press(Message::Instance);
    Element::column().width(Length::Fill).gap(5.0).children([
        Element::row()
            .width(Length::Fill)
            .gap(5.0)
            .align(Align::Center)
            .children([import, button]),
        Element::row()
            .width(Length::Fill)
            .gap(5.0)
            .align(Align::Center)
            .children([instance_field, instance]),
    ])
}

fn list<'a>(
    ctx: &BuildContext<'_, Message>,
    view: &View<'a>,
    asset: Asset,
) -> Element<'a, Message> {
    let theme = ctx.theme();
    let palette = theme.palette;
    let title = asset.title();
    let entries = asset.entries(view);
    if entries.is_empty() {
        return Element::column()
            .width(Length::Fill)
            .gap(2.0)
            .children([text(
                title,
                TextStyle::new(
                    theme.font(dome_ui::Level::Label),
                    palette.on_surface_variant,
                ),
            )])
            .children([text(
                "none",
                TextStyle::new(theme.font(dome_ui::Level::Caption), palette.outline),
            )]);
    }
    let mut rows = vec![text(
        title,
        TextStyle::new(
            theme.font(dome_ui::Level::Label),
            palette.on_surface_variant,
        ),
    )];
    for (id, target) in entries {
        let selected = view.selected == Some(&target);
        let background = if selected {
            theme.surface(Role::PrimaryContainer, State::Selected)
        } else {
            dome_ui::Surface::NONE
        };
        let mut row = vec![text(
            &id,
            TextStyle::new(
                theme.font(dome_ui::Level::Caption),
                if selected {
                    palette.on_primary_container
                } else {
                    palette.on_surface
                },
            ),
        )];
        if let Some(texture) = texture_of(&target) {
            row.insert(
                0,
                widget::thumbnail(
                    theme,
                    &texture,
                    Length::Fixed(16.0),
                    Length::Fixed(16.0),
                    Corners::all(theme.metrics.corner_xs),
                ),
            );
        }
        rows.push(
            Element::row()
                .key(Key::new(format!("library.{}.{id}", asset.name())))
                .width(Length::Fill)
                .height(Length::Fixed(24.0))
                .padding(dome_ui::Edges::symmetric(6.0, 0.0))
                .gap(6.0)
                .align(Align::Center)
                .background(background)
                .hover_highlight(theme.surface(Role::Standard, State::Hover))
                .children(row)
                .press(Message::Select(target)),
        );
    }
    Element::column()
        .width(Length::Fill)
        .gap(1.0)
        .children(rows)
}

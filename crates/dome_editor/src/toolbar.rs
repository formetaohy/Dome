use dome_ui::{
    Align, BuildContext, Corners, Edges, Element, Icon, Key, Layer, Length, Role, Surface, widget,
};

use crate::action::{Action, Sheet};
use crate::gizmo::{Mode, Space};
use crate::stage::State;

pub(crate) fn build(ctx: &BuildContext<'_, Action>, state: &State<'_>) -> Element<'static, Action> {
    let theme = *ctx.theme();
    let palette = theme.palette;
    let metrics = theme.metrics;

    let outliner_role = if state.outliner_open {
        Role::PrimaryContainer
    } else {
        Role::Standard
    };
    let outliner = widget::icon_button(Icon::Sidebar, 32.0, outliner_role, "Outliner")
        .key(Key::new("toolbar.outliner"))
        .press(Action::ToggleSheet(Sheet::Outliner));

    let mode_index = match state.mode {
        Mode::Translate => 0,
        Mode::Rotate => 1,
        Mode::Scale => 2,
    };
    let modes = widget::icon_segmented(
        &theme,
        &[
            (Icon::Fit, "Move (W)"),
            (Icon::Reset, "Rotate (E)"),
            (Icon::WindowMaximize, "Scale (R)"),
        ],
        mode_index,
    )
    .key(Key::new("toolbar.mode"))
    .on_segment(|at| {
        Action::Mode(match at {
            0 => Mode::Translate,
            1 => Mode::Rotate,
            _ => Mode::Scale,
        })
    });

    let (space_icon, space_role, space_tip) = match state.space {
        Space::World => (Icon::Globe, Role::Standard, "World Space (Q)"),
        Space::Local => (Icon::Cube, Role::Tonal, "Local Space (Q)"),
    };
    let space = widget::icon_button(space_icon, 32.0, space_role, space_tip)
        .key(Key::new("toolbar.space"))
        .press(Action::ToggleSpace);

    let debug_role = if state.debug {
        Role::Tonal
    } else {
        Role::Standard
    };
    let debug = widget::icon_button(Icon::Grid, 32.0, debug_role, "Physics Overlay")
        .key(Key::new("toolbar.debug"))
        .press(Action::ToggleDebug);

    let history = Element::row()
        .gap(metrics.group_gap)
        .align(Align::Center)
        .children([
            widget::icon_button(Icon::Undo, 32.0, Role::SurfaceHigh, "Undo (Ctrl+Z)")
                .key(Key::new("toolbar.undo"))
                .shaped(Corners::group_horizontal(
                    0,
                    2,
                    Corners::CAPSULE,
                    metrics.corner_xs,
                    false,
                ))
                .enabled(state.can_undo)
                .press(Action::Undo),
            widget::icon_button(Icon::Redo, 32.0, Role::SurfaceHigh, "Redo (Ctrl+Y)")
                .key(Key::new("toolbar.redo"))
                .shaped(Corners::group_horizontal(
                    1,
                    2,
                    Corners::CAPSULE,
                    metrics.corner_xs,
                    false,
                ))
                .enabled(state.can_redo)
                .press(Action::Redo),
        ]);

    let save_role = if state.dirty {
        Role::PrimaryContainer
    } else {
        Role::Standard
    };
    let save = widget::icon_button(Icon::Save, 32.0, save_role, "Save (Ctrl+S)")
        .key(Key::new("toolbar.save"))
        .press(Action::Save);

    let inspector_role = if state.inspector_open && state.selected.is_some() {
        Role::PrimaryContainer
    } else {
        Role::Standard
    };
    let inspector = widget::icon_button(Icon::Sliders, 32.0, inspector_role, "Inspector")
        .key(Key::new("toolbar.inspector"))
        .enabled(state.selected.is_some())
        .press(Action::ToggleSheet(Sheet::Inspector));

    let bar_surface =
        Surface::fill(palette.surface_container_high).with_corners(Corners::capsule());
    let island = Element::row()
        .key(Key::new("toolbar"))
        .layer(Layer::Chrome)
        .height(Length::Fixed(44.0))
        .padding(Edges::symmetric(6.0, 6.0))
        .gap(6.0)
        .align(Align::Center)
        .background(bar_surface)
        .children([outliner, modes, space, debug, history, save, inspector]);

    let run = if state.playing {
        widget::icon_button(Icon::Pause, 44.0, Role::Error, "Stop (F5)")
            .key(Key::new("toolbar.run"))
            .shaped(Corners::all(14.0))
            .filled(true)
            .press(Action::Stop)
    } else {
        widget::icon_button(Icon::Play, 44.0, Role::Filled, "Play (F5)")
            .key(Key::new("toolbar.run"))
            .shaped(Corners::capsule())
            .filled(true)
            .press(Action::Play)
    };

    Element::row()
        .gap(8.0)
        .align(Align::Center)
        .children([island, run])
}

//! What every settings panel and dialog is built from, so they all look and behave the same:
//! the frame (a header, a rail of tabs with icons, a scrolling body, a footer) and the dialog (the
//! same without the rail), groups of labelled rows, and the pickers for line width, line style and
//! height.
//!
//! They are plain functions returning elements, like [`super::widgets`]. A panel owns its state
//! and passes it in; the callbacks say what the user picked. A panel is shown in
//! [`super::modal`], which sizes it, so the frame fills the space it is given.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, Div, ElementId, Entity, Rgba, SharedString, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Sizable, StyledExt as _};

use super::connection::ui::icon_colored;
use super::{theme, widgets};

/// A tab of the rail.
#[derive(Clone, Copy)]
pub struct Tab {
    pub label: &'static str,
    pub icon: IconName,
}

/// What the header of a panel says: what is being edited, and of what.
pub struct Head {
    pub icon: IconName,
    pub title: SharedString,
    pub subtitle: SharedString,
}

/// The width of the rail of tabs.
const RAIL_WIDTH: f32 = 176.0;

/// The header of a panel: its icon, what it is about, and the close button.
fn header(head: Head, on_close: impl Fn(&mut Window, &mut App) + 'static) -> Div {
    div()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .h(px(56.))
        .px_4()
        .border_b_1()
        .border_color(theme::border_hairline())
        .child(
            div()
                .flex_none()
                .size(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_lg()
                .bg(theme::accent_selected())
                .child(icon_colored(head.icon, 17., theme::fg())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(14.))
                        .font_semibold()
                        .text_color(theme::fg())
                        .truncate()
                        .child(head.title),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::muted_fg())
                        .truncate()
                        .child(head.subtitle),
                ),
        )
        .child(
            Button::new("settings-close")
                .cursor_pointer()
                .ghost()
                .compact()
                .icon(IconName::X)
                .tooltip("Close (Esc)")
                .on_click(move |_, window, cx| on_close(window, cx)),
        )
}

/// The card everything sits in: it fills the modal, with rounded corners and a border.
fn shell() -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded_xl()
        .border_1()
        .border_color(theme::border_subtle())
        .bg(theme::bg())
        .shadow_2xl()
        .text_color(theme::fg())
}

/// The frame of a panel. `on_tab` gets the index of the tab clicked and `on_close` the close
/// button of the header.
pub fn frame(
    head: Head,
    tabs: &[Tab],
    active: usize,
    on_tab: impl Fn(usize, &mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    body: impl IntoElement,
    footer: impl IntoElement,
) -> Div {
    let on_tab = Rc::new(on_tab);
    let mut rail = div()
        .flex_none()
        .w(px(RAIL_WIDTH))
        .flex()
        .flex_col()
        .gap_0p5()
        .p_2()
        .border_r_1()
        .border_color(theme::border_hairline())
        .bg(theme::fg_alpha(0.03));
    for (index, tab) in tabs.iter().enumerate() {
        let chosen = index == active;
        let on_tab = on_tab.clone();
        rail = rail.child(
            div()
                .id(("settings-tab", index))
                .flex()
                .flex_row()
                .items_center()
                .gap_2p5()
                .h(px(34.))
                .px_2p5()
                .rounded_md()
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(ink(chosen))
                .when(chosen, |el| el.bg(theme::accent_selected()))
                .when(!chosen, |el| el.hover(|s| s.bg(theme::surface_hover())))
                .on_click(move |_, window, cx| on_tab(index, window, cx))
                .child(icon_colored(tab.icon, 15., ink(chosen)))
                .child(tab.label),
        );
    }

    shell()
        .child(header(head, on_close))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .child(rail)
                .child(
                    div()
                        .id("settings-body")
                        .flex_1()
                        .min_w_0()
                        .overflow_y_scroll()
                        .p_4()
                        .child(body),
                ),
        )
        .child(footer)
}

/// A dialog: the frame without a rail of tabs, for what has one page (a list, a confirmation, a
/// short form).
pub fn dialog(
    head: Head,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    body: impl IntoElement,
    footer: impl IntoElement,
) -> Div {
    shell()
        .child(header(head, on_close))
        .child(
            div()
                .id("dialog-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_4()
                .child(body),
        )
        .child(footer)
}

/// The footer of a panel: what sits on the left (defaults, reset), what sits on the right
/// (cancel, OK).
pub fn footer(left: Vec<AnyElement>, right: Vec<AnyElement>) -> Div {
    div()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(56.))
        .px_4()
        .border_t_1()
        .border_color(theme::border_hairline())
        .children(left)
        .child(div().flex_1())
        .children(right)
}

/// A button of a footer, with an optional icon. `primary` is the one that confirms.
pub fn action(
    id: impl Into<ElementId>,
    label: &'static str,
    icon: Option<IconName>,
    primary: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> Button {
    let button = Button::new(id)
        .cursor_pointer()
        .small()
        .label(label)
        .on_click(move |_, window, cx| on_click(window, cx));
    let button = if primary {
        button.primary()
    } else {
        button.ghost()
    };
    match icon {
        Some(icon) => button.icon(icon),
        None => button,
    }
}

/// A titled group of rows, on a card. The rows are separated by hairlines.
pub fn group(
    icon: IconName,
    title: impl Into<SharedString>,
    rows: impl IntoIterator<Item = AnyElement>,
) -> Div {
    group_with(icon, title, None, rows)
}

/// A [`group`] with a control at the right of its title, such as the switch that turns the whole
/// group on or off.
pub fn group_with(
    icon: IconName,
    title: impl Into<SharedString>,
    control: Option<AnyElement>,
    rows: impl IntoIterator<Item = AnyElement>,
) -> Div {
    let mut card = div()
        .flex()
        .flex_col()
        .rounded_lg()
        .border_1()
        .border_color(theme::border_hairline())
        .bg(theme::fg_alpha(0.025))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(34.))
                .px_3()
                .border_b_1()
                .border_color(theme::border_hairline())
                .child(icon_colored(icon, 14., theme::muted_fg()))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(12.))
                        .font_semibold()
                        .text_color(theme::muted_fg())
                        .child(title.into()),
                )
                .children(control),
        );
    for (index, row) in rows.into_iter().enumerate() {
        card = card.child(
            div()
                .px_3()
                .when(index > 0, |el| {
                    el.border_t_1().border_color(theme::border_hairline())
                })
                .child(row),
        );
    }
    card
}

/// The stack a panel's groups go in.
pub fn page() -> Div {
    div().flex().flex_col().gap_3()
}

/// A row of a group: a label, a line of help under it when there is one, and the control at the
/// right.
pub fn field(
    label: impl Into<SharedString>,
    hint: Option<&'static str>,
    control: impl IntoElement,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_4()
        .min_h(px(42.))
        .py_1p5()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(theme::fg())
                        .child(label.into()),
                )
                .children(hint.map(|hint| {
                    div()
                        .text_size(px(11.))
                        .text_color(theme::muted_fg())
                        .child(hint)
                })),
        )
        .child(div().flex_none().child(control))
        .into_any_element()
}

/// A row for a whole element that has no label at the left (a table, a list of chips).
pub fn block(content: impl IntoElement) -> AnyElement {
    div().py_2p5().child(content).into_any_element()
}

/// A small title over a list, for the sections of what is inside a group.
pub fn caption(title: impl Into<SharedString>) -> Div {
    div()
        .px_2()
        .pt_2()
        .pb_1()
        .text_size(px(11.))
        .font_semibold()
        .text_color(theme::muted_fg())
        .child(title.into().to_uppercase())
}

/// A switch in the look of the panels: small, with the pointer of a button. The caller adds what
/// it needs (a label, `disabled`, the click).
pub fn switch(id: impl Into<ElementId>, on: bool) -> Switch {
    Switch::new(id).cursor_pointer().small().checked(on)
}

/// A switch for a row: `on_change` gets the new state.
pub fn toggle(
    id: impl Into<ElementId>,
    on: bool,
    on_change: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> Switch {
    switch(id, on).on_click(move |checked, window, cx| on_change(*checked, window, cx))
}

/// A text field of a row, `width` pixels wide, for a state made with [`InputState::new`].
pub fn text_field(state: &Entity<InputState>, width: f32) -> impl IntoElement {
    div().w(px(width)).child(Input::new(state).small())
}

/// A small icon, for a mark beside a name.
pub fn small_icon(icon: IconName, color: Rgba) -> gpui::Svg {
    icon_colored(icon, 13., color)
}

/// A line of muted text, for a note under a group or a state with nothing to show.
pub fn note(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.))
        .text_color(theme::muted_fg())
        .child(text.into())
}

/// A state with nothing to edit: an icon and a sentence, centered.
pub fn empty(icon: IconName, text: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .py_8()
        .child(icon_colored(icon, 22., theme::muted_fg()))
        .child(note(text))
}

/// One option of a picker: a box holding `glyph`, lit when chosen.
fn option_box(id: ElementId, chosen: bool, glyph: impl IntoElement) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(40.))
        .h(px(28.))
        .rounded_md()
        .border_1()
        .border_color(if chosen {
            theme::accent()
        } else {
            theme::border_subtle()
        })
        .cursor_pointer()
        .when(chosen, |el| el.bg(theme::accent_selected()))
        .when(!chosen, |el| el.hover(|s| s.bg(theme::surface_hover())))
        .child(glyph)
}

fn ink(chosen: bool) -> Rgba {
    if chosen {
        theme::fg()
    } else {
        theme::muted_fg()
    }
}

/// Line widths shown as lines of that thickness, one chosen (`None` when the value is none of
/// them).
pub fn width_picker(
    id: impl Into<ElementId>,
    widths: &[f32],
    selected: Option<usize>,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id: ElementId = id.into();
    let on_select = Rc::new(on_select);
    let mut row = div().flex().flex_row().gap_1();
    for (index, width) in widths.iter().enumerate() {
        let chosen = selected == Some(index);
        let on_select = on_select.clone();
        row = row.child(
            option_box(
                widgets::child_id(&id, index),
                chosen,
                div()
                    .w(px(20.))
                    .h(px(width.clamp(1.0, 6.0)))
                    .rounded_full()
                    .bg(ink(chosen)),
            )
            .tooltip(gpui_tooltip(format!("{width}")))
            .on_click(move |_, window, cx| on_select(index, window, cx)),
        );
    }
    row
}

/// A sample of a line style, drawn as shapes so every style sits on the same middle line and has
/// the same width (text dots sit on the baseline and drift off center). `style` is 0 for solid, 1
/// for dashed and 2 for dotted.
pub fn dash_glyph(style: usize, ink: Rgba) -> impl IntoElement {
    let row = div().flex().flex_row().items_center().justify_center();
    match style {
        0 => row.child(div().w(px(22.)).h(px(2.)).rounded_full().bg(ink)),
        1 => row
            .gap(px(3.))
            .children((0..3).map(|_| div().w(px(6.)).h(px(2.)).rounded_full().bg(ink))),
        _ => row
            .gap(px(4.))
            .children((0..4).map(|_| div().size(px(2.)).rounded_full().bg(ink))),
    }
}

/// The three line styles as samples, one chosen.
pub fn dash_picker(
    id: impl Into<ElementId>,
    selected: usize,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id: ElementId = id.into();
    let on_select = Rc::new(on_select);
    let mut row = div().flex().flex_row().gap_1();
    for (index, name) in ["Solid", "Dashed", "Dotted"].into_iter().enumerate() {
        let chosen = selected == index;
        let on_select = on_select.clone();
        row = row.child(
            option_box(
                widgets::child_id(&id, index),
                chosen,
                dash_glyph(index, ink(chosen)),
            )
            .tooltip(gpui_tooltip(name))
            .on_click(move |_, window, cx| on_select(index, window, cx)),
        );
    }
    row
}

/// Heights by name (a name and a weight against the rest), one chosen when `current` is the
/// weight of one of them. `on_pick` gets the weight.
pub fn height_picker(
    id: impl Into<ElementId>,
    presets: &'static [(&'static str, f32)],
    current: f32,
    on_pick: impl Fn(f32, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let names: Vec<&str> = presets.iter().map(|(name, _)| *name).collect();
    let selected = presets
        .iter()
        .position(|(_, weight)| (weight - current).abs() < 0.05)
        .unwrap_or(usize::MAX);
    widgets::segmented(id, &names, selected, move |choice, window, cx| {
        on_pick(presets[choice].1, window, cx);
    })
    .into_any_element()
}

/// A tooltip with `text`, in the shape the elements of gpui take.
fn gpui_tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let text: SharedString = text.into();
    move |window, cx| gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
}

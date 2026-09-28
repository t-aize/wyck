//! What every settings panel and dialog is built from, so they all look and behave the same:
//! the frame (a header, a rail of tabs with icons, a scrolling body, a footer) and the dialog (the
//! same without the rail), and groups of labelled rows.
//!
//! They are plain functions returning elements. A panel owns its state and passes it in; the
//! callbacks say what the user picked. A panel is shown in [`crate::modal`], which sizes it, so the
//! frame fills the space it is given.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, Div, Entity, MouseButton, SharedString, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable, StyledExt as _};

use crate::controls::ink;
use crate::{icon, modal, theme};

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

/// The header of a panel: its icon, what it is about, and the close button. Dragging it moves the
/// panel (see [`modal::begin_drag`]).
fn header(head: Head, on_close: impl Fn(&mut Window, &mut App) + 'static) -> Div {
    div()
        .flex_none()
        .cursor_grab()
        .on_mouse_down(MouseButton::Left, |event, _, cx| {
            modal::begin_drag(event.position, cx);
        })
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
                .child(icon::tinted(head.icon, 17., theme::fg())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(crate::tokens::text::TITLE))
                        .font_semibold()
                        .text_color(theme::fg())
                        .truncate()
                        .child(head.title),
                )
                .child(
                    div()
                        .text_size(px(crate::tokens::text::SMALL))
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
                .h(px(crate::tokens::height::LARGE))
                .px_2p5()
                .rounded_md()
                .cursor_pointer()
                .text_size(px(crate::tokens::text::EMPHASIS))
                .text_color(ink(chosen))
                .when(chosen, |el| el.bg(theme::accent_selected()))
                .when(!chosen, |el| el.hover(|s| s.bg(theme::surface_hover())))
                .on_click(move |_, window, cx| on_tab(index, window, cx))
                .child(icon::tinted(tab.icon, 15., ink(chosen)))
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
        .border_color(theme::border_subtle())
        .bg(theme::fg_alpha(0.025))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(crate::tokens::height::LARGE))
                .px_3()
                .border_b_1()
                .border_color(theme::border_hairline())
                .child(icon::tinted(icon, 14., theme::muted_fg()))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(crate::tokens::text::BODY))
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
    let row = Row::new(label);
    match hint {
        Some(hint) => row.hint(hint),
        None => row,
    }
    .control(control)
}

/// What a row does when the user asks to put its value back.
type OnReset = Rc<dyn Fn(&mut Window, &mut App)>;

/// A row of a group, with the parts a plain [`field`] does not have: a longer explanation behind
/// a help mark, and a reset button that shows while the value differs from the default.
///
/// ```ignore
/// Row::new("Width").hint("In pixels").help("How thick the line is drawn.")
///     .reset(width != default, move |w, cx| reset(w, cx))
///     .control(number::field(&state, tokens::field::NARROW))
/// ```
pub struct Row {
    label: SharedString,
    hint: Option<SharedString>,
    help: Option<SharedString>,
    reset: Option<(bool, OnReset)>,
}

impl Row {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            hint: None,
            help: None,
            reset: None,
        }
    }

    /// A short line under the label.
    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// A longer explanation, shown in a tooltip on the help mark after the label.
    pub fn help(mut self, help: impl Into<SharedString>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Shows a reset button while `modified` is true; `on_reset` puts the default back.
    pub fn reset(
        mut self,
        modified: bool,
        on_reset: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.reset = Some((modified, Rc::new(on_reset)));
        self
    }

    pub fn control(self, control: impl IntoElement) -> AnyElement {
        let modified = self.reset.as_ref().is_some_and(|(modified, _)| *modified);
        let mut title = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .child(
                div()
                    .text_size(px(crate::tokens::text::EMPHASIS))
                    .text_color(theme::fg())
                    .child(self.label),
            )
            .children(modified.then(|| {
                div()
                    .size(px(6.))
                    .rounded_full()
                    .bg(theme::accent())
                    .into_any_element()
            }));
        if let Some(help) = self.help {
            title = title.child(
                div()
                    .id("row-help")
                    .cursor_default()
                    .tooltip(crate::controls::tooltip(help))
                    .child(icon::tinted(IconName::Info, 12., theme::muted_fg())),
            );
        }
        let reset = self.reset.filter(|(modified, _)| *modified).map(|(_, f)| {
            div()
                .id("row-reset")
                .flex()
                .items_center()
                .justify_center()
                .size(px(crate::tokens::height::COMPACT))
                .rounded_md()
                .cursor_pointer()
                .hover(|s| s.bg(theme::surface_hover()))
                .tooltip(crate::controls::tooltip("Back to the default"))
                .on_click(move |_, window, cx| f(window, cx))
                .child(icon::tinted(IconName::RotateCcw, 13., theme::muted_fg()))
        });
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
                    .child(title)
                    .children(self.hint.map(|hint| {
                        div()
                            .text_size(px(crate::tokens::text::SMALL))
                            .text_color(theme::muted_fg())
                            .child(hint)
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .children(reset)
                    .child(control),
            )
            .into_any_element()
    }
}

/// A row for a whole element that has no label at the left (a table, a list of chips).
pub fn block(content: impl IntoElement) -> AnyElement {
    div().py_2p5().child(content).into_any_element()
}

/// A text field of a row, `width` pixels wide, for a state made with [`InputState::new`].
pub fn text_field(state: &Entity<InputState>, width: f32) -> impl IntoElement {
    div().w(px(width)).child(Input::new(state).small())
}

/// A line of muted text, for a note under a group or a state with nothing to show.
pub fn note(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(crate::tokens::text::BODY))
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
        .child(icon::tinted(icon, 22., theme::muted_fg()))
        .child(note(text))
}

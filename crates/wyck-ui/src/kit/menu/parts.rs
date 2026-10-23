//! The pieces a menu or a popover is made of, for a list placed by hand: the card, the panel,
//! a row, a heading, a separator and the layer behind a popover.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Div, ElementId, MouseButton, SharedString, Window, anchored, deferred, div, px,
};
use gpui_kit::assets::IconName;

use crate::kit::tokens::{menu as size, text};
use crate::kit::{icon, theme};

/// Puts `content` under the button it is a child of, which is `button` pixels tall, drawn over
/// what is around (at `priority`) and kept inside the window. The parent must be `relative` and
/// the size of the button.
///
/// The box is anchored to the corner of the button: without an offset an anchored element sits
/// where it would have been in the flow, which is under the button, and the gap would count from
/// there. Every panel that opens under a button goes through this, so they all leave the same
/// `size::GAP`.
pub fn below(content: impl IntoElement, button: f32, priority: usize) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_0()
        .child(
            deferred(
                anchored()
                    .snap_to_window_with_margin(px(size::MARGIN))
                    .child(div().pt(px(button + size::GAP)).child(content)),
            )
            .with_priority(priority),
        )
        .into_any_element()
}

/// The card of a menu: a column of rows on the surface, with an edge and a shadow. [`super::Menu`] draws
/// its entries in it; a list that is not a [`super::Menu`] (a picker placed by hand) uses it too, so every
/// menu of the app is the same card.
pub fn card() -> Div {
    div()
        .p_1p5()
        .flex()
        .flex_col()
        .gap_0p5()
        .rounded_lg()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border_subtle())
        .shadow_lg()
        .occlude()
}

/// The card of a popover that holds controls rather than entries (the timeframes, the layouts,
/// the account), `width` pixels wide: usually [`size::PANEL_WIDTH`].
pub fn panel(width: f32) -> Div {
    div()
        .w(px(width))
        .p_3()
        .flex()
        .flex_col()
        .gap_3()
        .rounded_lg()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border_subtle())
        .shadow_lg()
        .occlude()
}

/// What every row of a menu starts from: its height, padding, corners and text.
pub(super) fn row_base() -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2p5()
        .min_h(px(crate::kit::tokens::height::control()))
        .py_1()
        .px_3()
        .rounded_md()
        .text_size(px(text::body()))
}

/// A row of a list placed in a [`card`] by hand, in the look of a [`super::Menu`] entry: an optional
/// icon, the label, and a check mark on the one chosen. The caller adds the click.
pub fn row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    glyph: Option<IconName>,
    checked: bool,
) -> gpui::Stateful<Div> {
    row_base()
        .id(id)
        .cursor_pointer()
        .text_color(theme::fg())
        .hover(|s| s.bg(theme::surface_hover()))
        .children(glyph.map(|glyph| icon::tinted(glyph, 15., theme::fg())))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .whitespace_nowrap()
                .child(label.into()),
        )
        .children(checked.then(|| icon::tinted(IconName::Check, 13., theme::accent())))
}

/// The heading of a group of rows, in a menu or a popover.
pub fn section_title(title: impl Into<SharedString>) -> Div {
    div()
        .px_3()
        .pt_1()
        .pb_0p5()
        .text_size(px(text::caption()))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme::muted_fg())
        .child(title.into().to_uppercase())
}

/// A line between two groups of rows.
pub(crate) fn separator() -> Div {
    div()
        .mx_1()
        .my_1()
        .h(px(crate::kit::tokens::HAIRLINE))
        .bg(theme::border_hairline())
}

/// A layer over the whole window that closes what is open when it is pressed: the one way a
/// popover whose open state its view keeps is dismissed by a click outside it. The caller draws it
/// under the popover (`deferred` at a lower priority).
pub fn backdrop(on_dismiss: impl Fn(&mut Window, &mut App) + 'static) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .occlude()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            on_dismiss(window, cx);
        })
}

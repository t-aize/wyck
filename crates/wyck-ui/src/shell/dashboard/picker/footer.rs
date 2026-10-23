//! The key hints under the list of the symbol picker.

use gpui::prelude::*;
use gpui::{div, px};

use crate::kit::icon::IconName;
use crate::kit::{icon, theme, tokens};

/// A key cap: `Esc`, or an icon for an arrow key.
pub(super) fn key_cap(content: impl IntoElement) -> gpui::Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .min_w(px(tokens::height::tiny()))
        .h(px(tokens::height::tiny()))
        .px_2()
        .rounded_md()
        .border_1()
        .border_color(theme::border_subtle())
        .bg(theme::bg())
        .text_size(px(tokens::text::small()))
        .text_color(theme::muted_fg())
        .child(content)
}

fn key_hint(caps: Vec<gpui::AnyElement>, label: &'static str) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .children(caps)
        .child(
            div()
                .text_size(px(tokens::text::body()))
                .text_color(theme::muted_fg())
                .child(label),
        )
}

/// The line of key hints and the count.
pub(super) fn footer(shown: usize, total: usize) -> gpui::Div {
    let arrow = |icon| key_cap(icon::tinted(icon, 11., theme::muted_fg())).into_any_element();
    let count = if shown == total {
        format!("{total} symbols")
    } else {
        format!("{shown} of {total} symbols")
    };
    div()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .h(px(tokens::height::row()))
        .px_4()
        .border_t_1()
        .border_color(theme::border_hairline())
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_4()
                .child(key_hint(
                    vec![arrow(IconName::ArrowUp), arrow(IconName::ArrowDown)],
                    "Navigate",
                ))
                .child(key_hint(vec![arrow(IconName::CornerDownLeft)], "Select"))
                .child(key_hint(vec![key_cap("esc").into_any_element()], "Close")),
        )
        .child(
            div()
                .text_size(px(tokens::text::body()))
                .text_color(theme::muted_fg())
                .child(count),
        )
}

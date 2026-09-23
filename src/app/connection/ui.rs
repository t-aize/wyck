//! Small building blocks shared by every screen in the connection flow, styled from
//! [`super::theme`] so a screen never spells out a color or radius itself.

use gpui::prelude::*;
use gpui::{App, ClickEvent, Div, SharedString, Window, div, px};

use super::theme;

/// The filled, accent-colored call-to-action button.
pub fn primary_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .w_full()
        .h(px(40.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_lg()
        .bg(theme::accent())
        .text_size(px(14.))
        .text_color(theme::accent_fg())
        .cursor_pointer()
        .hover(|style| style.opacity(0.9))
        .on_click(on_click)
        .child(label.into())
}

/// A borderless, muted text link/button (e.g. "Cancel", "Use a different cTrader ID").
pub fn ghost_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .text_size(px(13.))
        .text_color(theme::muted_fg())
        .cursor_pointer()
        .hover(|style| style.text_color(theme::fg()))
        .on_click(on_click)
        .child(label.into())
}

/// An outlined, secondary button (e.g. "Open browser again", "Retry").
pub fn secondary_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .w_full()
        .h(px(36.))
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .rounded_lg()
        .border_1()
        .border_color(theme::border_subtle())
        .text_size(px(13.))
        .text_color(theme::fg())
        .cursor_pointer()
        .hover(|style| style.bg(theme::surface()))
        .on_click(on_click)
        .child(label.into())
}

/// The raised panel most screens center their content in.
pub fn card() -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(420.))
        .p(px(28.))
        .gap_5()
        .rounded_xl()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border_subtle())
}

/// A small circular index badge, used in the "what happens next" list and progress steps.
pub fn step_badge(label: impl Into<SharedString>) -> impl IntoElement {
    div()
        .size(px(22.))
        .flex_shrink_0()
        .rounded_full()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border_subtle())
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.))
        .text_color(theme::muted_fg())
        .child(label.into())
}

/// A labeled field wrapper: a small caption above whatever's given as the field itself.
pub fn field(label: impl Into<SharedString>, content: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1p5()
        .child(
            div()
                .text_size(px(13.))
                .text_color(theme::fg())
                .child(label.into()),
        )
        .child(content)
}

/// An inline error message under a field.
pub fn field_error(message: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_size(px(12.))
        .text_color(theme::destructive())
        .child(message.into())
}

/// A full-width error banner, for a mistake that blocks the whole screen (bad credentials, no
/// network, ...).
pub fn error_banner(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .p(px(12.))
        .rounded_lg()
        .bg(theme::destructive_bg())
        .border_1()
        .border_color(theme::destructive())
        .child(
            div()
                .flex()
                .flex_row()
                .items_start()
                .gap_2()
                .text_size(px(13.))
                .text_color(theme::destructive())
                .child("\u{26a0}")
                .child(div().flex_1().child(title.into())),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(theme::muted_fg())
                .child(detail.into()),
        )
}

/// A pill-shaped tag, e.g. `LIVE` or `DEMO` next to an account name.
pub fn badge(
    label: impl Into<SharedString>,
    color: gpui::Rgba,
    tint: gpui::Rgba,
) -> impl IntoElement {
    div()
        .px_1p5()
        .py(px(1.))
        .rounded_sm()
        .bg(tint)
        .text_size(px(10.))
        .text_color(color)
        .child(label.into())
}

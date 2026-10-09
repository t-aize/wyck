//! Page-level building blocks: the screen shell, cards, tiles, banners, badges and status dots.

use gpui::prelude::*;
use gpui::{Div, Rgba, SharedString, div, px};
use gpui_kit::assets::IconName;

use crate::ui::kit::{anim, icon, theme};

/// The page shell every screen renders into: content centered in the remaining space, with room
/// for a [`crate::ui::kit::button::back`] to sit absolutely positioned in the top-left corner.
pub fn screen() -> Div {
    // The top padding keeps content clear of the progress indicator and the Back button.
    div()
        .relative()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .pt(px(76.))
        .pb(px(24.))
}

/// The raised panel most screens center their content in.
pub fn card() -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(480.))
        .p(px(32.))
        .gap_6()
        .rounded_xl()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border_subtle())
}

/// A rounded tile with an icon inside, used as the hero mark of a screen.
pub fn icon_tile(name: IconName, tile: f32, glyph: f32, bg: Rgba, fg: Rgba) -> Div {
    div()
        .size(px(tile))
        .flex_shrink_0()
        .rounded_xl()
        .bg(bg)
        .flex()
        .items_center()
        .justify_center()
        .text_color(fg)
        .child(icon::tinted(name, glyph, fg))
}

/// A circular icon badge for the "what happens next" list.
pub fn step_badge(name: IconName) -> impl IntoElement {
    div()
        .size(px(32.))
        .flex_shrink_0()
        .rounded_full()
        .bg(theme::accent_selected())
        .flex()
        .items_center()
        .justify_center()
        .text_color(theme::fg())
        .child(icon::plain(name, 15.))
}

/// A labeled field wrapper: an icon and a small caption above whatever's given as the field. For a
/// label beside the control, as in the settings panels, use [`crate::ui::kit::form::field`].
pub fn stacked_field(
    icon_name: IconName,
    label: impl Into<SharedString>,
    content: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(crate::ui::kit::tokens::text::emphasis()))
                .text_color(theme::fg())
                .child(icon::plain(icon_name, 14.).text_color(theme::muted_fg()))
                .child(label.into()),
        )
        .child(content)
}

/// A full-width error banner, for a mistake that blocks the whole screen (bad credentials, no
/// network, ...). Drops in from above and shakes once. `epoch` keeps the motion replaying each
/// time a new error arrives.
pub fn error_banner(
    id: &'static str,
    epoch: u64,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
) -> impl IntoElement {
    let body = div()
        .flex()
        .flex_col()
        .gap_1()
        .p(px(14.))
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
                .text_size(px(crate::ui::kit::tokens::text::emphasis()))
                .text_color(theme::destructive())
                .child(icon::tinted(
                    IconName::TriangleAlert,
                    15.,
                    theme::destructive(),
                ))
                .child(div().flex_1().child(title.into())),
        )
        .child(
            div()
                .text_size(px(crate::ui::kit::tokens::text::body()))
                .text_color(theme::muted_fg())
                .child(detail.into()),
        );
    anim::drop_in(
        div().w_full().child(anim::shake(body, (id, epoch))),
        (id, epoch),
    )
}

/// A pill-shaped tag, e.g. `LIVE` or `DEMO` next to an account name.
pub fn badge(label: impl Into<SharedString>, color: Rgba, tint: Rgba) -> impl IntoElement {
    div()
        .px_2()
        .py(px(2.))
        .rounded_sm()
        .bg(tint)
        .text_size(px(crate::ui::kit::tokens::text::caption()))
        .text_color(color)
        .child(label.into())
}

/// The `LIVE` or `DEMO` badge for an account.
pub fn environment_badge(is_live: bool) -> impl IntoElement {
    if is_live {
        badge("LIVE", theme::amber(), theme::amber_bg())
    } else {
        badge("DEMO", theme::muted_fg(), theme::bg())
    }
}

/// A small solid dot, for a status ("connected", "reconnecting").
pub fn status_dot(color: Rgba) -> impl IntoElement {
    div().size(px(7.)).flex_shrink_0().rounded_full().bg(color)
}

/// A short upright line between two groups of a toolbar.
pub fn divider() -> Div {
    div()
        .flex_none()
        .mx_1()
        .w(px(1.))
        .h(px(16.))
        .bg(theme::border_hairline())
}

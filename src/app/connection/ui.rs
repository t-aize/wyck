//! Small building blocks shared by every screen in the connection flow, styled from
//! [`super::theme`] so a screen never spells out a color or radius itself. Buttons are
//! gpui-component's, restyled through [`theme::apply`]; everything else is plain divs.

use gpui::prelude::*;
use gpui::{App, ClickEvent, Div, Rgba, SharedString, Svg, Window, div, px, svg};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::button::{Button, ButtonVariants};

use super::theme;
use crate::app::anim;

/// A Lucide icon in the primary text color. An `svg` doesn't inherit color from its parent in
/// this GPUI version, so a different tint is chained on with `.text_color(...)` (or use
/// [`icon_colored`]).
pub fn icon(name: IconName, size_px: f32) -> Svg {
    icon_colored(name, size_px, theme::fg())
}

pub fn icon_colored(name: IconName, size_px: f32, color: Rgba) -> Svg {
    svg()
        .path(name.path())
        .size(px(size_px))
        .flex_shrink_0()
        .text_color(color)
}

/// The page shell every screen renders into: content centered in the remaining space, with room
/// for a [`back_button`] to sit absolutely positioned in the top-left corner.
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

/// The "Back" button in the top-left corner of a [`screen`].
pub fn back_button(
    id: impl Into<gpui::ElementId>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div().absolute().top_5().left_5().child(
        Button::new(id)
            .ghost()
            .icon(IconName::ArrowLeft)
            .label("Back")
            .cursor_pointer()
            .tooltip("Go back")
            .on_click(on_click),
    )
}

/// The filled, accent-colored call-to-action button.
pub fn primary_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id)
        .primary()
        .large()
        .w_full()
        .label(label)
        .cursor_pointer()
        .on_click(on_click)
}

/// A borderless, muted text button (e.g. "Cancel", "Use a different cTrader ID").
pub fn ghost_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id)
        .ghost()
        .label(label)
        .cursor_pointer()
        .on_click(on_click)
}

/// An outlined, secondary button (e.g. "Disconnect").
pub fn secondary_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id)
        .outline()
        .large()
        .w_full()
        .label(label)
        .cursor_pointer()
        .on_click(on_click)
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
        .child(icon_colored(name, glyph, fg))
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
        .child(icon(name, 15.))
}

/// A labeled field wrapper: an icon and a small caption above whatever's given as the field.
pub fn field(
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
                .text_size(px(13.))
                .text_color(theme::fg())
                .child(icon(icon_name, 14.).text_color(theme::muted_fg()))
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
                .text_size(px(13.))
                .text_color(theme::destructive())
                .child(icon_colored(
                    IconName::TriangleAlert,
                    15.,
                    theme::destructive(),
                ))
                .child(div().flex_1().child(title.into())),
        )
        .child(
            div()
                .text_size(px(12.))
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
        .text_size(px(10.))
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

/// A dot with a radar ring pulsing around it: "this is live / working".
pub fn status_dot(id: &'static str, color: Rgba) -> impl IntoElement {
    div()
        .relative()
        .size(px(10.))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .absolute()
                .child(anim::ping(div().rounded_full().bg(color), id, 10.)),
        )
        .child(div().size(px(6.)).rounded_full().bg(color))
}

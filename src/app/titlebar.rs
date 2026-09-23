//! The custom titlebar: replaces the OS chrome with our own draggable bar, the app mark, and
//! (on platforms without native traffic lights) our own window control buttons.

use gpui::prelude::*;
use gpui::{App, MouseButton, Window, div, px, rgb};

use super::theme;

/// Call once, right after opening the window, so platforms that support client-side decorations
/// (Linux; macOS/Windows hide their own titlebar via [`gpui::TitlebarOptions::appears_transparent`]
/// instead) know we're drawing our own.
pub fn request_client_decorations(window: &mut Window) {
    window.request_decorations(gpui::WindowDecorations::Client);
}

/// Renders the titlebar. `window` must be the same window this is painted into: dragging and the
/// window controls act on it directly.
pub fn render(_window: &mut Window, _cx: &mut App) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .w_full()
        .h(px(theme::TITLEBAR_HEIGHT))
        .flex_shrink_0()
        .px_4()
        .bg(theme::bg())
        .border_b_1()
        .border_color(theme::border_hairline())
        .id("titlebar")
        .on_mouse_down(MouseButton::Left, |event, window, _cx| {
            if event.click_count == 2 {
                window.zoom_window();
            } else {
                window.start_window_move();
            }
        })
        .child(brand())
        .child(if cfg!(target_os = "macos") {
            div()
        } else {
            window_controls()
        })
}

fn brand() -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            div()
                .size(px(20.))
                .rounded_md()
                .bg(theme::accent())
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.))
                .text_color(theme::accent_fg())
                .child("W"),
        )
        .child(
            div()
                .text_size(px(13.))
                .text_color(theme::fg())
                .child("Wyck"),
        )
}

fn window_controls() -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(control_button(
            "minimize-window",
            "\u{2212}",
            |window, _cx| {
                window.minimize_window();
            },
        ))
        .child(control_button(
            "maximize-window",
            "\u{25a1}",
            |window, _cx| {
                window.zoom_window();
            },
        ))
        .child(control_button_destructive(
            "close-window",
            "\u{d7}",
            |window, _cx| {
                window.remove_window();
            },
        ))
}

fn control_button(
    id: &'static str,
    glyph: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(28.))
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(14.))
        .text_color(theme::muted_fg())
        .hover(|style| style.bg(theme::surface()).text_color(theme::fg()))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            on_click(window, cx)
        })
        .child(glyph)
}

fn control_button_destructive(
    id: &'static str,
    glyph: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(28.))
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(14.))
        .text_color(theme::muted_fg())
        .hover(|style| style.bg(rgb(0xe81123)).text_color(theme::fg()))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            on_click(window, cx)
        })
        .child(glyph)
}

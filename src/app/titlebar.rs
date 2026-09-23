//! The custom titlebar: replaces the OS chrome with our own draggable bar, the app mark, and
//! (on platforms without native traffic lights) our own window control buttons.

use gpui::prelude::*;
use gpui::{AnyElement, App, MouseButton, Window, WindowControlArea, div, px, rgb, rgba};

use super::theme;

/// Call once, right after opening the window, so platforms that support client-side decorations
/// (Linux; macOS/Windows hide their own titlebar via [`gpui::TitlebarOptions::appears_transparent`]
/// instead) know we're drawing our own.
pub fn request_client_decorations(window: &mut Window) {
    window.request_decorations(gpui::WindowDecorations::Client);
}

/// Renders the titlebar. `window` must be the same window this is painted into: dragging and the
/// window controls act on it directly.
pub fn render(window: &mut Window, _cx: &mut App) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .h(px(theme::TITLEBAR_HEIGHT))
        .flex_shrink_0()
        .bg(theme::bg())
        .border_b_1()
        .border_color(theme::border_hairline())
        .id("titlebar")
        .child(drag_region())
        .child(if cfg!(target_os = "macos") {
            div().into_any_element()
        } else {
            window_controls(window)
        })
}

/// The draggable part of the bar: everything left of the window controls. Kept as its own
/// element (a sibling of the controls, not their parent) so its [`WindowControlArea::Drag`]
/// hitbox never overlaps the buttons' own `Min`/`Max`/`Close` hitboxes.
fn drag_region() -> impl IntoElement {
    div()
        .id("titlebar-drag")
        .flex()
        .flex_1()
        .h_full()
        .items_center()
        .px_4()
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |event, window, _cx| {
            if event.click_count == 2 {
                window.zoom_window();
            } else {
                window.start_window_move();
            }
        })
        // Right-click the bar to get the OS's own system menu (Restore/Move/Size/Minimize/
        // Maximize/Close), same as a native titlebar.
        .on_mouse_down(MouseButton::Right, |event, window, _cx| {
            window.show_window_menu(event.position);
        })
        .child(brand())
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

fn window_controls(window: &Window) -> AnyElement {
    let maximized = window.is_maximized();
    if cfg!(target_os = "windows") {
        windows_controls(maximized).into_any_element()
    } else {
        linux_controls(maximized).into_any_element()
    }
}

/// Matches the Fluent-design caption buttons Windows itself draws: flush against the bar's top,
/// bottom and right edges, 46px wide, square, adjacent with no gap. Each button is registered as
/// a [`WindowControlArea`] rather than driven only by `on_click`, so Windows does the actual
/// hit-testing (`WM_NCHITTEST`/`WM_NCLBUTTONUP`) and, on Windows 11, hovering the maximize button
/// shows the native Snap Layouts flyout. The `on_mouse_down` handlers stay as the fallback path
/// on builds where that native routing isn't wired up.
fn windows_controls(maximized: bool) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .h_full()
        .child(windows_button(
            "minimize-window",
            WindowControlArea::Min,
            "\u{2212}",
            false,
            |window, _cx| window.minimize_window(),
        ))
        .child(windows_button(
            "maximize-window",
            WindowControlArea::Max,
            if maximized { "\u{2750}" } else { "\u{25a1}" },
            false,
            |window, _cx| window.zoom_window(),
        ))
        .child(windows_button(
            "close-window",
            WindowControlArea::Close,
            "\u{d7}",
            true,
            |window, _cx| window.remove_window(),
        ))
}

fn windows_button(
    id: &'static str,
    area: WindowControlArea,
    glyph: &'static str,
    destructive: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .window_control_area(area)
        .w(px(46.))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(14.))
        .text_color(theme::muted_fg())
        .hover(|style| {
            if destructive {
                style.bg(rgb(0xc42b1c)).text_color(rgb(0xffffff))
            } else {
                style.bg(rgba(0xffffff17)).text_color(theme::fg())
            }
        })
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            on_click(window, cx)
        })
        .child(glyph)
}

/// A softer, rounded style for platforms without a single fixed caption-button convention
/// (Linux desktops vary widely). `window_control_area` is a no-op here, so these rely entirely
/// on their own `on_click` handlers.
fn linux_controls(maximized: bool) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .pr_2()
        .child(linux_button("minimize-window", "\u{2212}", false, |window, _cx| {
            window.minimize_window();
        }))
        .child(linux_button(
            "maximize-window",
            if maximized { "\u{2750}" } else { "\u{25a1}" },
            false,
            |window, _cx| window.zoom_window(),
        ))
        .child(linux_button("close-window", "\u{d7}", true, |window, _cx| {
            window.remove_window();
        }))
}

fn linux_button(
    id: &'static str,
    glyph: &'static str,
    destructive: bool,
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
        .hover(|style| {
            if destructive {
                style.bg(rgb(0xc42b1c)).text_color(rgb(0xffffff))
            } else {
                style.bg(theme::surface()).text_color(theme::fg())
            }
        })
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            on_click(window, cx)
        })
        .child(glyph)
}

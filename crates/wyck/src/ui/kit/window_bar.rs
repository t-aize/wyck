//! The window bar the app draws itself, only where the system draws none.
//!
//! The title bar and its buttons belong to the system. The exception is a Linux desktop that
//! refuses server-side decorations (some Wayland compositors): GPUI then falls back to
//! client-side decorations and nobody would draw the title, the buttons or the drag area.

use gpui::prelude::*;
use gpui::{AnyElement, Decorations, Window, div, px};
use gpui_kit::component::TitleBar;

use crate::ui::kit::{theme, tokens};

/// The bar to put above the content, or `None` when the system draws its own.
pub fn fallback(window: &Window) -> Option<AnyElement> {
    if !matches!(window.window_decorations(), Decorations::Client { .. }) {
        return None;
    }
    Some(
        TitleBar::new()
            .bg(theme::surface())
            .border_color(theme::border_hairline())
            .child(
                div()
                    .text_size(px(tokens::text::emphasis()))
                    .text_color(theme::fg())
                    .child("Wyck"),
            )
            .into_any_element(),
    )
}

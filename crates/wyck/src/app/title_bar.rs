//! The branded window bar shared by the connection flow and the dashboard.

use gpui::prelude::*;
use gpui::{FontWeight, MouseButton, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::TitleBar;

use super::connection::ui;
use super::theme;

/// Draws the current section and the unavailable journal beside a drag region.
pub fn render(window: &mut Window) -> impl IntoElement {
    let active = window.is_window_active();
    let width = f32::from(window.viewport_size().width);
    let foreground = if active {
        theme::fg()
    } else {
        theme::fg_alpha(0.65)
    };
    let accent = if active {
        theme::accent()
    } else {
        theme::accent_alpha(0.65)
    };
    let show_icon = width >= 440.0;
    let show_brand = width >= 520.0;
    let show_badge = width >= 660.0;

    let brand = div()
        .flex()
        .flex_none()
        .items_center()
        .gap_2()
        .when(show_icon, |bar| {
            bar.child(
                div()
                    .flex()
                    .size(px(23.0))
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(theme::accent_selected())
                    .child(ui::icon_colored(IconName::ChartCandlestick, 15.0, accent)),
            )
        })
        .when(show_brand, |bar| {
            bar.child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(foreground)
                    .child("Wyck"),
            )
        });

    // These sections are display-only. Occluding their hitboxes keeps them out of the
    // platform drag region that TitleBar places behind its children.
    let trading = div()
        .id("title-section-trading")
        .occlude()
        .flex()
        .flex_none()
        .h(px(25.0))
        .items_center()
        .px_2p5()
        .rounded_md()
        .border_1()
        .border_color(theme::accent_alpha(0.28))
        .bg(theme::accent_selected())
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(foreground)
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .child("Trading");

    let journal = div()
        .id("title-section-journal")
        .occlude()
        .flex()
        .flex_none()
        .h(px(25.0))
        .items_center()
        .gap_1p5()
        .px_2()
        .rounded_md()
        .text_size(px(11.0))
        .text_color(theme::muted_fg())
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .child("Journalisation")
        .when(show_badge, |bar| {
            bar.child(
                div()
                    .rounded_sm()
                    .border_1()
                    .border_color(theme::border_subtle())
                    .px_1p5()
                    .text_size(px(9.0))
                    .child("Bient\u{f4}t"),
            )
        });

    let content = div()
        .flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .items_center()
        .gap_2()
        .child(brand)
        .when(show_icon || show_brand, |bar| {
            bar.child(div().w(px(1.0)).h(px(16.0)).bg(theme::border_hairline()))
        })
        .child(trading)
        .child(journal)
        .child(
            div()
                .flex_1()
                .min_w(px(if width >= 480.0 { 80.0 } else { 32.0 })),
        );

    if window.is_fullscreen() {
        div()
            .flex()
            .flex_none()
            .h(px(34.0))
            .pl_3()
            .pr_3()
            .border_b_1()
            .border_color(theme::border_hairline())
            .bg(theme::surface())
            .child(content)
            .into_any_element()
    } else {
        TitleBar::new()
            .bg(theme::surface())
            .border_color(theme::border_hairline())
            .child(content)
            .into_any_element()
    }
}

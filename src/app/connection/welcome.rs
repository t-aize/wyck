//! Screen 1: the very first thing a user sees, with nothing configured yet.

use gpui::prelude::*;
use gpui::{Window, div, px};

use super::ui;
use super::{ConnectionFlow, theme};

impl ConnectionFlow {
    pub(super) fn render_welcome(
        &self,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        ui::screen().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_5()
                .w(px(420.))
                .child(
                    div()
                        .size(px(64.))
                        .rounded_xl()
                        .bg(theme::accent())
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(26.))
                        .text_color(theme::accent_fg())
                        .child("W"),
                )
                .child(
                    div()
                        .text_size(px(23.))
                        .text_color(theme::fg())
                        .child("Connect your cTrader account"),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(theme::muted_fg())
                        .text_center()
                        .child(
                            "Wyck trades through cTrader's Open API. You'll sign in with your \
                             own cTrader ID in your browser, and Wyck never sees your \
                             password.",
                        ),
                )
                .child(
                    div().w_full().pt_2().child(ui::primary_button(
                        "connect-ctrader",
                        "Connect cTrader account",
                        cx.listener(|this, _event, _window, cx| this.go_to_credentials(cx)),
                    )),
                )
                .child(
                    div()
                        .w_full()
                        .pt_5()
                        .mt_2()
                        .border_t_1()
                        .border_color(theme::border_hairline())
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::muted_fg())
                                .child("WHAT HAPPENS NEXT"),
                        )
                        .child(next_step(
                            "1",
                            "Give Wyck the Client ID and Secret of your own cTrader Open API application",
                        ))
                        .child(next_step(
                            "2",
                            "Sign in with your cTrader ID on cTrader's own page and choose what to allow",
                        ))
                        .child(next_step("3", "Pick which trading account Wyck connects to")),
                ),
        )
    }
}

fn next_step(index: &'static str, text: &'static str) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap_3()
        .child(ui::step_badge(index))
        .child(
            div()
                .flex_1()
                .text_size(px(14.))
                .text_color(theme::muted_fg())
                .child(text),
        )
}

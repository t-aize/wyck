//! Screen 1: the very first thing a user sees, with nothing configured yet.

use gpui::prelude::*;
use gpui::{Window, div, px};
use gpui_kit::assets::IconName;

use super::ui;
use super::{ConnectionFlow, anim, theme};

impl ConnectionFlow {
    pub(super) fn render_welcome(
        &self,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let epoch = self.epoch;
        let steps = [
            (
                IconName::KeyRound,
                "Give Wyck the Client ID and Secret of your own cTrader Open API application",
            ),
            (
                IconName::LogIn,
                "Sign in with your cTrader ID on cTrader's own page and choose what to allow",
            ),
            (
                IconName::Wallet,
                "Pick which trading account Wyck connects to",
            ),
        ];

        ui::screen().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_5()
                .w(px(440.))
                .child(anim::float(
                    div()
                        .relative()
                        .size(px(72.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ui::icon_tile(
                            IconName::ChartCandlestick,
                            72.,
                            34.,
                            theme::accent(),
                            theme::accent_fg(),
                        )),
                    ("welcome-float", epoch),
                ))
                .child(anim::enter(
                    div()
                        .text_size(px(26.))
                        .text_color(theme::fg())
                        .child("Connect your cTrader account"),
                    ("welcome-title", epoch),
                    1,
                ))
                .child(anim::enter(
                    div()
                        .text_size(px(14.))
                        .text_color(theme::muted_fg())
                        .text_center()
                        .child(
                            "Wyck trades through cTrader's Open API. You'll sign in with your \
                             own cTrader ID in your browser, and Wyck never sees your \
                             password.",
                        ),
                    ("welcome-lead", epoch),
                    2,
                ))
                .child(anim::enter(
                    div().w_full().pt_2().child(
                        ui::primary_button(
                            "connect-ctrader",
                            "Connect cTrader account",
                            cx.listener(|this, _event, _window, cx| this.go_to_credentials(cx)),
                        )
                        .icon(IconName::PlugZap),
                    ),
                    ("welcome-cta", epoch),
                    3,
                ))
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
                        .child(anim::enter(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::muted_fg())
                                .child("WHAT HAPPENS NEXT"),
                            ("welcome-next", epoch),
                            4,
                        ))
                        .children(steps.into_iter().enumerate().map(|(i, (icon, text))| {
                            anim::enter(
                                next_step(icon, text),
                                ("welcome-step", epoch * 10 + i as u64),
                                5 + i,
                            )
                        })),
                ),
        )
    }
}

fn next_step(icon: IconName, text: &'static str) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .child(ui::step_badge(icon))
        .child(
            div()
                .flex_1()
                .text_size(px(14.))
                .text_color(theme::muted_fg())
                .child(text),
        )
}

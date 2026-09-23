//! Screen 13: reviewing and disconnecting an existing connection.

use gpui::prelude::*;
use gpui::{Context, Window, div, px};
use wyck::config::ProfileId;
use wyck::openapi::transport::connection::Client;
use wyck::openapi::transport::messages::TraderAccount;

use super::ui;
use super::{ConnectionFlow, Screen, theme};

pub(super) struct ManageState {
    profile_id: ProfileId,
    account: TraderAccount,
    #[allow(dead_code)] // kept alive so the connection isn't dropped while reviewing it
    client: Client,
    confirming_disconnect: bool,
}

impl ManageState {
    pub(super) fn new(profile_id: ProfileId, account: TraderAccount, client: Client) -> Self {
        Self {
            profile_id,
            account,
            client,
            confirming_disconnect: false,
        }
    }
}

impl ConnectionFlow {
    pub(super) fn render_manage(
        &self,
        state: &ManageState,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_live = state.account.is_live.unwrap_or(false);

        div().flex().flex_1().items_center().justify_center().child(
            div()
                .flex()
                .flex_col()
                .gap_5()
                .w(px(420.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::muted_fg())
                                .child("SETTINGS"),
                        )
                        .child(
                            div()
                                .text_size(px(18.))
                                .text_color(theme::fg())
                                .child("cTrader connection"),
                        ),
                )
                .child(
                    ui::card()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_2()
                                        .text_size(px(14.))
                                        .text_color(theme::fg())
                                        .child(
                                            state
                                                .account
                                                .broker_title_short
                                                .clone()
                                                .unwrap_or_else(|| "cTrader".into()),
                                        )
                                        .child(if is_live {
                                            ui::badge("LIVE", theme::amber(), theme::amber_bg())
                                        } else {
                                            ui::badge("DEMO", theme::muted_fg(), theme::surface())
                                        }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1p5()
                                        .text_size(px(12.))
                                        .text_color(theme::emerald())
                                        .child(
                                            div().size(px(6.)).rounded_full().bg(theme::emerald()),
                                        )
                                        .child("Connected"),
                                ),
                        )
                        .child(detail_row("Access", "Trading (view and place orders)"))
                        .child(detail_row(
                            "cTrader ID",
                            &state.account.ctid_trader_account_id.to_string(),
                        ))
                        .child(if state.confirming_disconnect {
                            confirm_disconnect(cx).into_any_element()
                        } else {
                            disconnect_prompt(cx).into_any_element()
                        }),
                )
                .child(div().flex().justify_center().child(ui::ghost_button(
                    "back-to-connected",
                    "Back",
                    cx.listener(|this, _event, _window, cx| this.go_to_connected_from_manage(cx)),
                ))),
        )
    }

    fn go_to_connected_from_manage(&mut self, cx: &mut Context<Self>) {
        let Screen::Manage(state) = &self.screen else {
            return;
        };
        self.screen = Screen::Connected(super::connected::ConnectedState::new(
            state.profile_id.clone(),
            state.account.clone(),
            state.client.clone(),
        ));
        cx.notify();
    }

    fn disconnect(&mut self, cx: &mut Context<Self>) {
        let Screen::Manage(state) = &self.screen else {
            return;
        };
        let profile_id = state.profile_id.clone();
        if let Err(error) = self.config.remove_profile(&profile_id) {
            tracing::warn!(%error, "failed to remove the profile on disconnect");
        }
        self.go_to_welcome(cx);
    }
}

fn detail_row(label: &'static str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .py_2()
        .border_t_1()
        .border_color(theme::border_hairline())
        .text_size(px(12.))
        .child(div().text_color(theme::muted_fg()).child(label))
        .child(div().text_color(theme::fg()).child(value.to_string()))
}

fn disconnect_prompt(cx: &mut Context<ConnectionFlow>) -> impl IntoElement {
    div().pt_3().child(ui::secondary_button(
        "start-disconnect",
        "Disconnect",
        cx.listener(|this, _event, _window, cx| {
            if let Screen::Manage(state) = &mut this.screen {
                state.confirming_disconnect = true;
                cx.notify();
            }
        }),
    ))
}

fn confirm_disconnect(cx: &mut Context<ConnectionFlow>) -> impl IntoElement {
    div()
        .pt_3()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_size(px(12.))
                .text_color(theme::muted_fg())
                .child(
                    "Wyck will stop trading on this account and remove the stored access token \
                     from this device. You can reconnect anytime.",
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(ui::secondary_button(
                    "cancel-disconnect",
                    "Cancel",
                    cx.listener(|this, _event, _window, cx| {
                        if let Screen::Manage(state) = &mut this.screen {
                            state.confirming_disconnect = false;
                            cx.notify();
                        }
                    }),
                ))
                .child(ui::primary_button(
                    "confirm-disconnect",
                    "Disconnect",
                    cx.listener(|this, _event, _window, cx| this.disconnect(cx)),
                )),
        )
}

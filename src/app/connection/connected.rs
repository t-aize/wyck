//! Screen 11: the account is authorized and the profile is saved.

use gpui::prelude::*;
use gpui::{Context, Window, div, px};
use wyck::config::ProfileId;
use wyck::openapi::transport::connection::Client;
use wyck::openapi::transport::messages::TraderAccount;

use super::manage::ManageState;
use super::ui;
use super::{ConnectionFlow, Screen, theme};

pub(super) struct ConnectedState {
    profile_id: ProfileId,
    account: TraderAccount,
    client: Client,
}

impl ConnectedState {
    pub(super) fn new(profile_id: ProfileId, account: TraderAccount, client: Client) -> Self {
        Self {
            profile_id,
            account,
            client,
        }
    }
}

impl ConnectionFlow {
    pub(super) fn render_connected(
        &self,
        state: &ConnectedState,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_live = state.account.is_live.unwrap_or(false);
        let login = state
            .account
            .trader_login
            .map(|login| login.to_string())
            .unwrap_or_else(|| "-".into());

        ui::screen().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_5()
                .w(px(420.))
                .child(
                    div()
                        .size(px(56.))
                        .rounded_full()
                        .bg(theme::emerald_bg())
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme::emerald())
                        .child(ui::icon("icons/check.svg", 24.)),
                )
                .child(
                    div()
                        .text_size(px(19.))
                        .text_color(theme::fg())
                        .child("Connected"),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(theme::muted_fg())
                        .text_center()
                        .child(
                            "Wyck is authorized on this account. It'll refresh access \
                             automatically, so you won't need to sign in again unless you \
                             revoke it.",
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .p(px(14.))
                        .rounded_lg()
                        .bg(theme::surface())
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
                                .text_size(px(13.))
                                .text_color(theme::muted_fg())
                                .child(format!("login {login}")),
                        ),
                )
                .child(ui::primary_button(
                    "continue-to-wyck",
                    "Continue to Wyck",
                    // The trading dashboard isn't built yet; this is the connection flow's exit
                    // point once it is.
                    |_event, _window, _cx| {},
                ))
                .child(div().flex().justify_center().child(ui::ghost_button(
                    "manage-connection",
                    "Manage this connection",
                    cx.listener(|this, _event, _window, cx| this.go_to_manage(cx)),
                ))),
        )
    }

    fn go_to_manage(&mut self, cx: &mut Context<Self>) {
        let Screen::Connected(state) = &self.screen else {
            return;
        };
        self.screen = Screen::Manage(ManageState::new(
            state.profile_id.clone(),
            state.account.clone(),
            state.client.clone(),
        ));
        cx.notify();
    }
}

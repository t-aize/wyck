//! Screen 11: the account is authorized and the profile is saved.

use gpui::prelude::*;
use gpui::{Context, Window, div, px};
use gpui_kit::assets::IconName;
use wyck::config::ProfileId;
use wyck::openapi::transport::connection::Client;
use wyck::openapi::transport::messages::TraderAccount;

use super::manage::ManageState;
use super::ui;
use super::{ConnectionFlow, Screen, anim, theme};

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
        let epoch = self.epoch;
        let is_live = state.account.is_live.unwrap_or(false);
        let login = state
            .account
            .trader_login
            .map(|login| login.to_string())
            .unwrap_or_else(|| "-".into());

        // This is the flow's last step, so there is no Back button: the way out is "Continue"
        // (or "Manage" to disconnect).
        ui::screen().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_5()
                .w(px(440.))
                .child(
                    // A success mark: a green disc that pops in with an overshoot, with a ring
                    // radiating out of it once it lands.
                    div()
                        .relative()
                        .size(px(96.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(div().absolute().child(anim::ping(
                            div().rounded_full().bg(theme::emerald()),
                            ("connected-ping", epoch),
                            72.,
                        )))
                        .child(anim::pop(
                            div()
                                .rounded_full()
                                .bg(theme::emerald_bg())
                                .border_1()
                                .border_color(theme::emerald())
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(theme::emerald())
                                .child(ui::icon_colored(IconName::Check, 34., theme::emerald())),
                            ("connected-pop", epoch),
                            72.,
                        )),
                )
                .child(anim::enter(
                    div()
                        .text_size(px(24.))
                        .text_color(theme::fg())
                        .child("Connected"),
                    ("connected-title", epoch),
                    2,
                ))
                .child(anim::enter(
                    div()
                        .text_size(px(14.))
                        .text_color(theme::muted_fg())
                        .text_center()
                        .child(
                            "Wyck is authorized on this account. It'll refresh access \
                             automatically, so you won't need to sign in again unless you \
                             revoke it.",
                        ),
                    ("connected-lead", epoch),
                    3,
                ))
                .child(anim::enter(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .p(px(14.))
                        .rounded_lg()
                        .bg(theme::surface())
                        .border_1()
                        .border_color(theme::border_subtle())
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .size(px(36.))
                                        .flex_shrink_0()
                                        .rounded_lg()
                                        .bg(theme::bg())
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_color(theme::muted_fg())
                                        .child(ui::icon_colored(
                                            IconName::Building2,
                                            17.,
                                            theme::muted_fg(),
                                        )),
                                )
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
                                        .child(ui::environment_badge(is_live)),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(theme::muted_fg())
                                .child(format!("login {login}")),
                        ),
                    ("connected-card", epoch),
                    4,
                ))
                .child(anim::enter(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            ui::primary_button(
                                "continue-to-wyck",
                                "Continue to Wyck",
                                // The trading dashboard isn't built yet; this is the connection
                                // flow's exit point once it is.
                                |_event, _window, _cx| {},
                            )
                            .icon(IconName::ArrowRight),
                        )
                        .child(div().flex().justify_center().child(ui::ghost_button(
                            "manage-connection",
                            "Manage this connection",
                            cx.listener(|this, _event, _window, cx| this.go_to_manage(cx)),
                        ))),
                    ("connected-actions", epoch),
                    5,
                )),
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

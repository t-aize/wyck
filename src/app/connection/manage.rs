//! Screen 13: reviewing and disconnecting an existing connection.

use gpui::prelude::*;
use gpui::{Context, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants;
use wyck::config::ProfileId;
use wyck::openapi::transport::connection::Client;
use wyck::openapi::transport::messages::TraderAccount;

use super::ui;
use super::{ConnectionFlow, Screen, anim, theme};

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
        let epoch = self.epoch;
        let is_live = state.account.is_live.unwrap_or(false);

        ui::screen()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .w(px(500.))
                    .child(anim::enter(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_4()
                            .child(ui::icon_tile(
                                IconName::Settings,
                                48.,
                                22.,
                                theme::accent_selected(),
                                theme::fg(),
                            ))
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
                                            .text_size(px(20.))
                                            .text_color(theme::fg())
                                            .child("cTrader connection"),
                                    ),
                            ),
                        ("manage-title", epoch),
                        0,
                    ))
                    .child(anim::enter(
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
                                            .child(ui::environment_badge(is_live)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .text_size(px(12.))
                                            .text_color(theme::emerald())
                                            .child(ui::status_dot(
                                                "manage-status",
                                                theme::emerald(),
                                            ))
                                            .child("Connected"),
                                    ),
                            )
                            .child(detail_row(
                                IconName::ShieldCheck,
                                "Access",
                                "Trading (view and place orders)",
                            ))
                            .child(detail_row(
                                IconName::UserRound,
                                "cTrader ID",
                                &state.account.ctid_trader_account_id.to_string(),
                            ))
                            .child(if state.confirming_disconnect {
                                anim::enter(confirm_disconnect(cx), ("confirm", epoch), 0)
                                    .into_any_element()
                            } else {
                                disconnect_prompt(cx).into_any_element()
                            }),
                        ("manage-card", epoch),
                        1,
                    )),
            )
            .child(ui::back_button(
                "manage-back",
                cx.listener(|this, _event, _window, cx| this.go_to_connected_from_manage(cx)),
            ))
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

fn detail_row(icon: IconName, label: &'static str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .py_2p5()
        .border_t_1()
        .border_color(theme::border_hairline())
        .text_size(px(13.))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_color(theme::muted_fg())
                .child(ui::icon_colored(icon, 14., theme::muted_fg()))
                .child(label),
        )
        .child(div().text_color(theme::fg()).child(value.to_string()))
}

fn disconnect_prompt(cx: &mut Context<ConnectionFlow>) -> impl IntoElement {
    div().pt_3().child(
        ui::secondary_button(
            "start-disconnect",
            "Disconnect",
            cx.listener(|this, _event, _window, cx| {
                if let Screen::Manage(state) = &mut this.screen {
                    state.confirming_disconnect = true;
                    cx.notify();
                }
            }),
        )
        .icon(IconName::Unplug),
    )
}

fn confirm_disconnect(cx: &mut Context<ConnectionFlow>) -> gpui::Div {
    div()
        .w_full()
        .pt_3()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_start()
                .gap_2()
                .text_size(px(13.))
                .text_color(theme::muted_fg())
                .child(ui::icon(IconName::TriangleAlert, 15.).text_color(theme::amber()))
                .child(div().flex_1().min_w_0().child(
                    "Wyck will stop trading on this account and remove the stored access token \
                     from this device. You can reconnect anytime.",
                )),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                // Each button is full width, so each sits in its own equal share of the row.
                .child(div().flex_1().min_w_0().child(ui::secondary_button(
                    "cancel-disconnect",
                    "Cancel",
                    cx.listener(|this, _event, _window, cx| {
                        if let Screen::Manage(state) = &mut this.screen {
                            state.confirming_disconnect = false;
                            cx.notify();
                        }
                    }),
                )))
                .child(
                    div().flex_1().min_w_0().child(
                        ui::primary_button(
                            "confirm-disconnect",
                            "Disconnect",
                            cx.listener(|this, _event, _window, cx| this.disconnect(cx)),
                        )
                        .danger()
                        .icon(IconName::Unplug),
                    ),
                ),
        )
}

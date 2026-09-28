//! Screen 9: picking which of the authorized trading accounts to connect to.

use gpui::prelude::*;
use gpui::{Context, Window, div, px};
use gpui_kit::assets::IconName;
use wyck_openapi::Environment;
use wyck_openapi::auth::TokenSet;
use wyck_openapi::config::ClientCredentials;
use wyck_openapi::transport::connection::Client;
use wyck_openapi::transport::messages::TraderAccount;

use super::ui;
use super::{ConnectionFlow, Screen, anim, theme};

pub(super) struct SelectAccountState {
    credentials: ClientCredentials,
    environment: Environment,
    client: Client,
    tokens: TokenSet,
    accounts: Vec<TraderAccount>,
    selected: usize,
}

impl SelectAccountState {
    pub(super) fn new(
        credentials: ClientCredentials,
        environment: Environment,
        client: Client,
        tokens: TokenSet,
        accounts: Vec<TraderAccount>,
    ) -> Self {
        Self {
            credentials,
            environment,
            client,
            tokens,
            accounts,
            selected: 0,
        }
    }
}

impl ConnectionFlow {
    pub(super) fn render_select_account(
        &self,
        state: &SelectAccountState,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let epoch = self.epoch;

        ui::screen()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .w(px(520.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_4()
                            .child(ui::icon_tile(
                                IconName::Wallet,
                                48.,
                                22.,
                                theme::accent_selected(),
                                theme::fg(),
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(20.))
                                            .text_color(theme::fg())
                                            .child("Choose a trading account"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(theme::muted_fg())
                                            .child(
                                                "Your cTrader ID authorized Wyck for these \
                                                 accounts. Pick the one to trade on: you can \
                                                 add more later.",
                                            ),
                                    ),
                            ),
                    )
                    .child(div().flex().flex_col().gap_2().children(
                        state.accounts.iter().enumerate().map(|(index, account)| {
                            anim::enter(
                                account_row(index, account, index == state.selected, &mut *cx),
                                ("account-enter", epoch * 1000 + index as u64),
                                index,
                            )
                        }),
                    ))
                    .child(
                        ui::primary_button(
                            "connect-selected-account",
                            "Connect this account",
                            cx.listener(|this, _event, _window, cx| {
                                this.authorize_selected_account(cx)
                            }),
                        )
                        .icon(IconName::PlugZap),
                    )
                    .child(div().flex().justify_center().child(ui::ghost_button(
                        "use-different-account",
                        "Use a different cTrader ID",
                        cx.listener(|this, _event, _window, cx| this.go_to_credentials(cx)),
                    ))),
            )
            .child(ui::back_button(
                "select-account-back",
                cx.listener(|this, _event, _window, cx| this.go_to_credentials(cx)),
            ))
    }

    fn authorize_selected_account(&mut self, cx: &mut Context<Self>) {
        let Screen::SelectAccount(state) = &self.screen else {
            return;
        };
        let Some(account) = state.accounts.get(state.selected).cloned() else {
            return;
        };
        self.start_authorizing(
            state.credentials.clone(),
            state.environment,
            state.client.clone(),
            state.tokens.clone(),
            account,
            cx,
        );
    }
}

fn account_row(
    index: usize,
    account: &TraderAccount,
    selected: bool,
    cx: &mut Context<ConnectionFlow>,
) -> gpui::Stateful<gpui::Div> {
    let is_live = account.is_live.unwrap_or(false);
    let login = account
        .trader_login
        .map(|login| login.to_string())
        .unwrap_or_else(|| "-".into());

    // The check inside the radio pops in each time this row becomes the selected one; the id
    // includes `selected` so unselecting and reselecting replays it.
    let radio = div()
        .size(px(20.))
        .flex_shrink_0()
        .rounded_full()
        .border_1()
        .flex()
        .items_center()
        .justify_center()
        .border_color(if selected {
            theme::accent()
        } else {
            theme::border_subtle()
        })
        .when(selected, |el| {
            el.bg(theme::accent()).child(anim::pop(
                div().rounded_full().bg(theme::accent_fg()),
                ("account-radio", index as u64),
                8.,
            ))
        });
    let radio = if selected {
        radio
    } else {
        radio.bg(theme::bg())
    };

    div()
        .id(("account-row", index as u64))
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .p(px(14.))
        .rounded_lg()
        .bg(theme::surface())
        .border_1()
        .border_color(if selected {
            theme::accent()
        } else {
            theme::border_subtle()
        })
        .cursor_pointer()
        .hover(|style| {
            style.bg(theme::surface_hover()).border_color(if selected {
                theme::accent()
            } else {
                theme::border_strong()
            })
        })
        .active(|style| style.bg(theme::surface_pressed()))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            if let Screen::SelectAccount(state) = &mut this.screen {
                state.selected = index;
                cx.notify();
            }
        }))
        .child(radio)
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
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .text_size(px(14.))
                        .text_color(theme::fg())
                        .child(
                            account
                                .broker_title_short
                                .clone()
                                .unwrap_or_else(|| "cTrader".into()),
                        )
                        .child(ui::environment_badge(is_live)),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(theme::muted_fg())
                        .child(format!(
                            "cTrader ID {} - login {login}",
                            account.ctid_trader_account_id
                        )),
                ),
        )
}

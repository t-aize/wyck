//! Screens 4 through 8: everything that happens automatically once the browser has been opened
//! to cTrader's consent page: waiting for the redirect, exchanging the code for tokens, and
//! fetching the list of accounts the token covers. All of it is one background task; the screen
//! itself just shows progress and lets the user cancel or reopen the browser.

use std::time::Duration;

use gpui::prelude::*;
use gpui::{Context, SharedString, Window, div, px};
use gpui_kit::assets::IconName;
use secrecy::ExposeSecret;
use wyck::openapi::Environment;
use wyck::openapi::auth::{CallbackListener, OAuthClient, Scope, authorization_url, new_state};
use wyck::openapi::config::ClientCredentials;
use wyck::openapi::transport::connection::Client;

use super::select_account::SelectAccountState;
use super::ui;
use super::{ConnectionFlow, Screen, anim, theme};
use crate::app::runtime;

/// How long to wait for the user to finish signing in on cTrader's page before giving up.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

pub(super) struct BrowserHandoffState {
    url: String,
    error: Option<SharedString>,
    error_count: u64,
}

/// The application and connection an OAuth sign-in is in progress for.
struct PendingConnection {
    credentials: ClientCredentials,
    environment: Environment,
    client: Client,
}

impl ConnectionFlow {
    /// Called once, right after the credentials screen verifies the application: opens the
    /// system browser to cTrader's consent page and starts waiting for the redirect in the
    /// background.
    pub(super) fn start_browser_handoff(
        &mut self,
        credentials: ClientCredentials,
        environment: Environment,
        client: Client,
        listener: CallbackListener,
        cx: &mut Context<Self>,
    ) {
        let redirect_uri = listener.redirect_uri();
        let oauth_state = new_state();
        let url = authorization_url(
            &credentials.client_id,
            &redirect_uri,
            Scope::Accounts,
            &oauth_state,
        );

        cx.open_url(&url);
        self.screen = Screen::BrowserHandoff(BrowserHandoffState {
            url: url.clone(),
            error: None,
            error_count: 0,
        });
        cx.notify();

        self.wait_for_sign_in(
            listener,
            oauth_state,
            PendingConnection {
                credentials,
                environment,
                client,
            },
            cx,
        );
    }

    fn wait_for_sign_in(
        &mut self,
        listener: CallbackListener,
        oauth_state: String,
        pending: PendingConnection,
        cx: &mut Context<Self>,
    ) {
        let PendingConnection {
            credentials,
            environment,
            client,
        } = pending;
        let redirect_uri = listener.redirect_uri();

        cx.spawn(async move |this, cx| {
            let waited =
                runtime::spawn(async move { listener.wait(&oauth_state, SIGN_IN_TIMEOUT).await })
                    .await;
            let code = match waited {
                Ok(Ok(code)) => code,
                Ok(Err(error)) => return fail(&this, cx, error.to_string()),
                Err(_) => {
                    return fail(
                        &this,
                        cx,
                        "the background runtime dropped the request".into(),
                    );
                }
            };

            let oauth = match OAuthClient::new(credentials.clone()) {
                Ok(oauth) => oauth,
                Err(error) => return fail(&this, cx, error.to_string()),
            };
            let code_value = code.code().to_string();
            let exchanged =
                runtime::spawn(
                    async move { oauth.exchange_code(&code_value, &redirect_uri).await },
                )
                .await;
            let tokens = match exchanged {
                Ok(Ok(tokens)) => tokens,
                Ok(Err(error)) => return fail(&this, cx, error.to_string()),
                Err(_) => {
                    return fail(
                        &this,
                        cx,
                        "the background runtime dropped the request".into(),
                    );
                }
            };

            let access_token = tokens.access_token.expose_secret().to_string();
            let accounts_client = client.clone();
            let accounts_access = access_token.clone();
            let listed =
                runtime::spawn(async move { accounts_client.accounts(&accounts_access).await })
                    .await;
            let accounts = match listed {
                Ok(Ok(res)) if !res.ctid_trader_account.is_empty() => res.ctid_trader_account,
                Ok(Ok(_)) => {
                    return fail(
                        &this,
                        cx,
                        "no trading account is covered by this token yet: pick at least one on \
                         cTrader's consent page, then reconnect."
                            .into(),
                    );
                }
                Ok(Err(error)) => return fail(&this, cx, error.to_string()),
                Err(_) => {
                    return fail(
                        &this,
                        cx,
                        "the background runtime dropped the request".into(),
                    );
                }
            };

            let _ = this.update(cx, |this, cx| {
                // The user may have gone back or cancelled while this was waiting.
                if !matches!(this.screen, Screen::BrowserHandoff(_)) {
                    return;
                }
                this.screen = Screen::SelectAccount(SelectAccountState::new(
                    credentials,
                    environment,
                    client,
                    tokens,
                    accounts,
                ));
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_browser_handoff(
        &self,
        state: &BrowserHandoffState,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let epoch = self.epoch;
        let failed = state.error.is_some();

        ui::screen()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_5()
                    .w(px(420.))
                    .children(state.error.clone().map(|message| {
                        ui::error_banner(
                            "handoff-error",
                            state.error_count,
                            "Couldn't finish signing in",
                            message,
                        )
                    }))
                    .child(
                        div()
                            .relative()
                            .size(px(88.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .size(px(72.))
                                    .rounded_full()
                                    .bg(theme::accent_selected())
                                    .border_1()
                                    .border_color(theme::accent())
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(theme::fg())
                                    .child(ui::icon(IconName::Globe, 30.)),
                            )
                            .when(!failed, |el| {
                                // A small spinner badge on the corner of the globe.
                                el.child(
                                    div()
                                        .absolute()
                                        .right_0()
                                        .bottom_0()
                                        .size(px(28.))
                                        .rounded_full()
                                        .bg(theme::bg())
                                        .border_1()
                                        .border_color(theme::border_subtle())
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_color(theme::accent())
                                        .child(anim::spin(
                                            ui::icon_colored(
                                                IconName::LoaderCircle,
                                                15.,
                                                theme::accent(),
                                            ),
                                            ("handoff-spin", epoch),
                                        )),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(20.))
                            .text_color(theme::fg())
                            .child("Finish this in your browser"),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(theme::muted_fg())
                            .text_center()
                            .child(
                                "We opened cTrader's sign-in page. Come back here once you've \
                                 allowed access, and Wyck picks it up automatically.",
                            ),
                    )
                    .when(!failed, |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_size(px(13.))
                                .text_color(theme::accent())
                                .child(ui::icon_colored(IconName::Radio, 14., theme::accent()))
                                .child("Waiting for cTrader..."),
                        )
                    })
                    .child(
                        div()
                            .w_full()
                            .pt_2()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                ui::secondary_button("reopen-browser", "Open browser again", {
                                    let url = state.url.clone();
                                    move |_event, _window, cx| cx.open_url(&url)
                                })
                                .icon(IconName::ExternalLink),
                            )
                            .child(ui::ghost_button(
                                "cancel-browser-handoff",
                                "Cancel",
                                cx.listener(|this, _event, _window, cx| this.go_to_credentials(cx)),
                            )),
                    ),
            )
            .child(ui::back_button(
                "browser-handoff-back",
                cx.listener(|this, _event, _window, cx| this.go_to_credentials(cx)),
            ))
    }
}

fn fail(weak: &gpui::WeakEntity<ConnectionFlow>, cx: &mut gpui::AsyncApp, message: String) {
    let _ = weak.update(cx, |this, cx| {
        if let Screen::BrowserHandoff(state) = &mut this.screen {
            state.error = Some(message.into());
            state.error_count += 1;
        }
        cx.notify();
    });
}

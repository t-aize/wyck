//! Screen 10: the last automatic step, authorizing the chosen account on the connection and
//! saving everything to disk.

use gpui::prelude::*;
use gpui::{Context, SharedString, Window, div, px};
use gpui_kit::assets::IconName;
use secrecy::ExposeSecret;
use wyck::config::OpenApiTokens;
use wyck::openapi::Environment;
use wyck::openapi::auth::TokenSet;
use wyck::openapi::config::ClientCredentials;
use wyck::openapi::transport::connection::Client;
use wyck::openapi::transport::messages::TraderAccount;

use super::connected::ConnectedState;
use super::credentials::CALLBACK_PORT;
use super::ui;
use super::{ConnectionFlow, Screen, anim, theme};
use crate::app::runtime;

pub(super) struct AuthorizingState {
    account: TraderAccount,
    error: Option<SharedString>,
    error_count: u64,
}

impl ConnectionFlow {
    pub(super) fn start_authorizing(
        &mut self,
        credentials: ClientCredentials,
        environment: Environment,
        client: Client,
        tokens: TokenSet,
        account: TraderAccount,
        cx: &mut Context<Self>,
    ) {
        self.screen = Screen::Authorizing(AuthorizingState {
            account: account.clone(),
            error: None,
            error_count: 0,
        });
        cx.notify();

        let account_id = account.ctid_trader_account_id;
        let access_token = tokens.access_token.expose_secret().to_string();

        cx.spawn(async move |this, cx| {
            let authorize_client = client.clone();
            let authorized = runtime::spawn(async move {
                authorize_client
                    .account(account_id)
                    .authorize(&access_token)
                    .await
            })
            .await;

            let error_message = match authorized {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error.to_string()),
                Err(_) => Some("the background runtime dropped the request".to_string()),
            };
            if let Some(message) = error_message {
                let _ = this.update(cx, |this, cx| {
                    if let Screen::Authorizing(state) = &mut this.screen {
                        state.error = Some(message.into());
                        state.error_count += 1;
                    }
                    cx.notify();
                });
                return;
            }

            let _ = this.update(cx, |this, cx| {
                // The user may have gone back while this was in flight.
                if !matches!(this.screen, Screen::Authorizing(_)) {
                    return;
                }
                match this.save_connected_profile(&credentials, environment, &account, &tokens) {
                    Ok(profile_id) => {
                        this.screen =
                            Screen::Connected(ConnectedState::new(profile_id, account, client));
                    }
                    Err(error) => {
                        if let Screen::Authorizing(state) = &mut this.screen {
                            state.error_count += 1;
                            state.error = Some(
                                format!("connected, but couldn't save the profile: {error}").into(),
                            );
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save_connected_profile(
        &mut self,
        credentials: &ClientCredentials,
        environment: Environment,
        account: &TraderAccount,
        tokens: &TokenSet,
    ) -> wyck::config::Result<wyck::config::ProfileId> {
        let service = format!(
            "ctrader-openapi-{}",
            if environment == Environment::Live {
                "live"
            } else {
                "demo"
            }
        );
        let broker = account.broker_title_short.as_deref().unwrap_or("cTrader");
        let login = account
            .trader_login
            .map(|login| login.to_string())
            .unwrap_or_else(|| account.ctid_trader_account_id.to_string());
        let display_name = format!(
            "{broker} {} - {login}",
            if account.is_live.unwrap_or(false) {
                "Live"
            } else {
                "Demo"
            }
        );

        let id = self.config.add_profile(display_name, service, None, None)?;
        self.config.set_openapi_profile(
            &id,
            credentials.client_id.clone(),
            CALLBACK_PORT,
            account.ctid_trader_account_id,
        )?;
        self.config
            .set_profile_secret(&id, "client-secret", &credentials.client_secret)?;
        self.config.save_openapi_tokens(
            &id,
            &OpenApiTokens {
                access_token: tokens.access_token.clone(),
                refresh_token: tokens.refresh_token.clone(),
                expires_at: tokens.expires_at(),
            },
        )?;
        self.config.set_active_profile(Some(id.clone()))?;
        Ok(id)
    }

    pub(super) fn render_authorizing(
        &self,
        state: &AuthorizingState,
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
                            "authorizing-error",
                            state.error_count,
                            "Couldn't finish connecting",
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
                            .when(!failed, |el| {
                                el.child(div().absolute().child(anim::ping(
                                    div().rounded_full().bg(theme::accent()),
                                    ("authorizing-ping", epoch),
                                    88.,
                                )))
                            })
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
                                    .child(if failed {
                                        ui::icon(IconName::ShieldAlert, 30.).into_any_element()
                                    } else {
                                        anim::spin(
                                            ui::icon(IconName::LoaderCircle, 30.),
                                            ("authorizing-spin", epoch),
                                        )
                                        .into_any_element()
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(20.))
                            .text_color(theme::fg())
                            .child("Connecting your account"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(theme::muted_fg())
                            .child(format!(
                                "Authorizing {} - {}",
                                state
                                    .account
                                    .broker_title_short
                                    .as_deref()
                                    .unwrap_or("cTrader"),
                                state.account.ctid_trader_account_id
                            )),
                    )
                    .when(failed, |el| {
                        el.child(div().w_full().pt_2().child(ui::secondary_button(
                            "authorizing-start-over",
                            "Start over",
                            cx.listener(|this, _event, _window, cx| this.go_to_credentials(cx)),
                        )))
                    }),
            )
            .child(ui::back_button(
                "authorizing-back",
                cx.listener(|this, _event, _window, cx| this.go_to_credentials(cx)),
            ))
    }
}

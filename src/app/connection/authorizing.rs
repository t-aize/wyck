//! Screen 10: the last automatic step, authorizing the chosen account on the connection and
//! saving everything to disk.

use gpui::prelude::*;
use gpui::{Context, SharedString, Window, div, px};
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
use super::{ConnectionFlow, Screen, theme};
use crate::app::runtime;

pub(super) struct AuthorizingState {
    account: TraderAccount,
    error: Option<SharedString>,
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
                    }
                    cx.notify();
                });
                return;
            }

            let _ = this.update(cx, |this, cx| {
                match this.save_connected_profile(&credentials, environment, &account, &tokens) {
                    Ok(profile_id) => {
                        this.screen =
                            Screen::Connected(ConnectedState::new(profile_id, account, client));
                    }
                    Err(error) => {
                        if let Screen::Authorizing(state) = &mut this.screen {
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
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div().flex().flex_1().items_center().justify_center().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_4()
                .w(px(340.))
                .children(
                    state
                        .error
                        .clone()
                        .map(|message| ui::error_banner("Couldn't finish connecting", message)),
                )
                .child(
                    div()
                        .text_size(px(17.))
                        .text_color(theme::fg())
                        .child("Connecting your account"),
                )
                .child(
                    div()
                        .text_size(px(12.))
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
                ),
        )
    }
}

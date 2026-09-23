//! Screen 2 (and 2b, the same screen with an error banner): the user's own cTrader Open API
//! application credentials.

use gpui::prelude::*;
use gpui::{ClickEvent, Context, Entity, SharedString, Window, div, px};
use wyck::openapi::auth::CallbackListener;
use wyck::openapi::config::ClientCredentials;
use wyck::openapi::{ClientBuilder, Environment};

use super::ui;
use super::{ConnectionFlow, Screen, theme};
use crate::app::runtime;
use crate::app::text_input::TextInput;

/// The local port the OAuth redirect listener binds. Must match a redirect URI
/// (`http://localhost:<port>`) registered for the user's cTrader Open API application.
pub const CALLBACK_PORT: u16 = 8765;

pub(super) struct CredentialsState {
    client_id: Entity<TextInput>,
    client_secret: Entity<TextInput>,
    environment: Environment,
    error: Option<SharedString>,
    connecting: bool,
}

impl CredentialsState {
    pub(super) fn new(cx: &mut Context<ConnectionFlow>) -> Self {
        Self {
            client_id: cx.new(|cx| TextInput::new(cx, "e.g. 3492_oXQnP7vLk9fZ2mR8bQwT")),
            client_secret: cx
                .new(|cx| TextInput::new(cx, "your application's client secret").masked(true)),
            environment: Environment::Demo,
            error: None,
            connecting: false,
        }
    }
}

impl ConnectionFlow {
    pub(super) fn render_credentials(
        &self,
        state: &CredentialsState,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let redirect_uri = format!("http://localhost:{CALLBACK_PORT}");
        let can_submit = !state.connecting;
        let has_error = state.error.is_some();

        div().flex().flex_1().items_center().justify_center().child(
            ui::card()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(17.))
                                .text_color(theme::fg())
                                .child("Application credentials"),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(theme::muted_fg())
                                .child(
                                    "From your own application on cTrader Connect. Wyck stores \
                                     these on this device only.",
                                ),
                        ),
                )
                .children(state.error.clone().map(|message| {
                    ui::error_banner("Wyck couldn't verify this application", message)
                }))
                .child(ui::field("Client ID", state.client_id.clone()))
                .child(ui::field("Client secret", state.client_secret.clone()))
                .when(has_error, |el| {
                    el.child(ui::field_error("Client ID or secret is invalid"))
                })
                .child(ui::field(
                    "Redirect URI",
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .h(px(36.))
                        .px_3()
                        .rounded_lg()
                        .bg(theme::bg())
                        .border_1()
                        .border_color(theme::border_subtle())
                        .text_size(px(13.))
                        .text_color(theme::muted_fg())
                        .child(redirect_uri.clone())
                        .child(
                            div()
                                .id("copy-redirect-uri")
                                .text_color(theme::muted_fg())
                                .cursor_pointer()
                                .hover(|style| style.text_color(theme::fg()))
                                .on_click({
                                    let redirect_uri = redirect_uri.clone();
                                    move |_event: &ClickEvent, _window, cx: &mut gpui::App| {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            redirect_uri.clone(),
                                        ));
                                    }
                                })
                                .child("Copy"),
                        ),
                ))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::muted_fg())
                        .child(
                            "Add this exact URI to your application's redirect list on cTrader \
                             Connect: Wyck listens on it locally to catch the callback.",
                        ),
                )
                .child(ui::field(
                    "Environment",
                    environment_switch(state.environment, cx),
                ))
                .child(ui::primary_button(
                    "credentials-continue",
                    if state.connecting {
                        "Verifying..."
                    } else {
                        "Continue"
                    },
                    cx.listener(move |this, _event, window, cx| {
                        if can_submit {
                            this.submit_credentials(window, cx);
                        }
                    }),
                )),
        )
    }

    fn submit_credentials(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Screen::Credentials(state) = &mut self.screen else {
            return;
        };
        let client_id = state.client_id.read(cx).text().trim().to_string();
        let client_secret = state.client_secret.read(cx).text().trim().to_string();
        let environment = state.environment;

        if client_id.is_empty() || client_secret.is_empty() {
            state.error = Some("Client ID and Client Secret are both required".into());
            cx.notify();
            return;
        }

        state.connecting = true;
        state.error = None;
        cx.notify();

        let credentials = ClientCredentials::new(client_id, client_secret);

        cx.spawn(async move |this, cx| {
            let bind = runtime::spawn(CallbackListener::bind(CALLBACK_PORT)).await;
            let listener = match bind {
                Ok(Ok(listener)) => listener,
                Ok(Err(error)) => {
                    report_error(
                        &this,
                        cx,
                        format!(
                            "couldn't start the local sign-in listener on port {CALLBACK_PORT}: {error}"
                        ),
                    );
                    return;
                }
                Err(_) => {
                    report_error(&this, cx, "the background runtime dropped the request".into());
                    return;
                }
            };

            let connect_creds = credentials.clone();
            let connected = runtime::spawn(async move {
                ClientBuilder::new(environment)
                    .credentials(connect_creds)
                    .connect()
                    .await
            })
            .await;

            let client = match connected {
                Ok(Ok(client)) => client,
                Ok(Err(error)) => {
                    report_error(
                        &this,
                        cx,
                        format!("cTrader rejected the Client ID or Client Secret: {error}"),
                    );
                    return;
                }
                Err(_) => {
                    report_error(&this, cx, "the background runtime dropped the request".into());
                    return;
                }
            };

            let _ = this.update(cx, |this, cx| {
                this.start_browser_handoff(credentials, environment, client, listener, cx);
            });
        })
        .detach();
    }
}

fn report_error(weak: &gpui::WeakEntity<ConnectionFlow>, cx: &mut gpui::AsyncApp, message: String) {
    let _ = weak.update(cx, |this, cx| {
        if let Screen::Credentials(state) = &mut this.screen {
            state.connecting = false;
            state.error = Some(message.into());
        }
        cx.notify();
    });
}

fn environment_switch(current: Environment, cx: &mut Context<ConnectionFlow>) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .p(px(3.))
        .gap_1()
        .rounded_lg()
        .bg(theme::bg())
        .border_1()
        .border_color(theme::border_subtle())
        .child(environment_option(
            "env-demo",
            "Demo",
            Environment::Demo,
            current,
            cx,
        ))
        .child(environment_option(
            "env-live",
            "Live",
            Environment::Live,
            current,
            cx,
        ))
}

fn environment_option(
    id: &'static str,
    label: &'static str,
    value: Environment,
    current: Environment,
    cx: &mut Context<ConnectionFlow>,
) -> impl IntoElement {
    let selected = value == current;
    div()
        .id(id)
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .h(px(28.))
        .rounded_md()
        .when(selected, |el| el.bg(theme::accent_selected()))
        .text_size(px(13.))
        .text_color(if selected {
            theme::fg()
        } else {
            theme::muted_fg()
        })
        .cursor_pointer()
        .on_click(cx.listener(move |this, _event, _window, cx| {
            if let Screen::Credentials(state) = &mut this.screen {
                state.environment = value;
                cx.notify();
            }
        }))
        .child(label)
}

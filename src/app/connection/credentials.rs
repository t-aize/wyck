//! Screen 2 (and 2b, the same screen with an error banner): the user's own cTrader Open API
//! application credentials.

use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt, ClickEvent, Context, Entity, SharedString, Window, div, px, relative,
};
use gpui_kit::assets::IconName;
use wyck::openapi::auth::CallbackListener;
use wyck::openapi::config::ClientCredentials;
use wyck::openapi::{ClientBuilder, Environment};

use super::ui;
use super::{ConnectionFlow, Screen, anim, theme};
use crate::app::runtime;
use crate::app::text_input::TextInput;

/// The local port the OAuth redirect listener binds. Must match a redirect URI
/// (`http://localhost:<port>`) registered for the user's cTrader Open API application.
pub const CALLBACK_PORT: u16 = 8765;

pub(super) struct CredentialsState {
    client_id: Entity<TextInput>,
    client_secret: Entity<TextInput>,
    environment: Environment,
    /// The environment selected before the latest click, so the segmented control's highlight
    /// can slide from where it was to where it is.
    previous_environment: Environment,
    environment_changes: u64,
    error: Option<SharedString>,
    /// Counts errors shown so far, so each new one replays the banner's entrance.
    error_count: u64,
    connecting: bool,
    copied: bool,
}

impl CredentialsState {
    pub(super) fn new(cx: &mut Context<ConnectionFlow>) -> Self {
        Self {
            client_id: cx.new(|cx| TextInput::new(cx, "e.g. 3492_oXQnP7vLk9fZ2mR8bQwT")),
            client_secret: cx
                .new(|cx| TextInput::new(cx, "your application's client secret").masked(true)),
            environment: Environment::Demo,
            previous_environment: Environment::Demo,
            environment_changes: 0,
            error: None,
            error_count: 0,
            connecting: false,
            copied: false,
        }
    }

    fn set_error(&mut self, message: SharedString) {
        self.error = Some(message);
        self.error_count += 1;
    }
}

impl ConnectionFlow {
    pub(super) fn render_credentials(
        &self,
        state: &CredentialsState,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let epoch = self.epoch;
        let redirect_uri = super::rules::redirect_uri(CALLBACK_PORT);
        let can_submit = !state.connecting;
        let copied = state.copied;

        ui::screen()
            .child(
                ui::card()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_4()
                            .child(ui::icon_tile(
                                IconName::KeyRound,
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
                                            .text_size(px(18.))
                                            .text_color(theme::fg())
                                            .child("Application credentials"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(theme::muted_fg())
                                            .child(
                                                "From your own application on cTrader Connect. \
                                                 Wyck stores these on this device only.",
                                            ),
                                    ),
                            ),
                    )
                    .children(state.error.clone().map(|message| {
                        ui::error_banner(
                            "credentials-error",
                            state.error_count,
                            "Wyck couldn't verify this application",
                            message,
                        )
                    }))
                    .child(ui::field(
                        IconName::Hash,
                        "Client ID",
                        state.client_id.clone(),
                    ))
                    .child(ui::field(
                        IconName::Lock,
                        "Client secret",
                        state.client_secret.clone(),
                    ))
                    .child(ui::field(
                        IconName::Link,
                        "Redirect URI",
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .h(px(44.))
                            .px_3p5()
                            .rounded_lg()
                            .bg(theme::bg())
                            .border_1()
                            .border_color(theme::border_subtle())
                            .text_size(px(13.))
                            .text_color(theme::muted_fg())
                            .child(div().flex_1().truncate().child(redirect_uri.clone()))
                            .child(copy_button(redirect_uri, copied, epoch, cx)),
                    ))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::muted_fg())
                            .child(
                                "Add this exact URI to your application's redirect list on \
                                 cTrader Connect: Wyck listens on it locally to catch the \
                                 callback.",
                            ),
                    )
                    .child(ui::field(
                        IconName::Globe,
                        "Environment",
                        environment_switch(state, cx),
                    ))
                    .child(
                        ui::primary_button(
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
                        )
                        .loading(state.connecting)
                        .when(state.connecting, |button| button.cursor_not_allowed()),
                    ),
            )
            .child(ui::back_button(
                "credentials-back",
                cx.listener(|this, _event, _window, cx| this.go_to_welcome(cx)),
            ))
    }

    fn submit_credentials(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Screen::Credentials(state) = &mut self.screen else {
            return;
        };
        let client_id = state.client_id.read(cx).text().trim().to_string();
        let client_secret = state.client_secret.read(cx).text().trim().to_string();
        let environment = state.environment;

        if client_id.is_empty() || client_secret.is_empty() {
            state.set_error("Client ID and Client Secret are both required".into());
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
            state.set_error(message.into());
        }
        cx.notify();
    });
}

/// The "Copy" control next to the redirect URI. Flips to a green "Copied" for a moment.
fn copy_button(
    text: String,
    copied: bool,
    epoch: u64,
    cx: &mut Context<ConnectionFlow>,
) -> impl IntoElement {
    let content = div()
        .flex()
        .items_center()
        .gap_1p5()
        .child(ui::icon_colored(
            if copied {
                IconName::Check
            } else {
                IconName::Copy
            },
            14.,
            if copied {
                theme::emerald()
            } else {
                theme::muted_fg()
            },
        ))
        .child(if copied { "Copied" } else { "Copy" });

    div()
        .id("copy-redirect-uri")
        .flex_shrink_0()
        .text_color(if copied {
            theme::emerald()
        } else {
            theme::muted_fg()
        })
        .cursor_pointer()
        .hover(|style| style.text_color(theme::fg()))
        .active(|style| style.opacity(0.7))
        .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
            if let Screen::Credentials(state) = &mut this.screen {
                state.copied = true;
                cx.notify();
            }
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(1600))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if let Screen::Credentials(state) = &mut this.screen {
                        state.copied = false;
                        cx.notify();
                    }
                });
            })
            .detach();
        }))
        .child(if copied {
            // Pops in each time it flips to "Copied".
            anim::enter(content, ("copied", epoch), 0).into_any_element()
        } else {
            content.into_any_element()
        })
}

/// A two-option segmented control whose highlight slides between the options.
fn environment_switch(
    state: &CredentialsState,
    cx: &mut Context<ConnectionFlow>,
) -> impl IntoElement {
    let position = |environment: Environment| {
        if environment == Environment::Live {
            0.5
        } else {
            0.0
        }
    };
    let (from, to) = (
        position(state.previous_environment),
        position(state.environment),
    );

    let highlight = div()
        .absolute()
        .top_0()
        .h_full()
        .w(relative(0.5))
        .rounded_md()
        .bg(theme::accent_selected())
        .with_animation(
            ("environment-slide", state.environment_changes),
            Animation::new(Duration::from_millis(240)),
            move |el, delta| el.left(relative(anim::lerp(from, to, anim::ease_out_cubic(delta)))),
        );

    div()
        .p(px(3.))
        .rounded_lg()
        .bg(theme::bg())
        .border_1()
        .border_color(theme::border_subtle())
        .child(
            div()
                .relative()
                .flex()
                .flex_row()
                .child(highlight)
                .child(environment_option(
                    "env-demo",
                    IconName::FlaskConical,
                    "Demo",
                    Environment::Demo,
                    state.environment,
                    cx,
                ))
                .child(environment_option(
                    "env-live",
                    IconName::Zap,
                    "Live",
                    Environment::Live,
                    state.environment,
                    cx,
                )),
        )
}

fn environment_option(
    id: &'static str,
    icon: IconName,
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
        .gap_2()
        .h(px(34.))
        .rounded_md()
        .text_size(px(13.))
        .text_color(if selected {
            theme::fg()
        } else {
            theme::muted_fg()
        })
        .cursor_pointer()
        .when(!selected, |el| {
            el.hover(|style| style.bg(theme::surface_hover()).text_color(theme::fg()))
        })
        .active(|style| style.opacity(0.7))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            if let Screen::Credentials(state) = &mut this.screen
                && state.environment != value
            {
                state.previous_environment = state.environment;
                state.environment = value;
                state.environment_changes += 1;
                cx.notify();
            }
        }))
        .child(ui::icon_colored(
            icon,
            14.,
            if selected {
                theme::fg()
            } else {
                theme::muted_fg()
            },
        ))
        .child(label)
}

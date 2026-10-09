//! The root view of the window: the dashboard once there is a session, and until then a soft
//! frame of it under the sign-in modal.
//!
//! [`crate::app::sign_in::SignIn`] decides when a session can open; this view opens it, shows the
//! dashboard on it and goes back to the modal when the session ends.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, Subscription, Window, div, px,
};

use crate::app::appearance;
use crate::app::broker::session::Session;
use crate::app::broker::{ConnectionConfig, Environment};
use crate::app::sign_in::{Connection, SignIn, SignInEvent, load_config};
use crate::app::storage::ProfileId;
use crate::app::system::runtime;
use crate::ui::kit::prelude::Root;
use crate::ui::kit::{button, layout, theme, toast, tokens};
use crate::ui::shell::dashboard::{AccountInfo, Dashboard, DashboardEvent};
use crate::ui::shell::locked_frame;
use crate::ui::shell::sign_in_gate::SignInGate;

/// The session on screen.
struct Open {
    view: Entity<Dashboard>,
    profile: Option<ProfileId>,
    _subscription: Subscription,
}

/// What the window shows before anything else can.
enum Start {
    /// The settings could not be opened.
    Failed(String),
    Ready {
        gate: Entity<SignInGate>,
        _subscription: Subscription,
    },
}

/// The root view.
pub struct AppView {
    start: Start,
    open: Option<Open>,
    focus_handle: FocusHandle,
}

impl AppView {
    /// Opens the settings and starts looking for a saved session.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let start = match load_config() {
            Ok(config) => {
                let sign_in = cx.new(|_| SignIn::new(config));
                let gate = cx.new(|cx| SignInGate::new(sign_in.clone(), window, cx));
                let subscription = cx.subscribe(&sign_in, |this, sign_in, event, cx| match event {
                    SignInEvent::Connected(connection) => {
                        this.open_session(&sign_in, connection, cx);
                    }
                });
                sign_in.update(cx, |sign_in, cx| sign_in.restore(cx));
                Start::Ready {
                    gate,
                    _subscription: subscription,
                }
            }
            Err(error) => {
                tracing::error!(%error, "could not open the settings");
                Start::Failed(error.to_string())
            }
        };
        Self {
            start,
            open: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Starts the session of `connection` and shows the dashboard on it.
    fn open_session(
        &mut self,
        sign_in: &Entity<SignIn>,
        connection: &Connection,
        cx: &mut Context<Self>,
    ) {
        let config = crate::app::broker::session::SessionConfig::new(
            ConnectionConfig::new(connection.environment),
            connection.credentials.clone(),
            connection.account_id,
        );
        // The session's supervisor is a tokio task, so it has to be started inside the runtime.
        let started = {
            let _guard = runtime::handle().enter();
            Session::start(config, connection.tokens.clone(), connection.store.clone())
        };
        let session = match started {
            Ok(session) => session,
            Err(error) => {
                tracing::error!(%error, "could not start the session");
                sign_in.update(cx, |sign_in, cx| {
                    sign_in.session_failed(error.to_string(), cx);
                });
                return;
            }
        };
        if let Some(reason) = &connection.not_saved {
            toast::Toast::new(
                toast::Kind::Warning,
                "Sign-in not saved",
                "You are connected, but Wyck could not keep the sign-in on this device. You will sign in again next time.",
            )
            .details_opt(Some(reason.clone()))
            .show(cx);
        }

        let account = AccountInfo {
            label: connection.label.clone().into(),
            is_live: connection.environment == Environment::Live,
        };
        let dashboard = cx.new(|cx| {
            Dashboard::new(
                session,
                account,
                connection.initial_symbol.clone(),
                connection.documents.clone(),
                cx,
            )
        });
        let profile = connection.profile_id.clone();
        let sign_in = sign_in.clone();
        let subscription = cx.subscribe(&dashboard, move |this, dashboard, event, cx| {
            this.on_dashboard_event(&sign_in, dashboard, event, cx);
        });
        self.open = Some(Open {
            view: dashboard,
            profile,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn on_dashboard_event(
        &mut self,
        sign_in: &Entity<SignIn>,
        dashboard: Entity<Dashboard>,
        event: &DashboardEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            DashboardEvent::SymbolChosen(name) => {
                sign_in.update(cx, |sign_in, _| sign_in.remember_symbol(name));
            }
            DashboardEvent::Disconnect => {
                dashboard.read(cx).stop();
                let profile = self.open.take().and_then(|open| open.profile);
                sign_in.update(cx, |sign_in, cx| sign_in.forget(profile.as_ref(), cx));
                self.show_gate_again(false, cx);
            }
            DashboardEvent::SignInAgain => {
                dashboard.read(cx).stop();
                self.open = None;
                sign_in.update(cx, |sign_in, cx| sign_in.session_ended(cx));
                self.show_gate_again(true, cx);
            }
        }
        cx.notify();
    }

    /// The modal comes back with the saved Client ID filled in, and the secret too when the session
    /// only ended.
    fn show_gate_again(&mut self, with_secret: bool, cx: &mut Context<Self>) {
        let Start::Ready { gate, .. } = &self.start else {
            return;
        };
        let gate = gate.clone();
        cx.defer(move |cx| {
            let Some(window) = cx.active_window() else {
                return;
            };
            let _ = window.update(cx, |_, window, cx| {
                gate.update(cx, |gate, cx| gate.prefill(with_secret, window, cx));
            });
        });
    }

    fn failed(&self, message: &str) -> AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                layout::card()
                    .child(
                        div()
                            .text_size(px(tokens::text::display()))
                            .text_color(theme::fg())
                            .child("Wyck can't open its settings"),
                    )
                    .child(
                        div()
                            .text_size(px(tokens::text::emphasis()))
                            .text_color(theme::muted_fg())
                            .child(message.to_owned()),
                    )
                    .child(button::primary(
                        "startup-quit",
                        "Quit Wyck",
                        |_, _window, cx| cx.quit(),
                    )),
            )
            .into_any_element()
    }
}

impl Focusable for AppView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match (&self.open, &self.start) {
            (Some(open), _) => open.view.clone().into_any_element(),
            (None, Start::Failed(message)) => self.failed(message),
            (None, Start::Ready { gate, .. }) => div()
                .relative()
                .size_full()
                .child(locked_frame::render())
                .child(gate.clone())
                .into_any_element(),
        };

        div()
            .track_focus(&self.focus_handle)
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::bg())
            .text_color(theme::fg())
            .font_family(appearance::font(cx))
            // Only where the system draws no title bar (a Wayland desktop without server-side
            // decorations): there the window would have no title and no buttons.
            .children(crate::ui::kit::window_bar::fallback(window))
            // `min_h_0`: without it this box never gets shorter than what the screen inside asks
            // for, and a tall bottom panel pushes the dashboard out of the window.
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(content),
            )
            // Dialogs and notices of gpui-component draw in these layers, over everything.
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_dialog_layer(window, cx))
            // The settings panels, over the dialogs and under the notices.
            .child(crate::ui::kit::modal::host(cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

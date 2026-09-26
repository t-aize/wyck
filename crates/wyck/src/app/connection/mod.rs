//! The cTrader connection flow: welcome, application credentials, the browser hand-off, account
//! selection and authorizing, ending on the dashboard.
//!
//! [`ConnectionFlow`] is the root view: one [`Screen`] is active at a time, and each screen that
//! needs its own state (text inputs, an in-progress error) carries it inline. A screen transition
//! is just `self.screen = Screen::Whatever(state); cx.notify();`, from a plain callback or from
//! the tail of a `cx.spawn` future once real work (an HTTP call, the OAuth round trip) settles.
//!
//! The sign-in is kept: on the next start the app finds the saved connection and goes straight to
//! the dashboard (see [`ConnectionFlow::restore`]).
//!
mod authorizing;
mod browser_handoff;
mod credentials;
mod rules;
mod select_account;
mod stepper;
pub(crate) mod ui;
mod welcome;

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, SharedString, Subscription, Window,
    div,
};
use secrecy::ExposeSecret;
use wyck_config::{AppPaths, KeyringSecretStore, ProfileId, WyckConfig};
use wyck_openapi::auth::TokenSet;
use wyck_openapi::config::ClientCredentials;
use wyck_openapi::session::{Session, SessionConfig, TokenStore};
use wyck_openapi::{ConnectionConfig, Environment};

use gpui_kit::component::Root;

use super::dashboard::{AccountInfo, Dashboard, DashboardEvent};
use super::token_store::{ConfigTokenStore, to_token_set};
use super::workspace::Documents;
use super::{anim, runtime, theme};

enum Screen {
    Welcome,
    Credentials(credentials::CredentialsState),
    BrowserHandoff(browser_handoff::BrowserHandoffState),
    SelectAccount(select_account::SelectAccountState),
    Authorizing(authorizing::AuthorizingState),
    /// The connected dashboard, and the subscription to what it reports.
    Dashboard(Entity<Dashboard>, #[allow(dead_code)] Subscription),
}

use self::rules::service_tag;

/// Everything needed to open a session again, read back from the saved profile.
struct SavedConnection {
    profile_id: ProfileId,
    label: SharedString,
    environment: Environment,
    credentials: ClientCredentials,
    account_id: i64,
    tokens: TokenSet,
}

/// The root view: whichever screen is active.
pub struct ConnectionFlow {
    config: WyckConfig,
    screen: Screen,
    focus_handle: FocusHandle,
    /// Bumped every time a different screen becomes active. Animation ids include it, so a
    /// screen's entrance (and anything staggered inside it) replays on each visit instead of
    /// only the first.
    epoch: u64,
    last_screen: Option<std::mem::Discriminant<Screen>>,
}

impl ConnectionFlow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut flow = Self {
            config: load_config(),
            screen: Screen::Welcome,
            focus_handle: cx.focus_handle(),
            epoch: 0,
            last_screen: None,
        };
        flow.restore(cx);
        flow
    }

    /// If a connection was saved by an earlier run, opens the dashboard on it, so the user does
    /// not sign in again. Anything missing or unreadable (the secret was deleted from the OS
    /// keyring, the profile predates Open API) just leaves the welcome screen.
    fn restore(&mut self, cx: &mut Context<Self>) {
        let Some(saved) = self.saved_connection() else {
            return;
        };
        tracing::info!(profile = ?saved.profile_id, "restoring the saved connection");
        self.start_dashboard(saved, cx);
    }

    fn saved_connection(&self) -> Option<SavedConnection> {
        let profile = rules::saved_profile(self.config.active_profile(), self.config.profiles())?;
        let environment = rules::environment_of(&profile.service)?;
        let client_id = profile.client_id.clone()?;
        let account_id = profile.account_id?;

        let secret = match self.config.profile_secret(&profile.id, "client-secret") {
            Ok(Some(secret)) => secret,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(%error, "could not read the saved client secret");
                return None;
            }
        };
        let tokens = match self.config.openapi_tokens(&profile.id) {
            Ok(Some(tokens)) => tokens,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(%error, "could not read the saved tokens");
                return None;
            }
        };
        Some(SavedConnection {
            profile_id: profile.id.clone(),
            label: profile.display_name.clone().into(),
            environment,
            credentials: ClientCredentials::new(client_id, secret.expose_secret()),
            account_id,
            tokens: to_token_set(tokens),
        })
    }

    /// Opens a session for the connection and shows the dashboard on it.
    fn start_dashboard(&mut self, saved: SavedConnection, cx: &mut Context<Self>) {
        let SavedConnection {
            profile_id,
            label,
            environment,
            credentials,
            account_id,
            tokens,
        } = saved;

        let connection = ConnectionConfig::new(environment);
        let config = SessionConfig::new(connection, credentials, account_id);
        let store: Arc<dyn TokenStore> = Arc::new(ConfigTokenStore::new(
            self.config.openapi_token_storage(&profile_id),
        ));
        // The session's supervisor is a tokio task, so it has to be started inside the runtime.
        let session = {
            let _guard = runtime::handle().enter();
            Session::start(config, tokens, store)
        };
        let session = match session {
            Ok(session) => session,
            Err(error) => {
                tracing::error!(%error, "could not start the session");
                self.go_to_welcome(cx);
                return;
            }
        };

        let account = AccountInfo {
            label,
            is_live: environment == Environment::Live,
        };
        let initial_symbol = self.config.last_symbol().map(str::to_owned);
        // What the user arranges is kept per account number, so signing out and in again finds
        // the watchlists where they were.
        let scope = rules::document_scope(environment, account_id);
        let documents = Documents {
            global: self.config.global_documents(),
            account: self.config.scoped_documents(&scope),
        };
        let dashboard =
            cx.new(|cx| Dashboard::new(session, account, initial_symbol, documents, cx));
        let subscription = cx.subscribe(&dashboard, move |this, dashboard, event, cx| {
            this.on_dashboard_event(&profile_id, &dashboard, event, cx);
        });
        self.screen = Screen::Dashboard(dashboard, subscription);
        cx.notify();
    }

    fn on_dashboard_event(
        &mut self,
        profile_id: &ProfileId,
        dashboard: &Entity<Dashboard>,
        event: &DashboardEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            DashboardEvent::SymbolChosen(name) => {
                if let Err(error) = self.config.set_last_symbol(Some(name.clone())) {
                    tracing::warn!(%error, "could not remember the symbol");
                }
            }
            DashboardEvent::Disconnect => {
                dashboard.read(cx).stop();
                if let Err(error) = self.config.remove_profile(profile_id) {
                    tracing::warn!(%error, "failed to remove the profile on disconnect");
                }
                self.go_to_welcome(cx);
            }
            DashboardEvent::SignInAgain => {
                dashboard.read(cx).stop();
                self.go_to_credentials(cx);
            }
        }
    }

    /// The index of the sign-in step the active screen belongs to, for the progress indicator;
    /// `None` on screens outside the sign-in sequence.
    fn step_index(&self) -> Option<usize> {
        match self.screen {
            Screen::Credentials(_) => Some(0),
            Screen::BrowserHandoff(_) => Some(1),
            Screen::SelectAccount(_) => Some(2),
            Screen::Authorizing(_) => Some(3),
            Screen::Welcome | Screen::Dashboard(..) => None,
        }
    }

    fn go_to_welcome(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Welcome;
        cx.notify();
    }

    fn go_to_credentials(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Credentials(credentials::CredentialsState::new(cx));
        cx.notify();
    }

    fn render_active_screen(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match &self.screen {
            Screen::Welcome => self.render_welcome(window, cx).into_any_element(),
            Screen::Credentials(state) => self
                .render_credentials(state, window, cx)
                .into_any_element(),
            Screen::BrowserHandoff(state) => self
                .render_browser_handoff(state, window, cx)
                .into_any_element(),
            Screen::SelectAccount(state) => self
                .render_select_account(state, window, cx)
                .into_any_element(),
            Screen::Authorizing(state) => self
                .render_authorizing(state, window, cx)
                .into_any_element(),
            Screen::Dashboard(dashboard, _) => dashboard.clone().into_any_element(),
        }
    }
}

impl Focusable for ConnectionFlow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ConnectionFlow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let kind = std::mem::discriminant(&self.screen);
        if self.last_screen != Some(kind) {
            self.last_screen = Some(kind);
            self.epoch += 1;
        }
        let epoch = self.epoch;

        div()
            .track_focus(&self.focus_handle)
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::bg())
            .text_color(theme::fg())
            .font_family(super::appearance::font(cx))
            .child(anim::enter(
                div()
                    .flex()
                    .flex_1()
                    .child(self.render_active_screen(window, cx)),
                ("screen", epoch),
                0,
            ))
            .children(
                self.step_index()
                    .map(|current| stepper::stepper(current, epoch)),
            )
            // Dialogs and notices of gpui-component draw in these layers, over everything.
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_dialog_layer(window, cx))
            // The settings panels, over the dialogs and under the notices.
            .child(super::modal::host(cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

/// Loads the on-disk config, falling back to the OS keyring for secret storage. This is the one
/// place the app decides where its state lives; every screen goes through `self.config` instead
/// of touching [`wyck_config`] directly.
fn load_config() -> WyckConfig {
    let paths = AppPaths::discover().expect("could not resolve the app's config directories");
    WyckConfig::load(paths, Box::new(KeyringSecretStore::default()))
        .expect("could not load or initialize the app config")
}

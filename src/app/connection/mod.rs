//! The cTrader connection flow: welcome, application credentials, the browser hand-off, account
//! selection, authorizing and connected, plus managing an existing connection.
//!
//! [`ConnectionFlow`] is the root view: one [`Screen`] is active at a time, and each screen that
//! needs its own state (text inputs, an in-progress error) carries it inline. A screen transition
//! is just `self.screen = Screen::Whatever(state); cx.notify();`, from a plain callback or from
//! the tail of a `cx.spawn` future once real work (an HTTP call, the OAuth round trip) settles.

mod authorizing;
mod browser_handoff;
mod connected;
mod credentials;
mod manage;
mod select_account;
mod stepper;
mod ui;
mod welcome;

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, FocusHandle, Focusable, Window, div};
use wyck::config::{AppPaths, KeyringSecretStore, WyckConfig};

use super::{anim, theme};

enum Screen {
    Welcome,
    Credentials(credentials::CredentialsState),
    BrowserHandoff(browser_handoff::BrowserHandoffState),
    SelectAccount(select_account::SelectAccountState),
    Authorizing(authorizing::AuthorizingState),
    Connected(connected::ConnectedState),
    Manage(manage::ManageState),
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
        Self {
            config: load_config(),
            screen: Screen::Welcome,
            focus_handle: cx.focus_handle(),
            epoch: 0,
            last_screen: None,
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
            Screen::Connected(_) => Some(4),
            Screen::Welcome | Screen::Manage(_) => None,
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
            Screen::Connected(state) => self.render_connected(state, window, cx).into_any_element(),
            Screen::Manage(state) => self.render_manage(state, window, cx).into_any_element(),
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
            .font_family("Inter")
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
    }
}

/// Loads the on-disk config, falling back to the OS keyring for secret storage. This is the one
/// place the app decides where its state lives; every screen goes through `self.config` instead
/// of touching [`wyck::config`] directly.
fn load_config() -> WyckConfig {
    let paths = AppPaths::discover().expect("could not resolve the app's config directories");
    WyckConfig::load(paths, Box::new(KeyringSecretStore::default()))
        .expect("could not load or initialize the app config")
}

// TODO: if `config.active_profile()` already has a saved access/refresh token, skip straight to
// a "reconnecting" screen instead of Welcome, so relaunching the app doesn't ask to sign in
// again every time. Needs a token-refresh path in `openapi::auth` wired up first.

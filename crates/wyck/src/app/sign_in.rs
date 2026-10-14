//! The sign-in: finding a saved session, or walking the user through cTrader's consent page and
//! the choice of an account, and handing a [`Connection`] to the shell.
//!
//! [`SignIn`] is an entity without a view. It owns the configuration, holds the pure
//! [`SignInState`] and runs the network steps in the background; the modal in `ui::shell` shows
//! its phase and calls its methods. A step that finishes after the user cancelled finds that the
//! attempt number changed and does nothing, and cancelling aborts the wait for the redirect, so
//! the local port is free again at once.

pub mod config;
mod consent;
pub mod rules;
pub mod state;

use std::sync::Arc;

use gpui::{Context, EventEmitter, Task};
use secrecy::ExposeSecret;
use tokio::task::AbortHandle;

use crate::app::storage::{
    CLIENT_SECRET, OpenApiTokenStorage, OpenApiTokens, ProfileId, ProfileSecretReader, WyckConfig,
};
use crate::app::token_store::{ConfigTokenStore, to_token_set};
use crate::app::workspace::Documents;
use wyck_ctrader::auth::TokenSet;
use wyck_ctrader::config::ClientCredentials;
use wyck_ctrader::session::{MemoryTokenStore, TokenStore};
use wyck_ctrader::{Client, Environment, TraderAccount};
use wyck_platform::runtime;

use self::rules::{account_label, document_scope, environment_of, saved_profile, service_tag};
pub use self::state::{AccountChoice, Failure, Phase, SignInState};

/// The local port the redirect listener binds. It must match a redirect URI
/// (`http://localhost:<port>`) registered for the user's cTrader application.
pub const CALLBACK_PORT: u16 = 8765;

/// Everything the shell needs to open a session.
pub struct Connection {
    /// The saved profile, or `None` when the user chose not to stay signed in.
    pub profile_id: Option<ProfileId>,
    /// The name of the account.
    pub label: String,
    /// Demo or live.
    pub environment: Environment,
    /// The application credentials.
    pub credentials: ClientCredentials,
    /// The account to trade on.
    pub account_id: i64,
    /// The tokens to start from.
    pub tokens: TokenSet,
    /// Where refreshed tokens go.
    pub store: Arc<dyn TokenStore>,
    /// The documents of the app and of this account.
    pub documents: Documents,
    /// The symbol shown last time.
    pub initial_symbol: Option<String>,
    /// Why the sign-in could not be saved on this device, when it could not.
    pub not_saved: Option<String>,
}

/// What [`SignIn`] reports.
pub enum SignInEvent {
    /// A session can be opened.
    Connected(Box<Connection>),
}

/// What the steps after the consent page need.
struct Pending {
    credentials: ClientCredentials,
    environment: Environment,
    remember: bool,
    client: Client,
    tokens: TokenSet,
    accounts: Vec<TraderAccount>,
}

/// A connection saved by an earlier run.
struct Saved {
    profile_id: ProfileId,
    label: String,
    environment: Environment,
    credentials: ClientCredentials,
    account_id: i64,
    tokens: TokenSet,
}

/// A saved connection whose secrets are still to be read from the keyring.
struct SavedPlan {
    profile_id: ProfileId,
    label: String,
    environment: Environment,
    client_id: String,
    account_id: i64,
    secret: ProfileSecretReader,
    tokens: OpenApiTokenStorage,
}

impl SavedPlan {
    /// Reads the client secret and the tokens. Blocks while the keyring answers.
    fn read(self) -> Option<Saved> {
        let secret = match self.secret.read() {
            Ok(Some(secret)) => secret,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(%error, "could not read the saved client secret");
                return None;
            }
        };
        let tokens = match self.tokens.load() {
            Ok(Some(tokens)) => tokens,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(%error, "could not read the saved tokens");
                return None;
            }
        };
        Some(Saved {
            profile_id: self.profile_id,
            label: self.label,
            environment: self.environment,
            credentials: ClientCredentials::new(self.client_id, secret.expose_secret()),
            account_id: self.account_id,
            tokens: to_token_set(tokens),
        })
    }
}

/// The sign-in entity.
pub struct SignIn {
    config: WyckConfig,
    state: SignInState,
    /// Bumped on every start and every cancel.
    attempt: u64,
    task: Option<Task<()>>,
    abort: Option<AbortHandle>,
    pending: Option<Pending>,
}

impl EventEmitter<SignInEvent> for SignIn {}

impl SignIn {
    /// A sign-in that will look for a saved session.
    pub fn new(config: WyckConfig) -> Self {
        Self {
            config,
            state: SignInState::restoring(),
            attempt: 0,
            task: None,
            abort: None,
            pending: None,
        }
    }

    /// The configuration, for what else the shell keeps in it.
    pub fn config(&self) -> &WyckConfig {
        &self.config
    }

    /// The state to show.
    pub fn state(&self) -> &SignInState {
        &self.state
    }

    /// The Client ID of the saved profile, to prefill the form.
    pub fn saved_client_id(&self) -> Option<String> {
        saved_profile(self.config.active_profile(), self.config.profiles())
            .and_then(|profile| profile.client_id.clone())
    }

    /// The Client secret of the saved profile, read from the system keyring. Used to fill the
    /// form again when only the consent has to be given again.
    pub fn saved_secret(&self) -> Option<String> {
        let profile = saved_profile(self.config.active_profile(), self.config.profiles())?;
        let secret = self
            .config
            .profile_secret(&profile.id, CLIENT_SECRET)
            .ok()
            .flatten()?;
        Some(secret.expose_secret().to_owned())
    }

    /// The environment of the saved profile.
    pub fn saved_environment(&self) -> Option<Environment> {
        saved_profile(self.config.active_profile(), self.config.profiles())
            .and_then(|profile| environment_of(&profile.service))
    }

    /// Looks for a saved session after the first frame, so the modal is already on screen while
    /// the keyring is read.
    pub fn restore(&mut self, cx: &mut Context<Self>) {
        let plan = self.saved_plan();
        self.task = Some(cx.spawn(async move |this, cx| {
            // The keyring can take a while to answer: read it off the thread of the windows.
            let saved = match plan {
                Some(plan) => {
                    cx.background_executor()
                        .spawn(async move { plan.read() })
                        .await
                }
                None => None,
            };
            let _ = this.update(cx, |this, cx| this.finish_restore(saved, cx));
        }));
    }

    fn finish_restore(&mut self, saved: Option<Saved>, cx: &mut Context<Self>) {
        match saved {
            Some(saved) => {
                tracing::info!(profile = ?saved.profile_id, "restoring the saved connection");
                let connection = self.connection_from_saved(saved);
                self.state.connected();
                cx.emit(SignInEvent::Connected(Box::new(connection)));
            }
            None => self.state.nothing_to_restore(),
        }
        cx.notify();
    }

    /// What can be read without the keyring about the saved connection, and where to read the rest.
    fn saved_plan(&self) -> Option<SavedPlan> {
        let profile = saved_profile(self.config.active_profile(), self.config.profiles())?;
        let environment = environment_of(&profile.service)?;
        let client_id = profile.client_id.clone()?;
        let account_id = profile.account_id?;
        let secret = match self
            .config
            .profile_secret_reader(&profile.id, CLIENT_SECRET)
        {
            Ok(reader) => reader,
            Err(error) => {
                tracing::warn!(%error, "could not find the saved client secret");
                return None;
            }
        };
        Some(SavedPlan {
            profile_id: profile.id.clone(),
            label: profile.display_name.clone(),
            environment,
            client_id,
            account_id,
            secret,
            tokens: self.config.openapi_token_storage(&profile.id),
        })
    }

    fn connection_from_saved(&self, saved: Saved) -> Connection {
        let store: Arc<dyn TokenStore> = Arc::new(ConfigTokenStore::new(
            self.config.openapi_token_storage(&saved.profile_id),
        ));
        self.connection(
            Some(saved.profile_id),
            saved.label,
            saved.environment,
            saved.credentials,
            saved.account_id,
            saved.tokens,
            store,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn connection(
        &self,
        profile_id: Option<ProfileId>,
        label: String,
        environment: Environment,
        credentials: ClientCredentials,
        account_id: i64,
        tokens: TokenSet,
        store: Arc<dyn TokenStore>,
        not_saved: Option<String>,
    ) -> Connection {
        // What the user arranges is kept per account number, so signing out and in again finds
        // the watchlists where they were.
        let scope = document_scope(environment, account_id);
        Connection {
            profile_id,
            label,
            environment,
            credentials,
            account_id,
            tokens,
            store,
            documents: Documents {
                global: self.config.documents(),
                account: self.config.scope(&scope),
            },
            initial_symbol: self.config.last_symbol().map(str::to_owned),
            not_saved,
        }
    }

    /// Sends the form: checks the application with cTrader, opens the consent page and waits for
    /// the redirect.
    pub fn submit(
        &mut self,
        client_id: &str,
        client_secret: &str,
        environment: Environment,
        remember: bool,
        cx: &mut Context<Self>,
    ) {
        if let Err(failure) = state::check_fields(client_id, client_secret) {
            self.state.fail(failure);
            cx.notify();
            return;
        }
        if !self.state.submit() {
            return;
        }
        self.attempt += 1;
        let attempt = self.attempt;
        let credentials =
            ClientCredentials::new(client_id.trim().to_owned(), client_secret.trim().to_owned());
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            consent::consent(this, cx, attempt, credentials, environment, remember).await;
        }));
    }

    /// Gives up the attempt in progress. The local port is free again at once.
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.attempt += 1;
        self.task = None;
        if let Some(abort) = self.abort.take() {
            abort.abort();
        }
        self.pending = None;
        self.state.cancel();
        cx.notify();
    }

    /// Picks the account at `index` in the list.
    pub fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        self.state.choose(index);
        cx.notify();
    }

    /// Connects the account picked now.
    pub fn connect_chosen(&mut self, cx: &mut Context<Self>) {
        let Some(chosen) = self.state.chosen().map(|a| a.id) else {
            return;
        };
        let Some(account) = self
            .pending
            .as_ref()
            .and_then(|p| {
                p.accounts
                    .iter()
                    .find(|a| a.ctid_trader_account_id == chosen)
            })
            .cloned()
        else {
            return;
        };
        self.authorize(account, cx);
    }

    fn authorize(&mut self, account: TraderAccount, cx: &mut Context<Self>) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        let label = account_label(&account);
        self.state.authorizing(label.clone());
        self.attempt += 1;
        let attempt = self.attempt;
        cx.notify();
        let Pending {
            credentials,
            environment,
            remember,
            client,
            tokens,
            ..
        } = pending;
        let account_id = account.ctid_trader_account_id;
        let access = tokens.access_token.expose_secret().to_owned();
        self.task = Some(cx.spawn(async move |this, cx| {
            let authorizing = client.clone();
            let authorized =
                runtime::spawn(
                    async move { authorizing.account(account_id).authorize(&access).await },
                )
                .await;
            let failure = match authorized {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(Failure::AuthorizeFailed(error.to_string())),
                Err(_) => Some(Failure::Other("the background runtime stopped".into())),
            };
            let _ = this.update(cx, |this, cx| {
                if this.attempt != attempt {
                    return;
                }
                if let Some(failure) = failure {
                    this.state.fail(failure);
                    cx.notify();
                    return;
                }
                this.finish(
                    credentials,
                    environment,
                    remember,
                    &account,
                    label,
                    tokens,
                    cx,
                );
            });
        }));
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        &mut self,
        credentials: ClientCredentials,
        environment: Environment,
        remember: bool,
        account: &TraderAccount,
        label: String,
        tokens: TokenSet,
        cx: &mut Context<Self>,
    ) {
        let (profile_id, store, not_saved): (_, Arc<dyn TokenStore>, _) = if remember {
            match self.save_profile(&credentials, environment, account, &tokens) {
                Ok(id) => {
                    let store = Arc::new(ConfigTokenStore::new(
                        self.config.openapi_token_storage(&id),
                    ));
                    (Some(id), store, None)
                }
                Err(error) => {
                    // The session still works; it just will not be there next time.
                    tracing::warn!(%error, "could not save the sign-in");
                    (
                        None,
                        Arc::new(MemoryTokenStore::default()),
                        Some(error.to_string()),
                    )
                }
            }
        } else {
            (None, Arc::new(MemoryTokenStore::default()), None)
        };
        let connection = self.connection(
            profile_id,
            label,
            environment,
            credentials,
            account.ctid_trader_account_id,
            tokens,
            store,
            not_saved,
        );
        self.state.connected();
        cx.emit(SignInEvent::Connected(Box::new(connection)));
        cx.notify();
    }

    /// Saves the connection so the next start can pick it up, replacing any earlier one (the app
    /// keeps a single Open API connection).
    fn save_profile(
        &mut self,
        credentials: &ClientCredentials,
        environment: Environment,
        account: &TraderAccount,
        tokens: &TokenSet,
    ) -> crate::app::storage::Result<ProfileId> {
        let stale: Vec<ProfileId> = self
            .config
            .profiles()
            .iter()
            .filter(|profile| environment_of(&profile.service).is_some())
            .map(|profile| profile.id.clone())
            .collect();
        for id in stale {
            self.config.remove_profile(&id)?;
        }
        let id = self
            .config
            .add_profile(account_label(account), service_tag(environment))?;
        self.config.set_openapi_profile(
            &id,
            credentials.client_id.clone(),
            CALLBACK_PORT,
            account.ctid_trader_account_id,
        )?;
        self.config
            .set_profile_secret(&id, CLIENT_SECRET, &credentials.client_secret)?;
        self.config
            .openapi_token_storage(&id)
            .save(&OpenApiTokens {
                access_token: tokens.access_token.clone(),
                refresh_token: tokens.refresh_token.clone(),
                expires_at: tokens.expires_at(),
            })?;
        self.config.set_active_profile(Some(id.clone()))?;
        Ok(id)
    }

    /// The user disconnected: the saved sign-in is removed and the form is empty again.
    pub fn forget(&mut self, profile: Option<&ProfileId>, cx: &mut Context<Self>) {
        if let Some(id) = profile
            && let Err(error) = self.config.remove_profile(id)
        {
            tracing::warn!(%error, "failed to remove the profile on disconnect");
        }
        self.state = SignInState::editing();
        cx.notify();
    }

    /// The session ended and a new sign-in is needed.
    pub fn session_ended(&mut self, cx: &mut Context<Self>) {
        self.state.expired();
        cx.notify();
    }

    /// The session could not be started.
    pub fn session_failed(&mut self, message: String, cx: &mut Context<Self>) {
        self.state.fail(Failure::Other(message));
        cx.notify();
    }

    /// Remembers the symbol shown, for the next start.
    pub fn remember_symbol(&mut self, name: &str) {
        if let Err(error) = self.config.set_last_symbol(Some(name.to_owned())) {
            tracing::warn!(%error, "could not remember the symbol");
        }
    }
}

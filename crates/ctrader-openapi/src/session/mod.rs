//! A session that stays up: it reconnects, signs in again, renews the tokens and restores the
//! subscriptions by itself.
//!
//! A [`Client`] is one connection and never reconnects (see its docs). A program that wants live
//! data for days needs the rest: notice the connection ended, wait a growing delay, connect,
//! identify the application, authorize the account, subscribe again, and meanwhile renew the access
//! token before its 30 days run out. [`Session`] does exactly that, in one background task, and
//! gives the program a single stream of [`SessionEvent`]s that survives every reconnect.
//!
//! ```no_run
//! use std::sync::Arc;
//! use ctrader_openapi::config::{ClientCredentials, ConnectionConfig, Environment};
//! use ctrader_openapi::session::{MemoryTokenStore, Session, SessionConfig, SessionEvent};
//! # async fn demo(tokens: ctrader_openapi::auth::TokenSet) -> ctrader_openapi::Result<()> {
//! let config = SessionConfig::new(
//!     ConnectionConfig::new(Environment::Demo),
//!     ClientCredentials::new("client-id", "client-secret"),
//!     48332955,
//! );
//! let session = Session::start(config, tokens, Arc::new(MemoryTokenStore::default()))?;
//! let mut events = session.events();
//! session.subscribe_spots(&[1]).await?;          // kept, and restored after every reconnect
//! while let Ok(event) = events.recv().await {
//!     match event {
//!         SessionEvent::Data(data) => println!("{data:?}"),
//!         SessionEvent::Failed(error) => { eprintln!("needs attention: {error}"); break; }
//!         _ => {}
//!     }
//! }
//! # Ok(()) }
//! ```
//!
//! # What it does
//!
//! - **Connects** and authenticates the application and the account. Until that succeeds it retries
//!   with a delay that doubles from [`Backoff::initial`] up to [`Backoff::max`] (with a little
//!   jitter, so many programs do not come back in step), or gives up after
//!   [`SessionConfig::max_reconnect_attempts`].
//! - **Restores the subscriptions** (prices, live bars, order book) that the program asked for,
//!   after every reconnect. They are kept in a registry: subscribing while the connection is down
//!   only records it.
//! - **Renews the tokens.** When the access token expires within [`SessionConfig::refresh_margin`]
//!   it is refreshed first; the new pair goes to the [`TokenStore`] before it is used, because the
//!   old refresh token stops working. A server notice that the tokens were invalidated forces a
//!   refresh and a reconnect.
//! - **Tells failures that will pass from those that will not.** A dropped connection, a timeout,
//!   maintenance or a rate limit are retried. A refused refresh token, an account that is not
//!   authorized, or a bad configuration end the session with [`SessionEvent::Failed`]: only the
//!   user can fix those (by signing in again).
//!
//! # What it does not do
//!
//! It does not replay requests that were in flight when the connection dropped (they fail with
//! [`OpenApiError::Closed`]; the caller may repeat them), and it does not keep data: events that
//! arrive while a reader is not listening are lost, as with any broadcast channel.
//!
//! # Why it stays flat
//!
//! [`Session`] keeps its own plain methods (`subscribe_spots`, `subscribe_live_bars`,
//! `subscribe_depth`) rather than adopting the [`MarketClient`](crate::market::MarketClient)-style
//! sub-client split the rest of the crate uses: its subscriptions have a different semantics
//! (recorded in a registry and replayed after every reconnect), and forcing the same split here
//! would not add anything real.

pub mod backoff;
pub mod token_store;

pub use backoff::Backoff;
pub use token_store::{MemoryTokenStore, TokenStore};

use std::collections::BTreeSet;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant, SystemTime};

use secrecy::ExposeSecret;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::auth::{OAuthClient, TokenSet};
use crate::config::{ClientCredentials, ConnectionConfig};
use crate::error::{ErrorKind, OpenApiError, Result};
use crate::event::Event;
use crate::market::Period;
use crate::transport::connection::Client;

/// The settings of a [`Session`].
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// The connection settings (demo or live, timeouts, limits).
    pub connection: ConnectionConfig,
    /// The application credentials.
    pub credentials: ClientCredentials,
    /// The trading account to authorize.
    pub account_id: i64,
    /// Refresh the access token when it expires within this long. The default is one day.
    pub refresh_margin: Duration,
    /// The wait between attempts to connect.
    pub backoff: Backoff,
    /// Give up after this many failed attempts in a row. `None` (the default) never gives up on
    /// failures that may pass.
    pub max_reconnect_attempts: Option<u32>,
    /// Use another token endpoint, for a test server.
    pub token_url: Option<String>,
}

impl SessionConfig {
    /// Settings with the defaults for everything but the three things a session cannot guess.
    #[must_use]
    pub fn new(
        connection: ConnectionConfig,
        credentials: ClientCredentials,
        account_id: i64,
    ) -> Self {
        Self {
            connection,
            credentials,
            account_id,
            refresh_margin: Duration::from_secs(24 * 3600),
            backoff: Backoff::default(),
            max_reconnect_attempts: None,
            token_url: None,
        }
    }
}

/// Where a session stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// Connecting (the attempt number, starting at 1).
    Connecting {
        /// Which attempt this is in a row.
        attempt: u32,
    },
    /// Connected, signed in, subscriptions restored.
    Ready,
    /// Waiting before the next attempt.
    Waiting {
        /// The attempt that failed.
        attempt: u32,
    },
    /// Ended on purpose ([`Session::stop`]).
    Stopped,
    /// Ended by a failure that will not pass. The text says why.
    Failed(String),
}

/// What a session reports.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SessionEvent {
    /// Connected and signed in, subscriptions restored. Sent again after each reconnect.
    Ready,
    /// Something the server sent: a price, an order book change, an account notice. The
    /// connection's own end ([`Event::Disconnected`]) is reported as `Reconnecting` instead.
    Data(Event),
    /// The tokens were renewed and saved.
    TokensRefreshed,
    /// The connection is down; the session will try again after `retry_in`.
    Reconnecting {
        /// The attempt that failed, starting at 1.
        attempt: u32,
        /// How long until the next attempt.
        retry_in: Duration,
        /// Why the connection is down.
        reason: String,
    },
    /// A subscription could not be restored. The session carries on without it.
    SubscriptionFailed {
        /// What was being subscribed to.
        what: String,
        /// Why the server refused it.
        error: OpenApiError,
    },
    /// The session ended by a failure that will not pass; the user has to act (sign in again).
    Failed(OpenApiError),
    /// The session was stopped. This is always the last event.
    Stopped,
}

/// What the program wants to stay subscribed to.
#[derive(Debug, Default, Clone)]
struct Registry {
    spots: BTreeSet<i64>,
    live_bars: BTreeSet<(i64, Period)>,
    depth: BTreeSet<i64>,
}

struct Shared {
    account_id: i64,
    events: broadcast::Sender<SessionEvent>,
    state: watch::Sender<SessionState>,
    client: RwLock<Option<Client>>,
    registry: Mutex<Registry>,
    tokens: Mutex<TokenSet>,
    stop: watch::Sender<bool>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Shared {
    fn emit(&self, event: SessionEvent) {
        // Nobody listening is fine.
        let _ = self.events.send(event);
    }

    fn set_state(&self, state: SessionState) {
        self.state.send_replace(state);
    }

    fn client(&self) -> Option<Client> {
        self.client
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn set_client(&self, client: Option<Client>) {
        *self
            .client
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = client;
    }
}

/// A session that stays up. See the [module docs](self).
#[derive(Clone)]
pub struct Session {
    shared: Arc<Shared>,
    task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("account_id", &self.shared.account_id)
            .field("state", &*self.shared.state.borrow())
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Starts the session in the background with the tokens the user signed in with. The tokens
    /// are saved to `store` when they are refreshed.
    ///
    /// # Errors
    ///
    /// [`OpenApiError::Config`] for unusable settings.
    pub fn start(
        config: SessionConfig,
        tokens: TokenSet,
        store: Arc<dyn TokenStore>,
    ) -> Result<Self> {
        config.connection.validate()?;
        let mut oauth = OAuthClient::new(config.credentials.clone())?;
        if let Some(url) = &config.token_url {
            oauth = oauth.with_token_url(url.clone());
        }
        let (events, _) = broadcast::channel(config.connection.event_capacity);
        let (state, _) = watch::channel(SessionState::Connecting { attempt: 1 });
        let (stop, _) = watch::channel(false);
        let shared = Arc::new(Shared {
            account_id: config.account_id,
            events,
            state,
            client: RwLock::new(None),
            registry: Mutex::new(Registry::default()),
            tokens: Mutex::new(tokens.clone()),
            stop,
        });
        let task = tokio::spawn(supervise(config, Arc::clone(&shared), oauth, store, tokens));
        Ok(Self {
            shared,
            task: Arc::new(Mutex::new(Some(task))),
        })
    }

    /// What the session reports: data, reconnections, failures. Each call gives an independent
    /// reader that sees the events from now on.
    #[must_use]
    pub fn events(&self) -> broadcast::Receiver<SessionEvent> {
        self.shared.events.subscribe()
    }

    /// Follows the state of the session.
    #[must_use]
    pub fn state(&self) -> watch::Receiver<SessionState> {
        self.shared.state.subscribe()
    }

    /// The connection in use right now, for the calls the session does not wrap (history, account
    /// data). `None` while it is down. It may end at any moment: a failed call is worth repeating
    /// with the next client.
    #[must_use]
    pub fn client(&self) -> Option<Client> {
        self.shared.client()
    }

    /// The account the session works for.
    #[must_use]
    pub fn account_id(&self) -> i64 {
        self.shared.account_id
    }

    /// The tokens in use (the latest pair, after refreshes).
    #[must_use]
    pub fn tokens(&self) -> TokenSet {
        lock(&self.shared.tokens).clone()
    }

    /// Waits until the session is ready and returns its connection.
    ///
    /// # Errors
    ///
    /// [`OpenApiError::Timeout`] when it is not ready in time, and [`OpenApiError::Closed`] when the
    /// session has stopped or failed.
    pub async fn wait_ready(&self, timeout: Duration) -> Result<Client> {
        let mut state = self.state();
        let waited = tokio::time::timeout(
            timeout,
            state.wait_for(|s| {
                matches!(
                    s,
                    SessionState::Ready | SessionState::Stopped | SessionState::Failed(_)
                )
            }),
        )
        .await
        .map_err(|_| OpenApiError::Timeout {
            operation: "the session to be ready",
        })?
        .map_err(|_| OpenApiError::Closed)?
        .clone();
        match waited {
            SessionState::Ready => self.client().ok_or(OpenApiError::Closed),
            _ => Err(OpenApiError::Closed),
        }
    }

    /// Ends the session: closes the connection and stops the background task. A last
    /// [`SessionEvent::Stopped`] is sent. Calling it again does nothing.
    pub async fn stop(&self) {
        self.shared.stop.send_replace(true);
        let task = lock(&self.task).take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }

    // ---- subscriptions ----

    /// Follows the prices of some symbols, now and after every reconnect. While the connection is
    /// down the request is only recorded and applied when it is back.
    ///
    /// # Errors
    ///
    /// A server error when the connection is up and the server refuses the subscription (an
    /// unknown symbol); the symbols are then not kept.
    pub async fn subscribe_spots(&self, symbol_ids: &[i64]) -> Result<()> {
        let fresh: Vec<i64> = {
            let mut registry = lock(&self.shared.registry);
            symbol_ids
                .iter()
                .copied()
                .filter(|id| registry.spots.insert(*id))
                .collect()
        };
        if fresh.is_empty() {
            return Ok(());
        }
        let Some(client) = self.shared.client() else {
            return Ok(());
        };
        let result = client
            .account(self.shared.account_id)
            .market()
            .subscribe_spots(&fresh)
            .await;
        self.settle(result, |registry| {
            for id in &fresh {
                registry.spots.remove(id);
            }
        })
    }

    /// Stops following the prices of some symbols and forgets them, so a reconnect does not bring
    /// them back.
    ///
    /// # Errors
    ///
    /// A server error when the connection is up and the server refuses.
    pub async fn unsubscribe_spots(&self, symbol_ids: &[i64]) -> Result<()> {
        let gone: Vec<i64> = {
            let mut registry = lock(&self.shared.registry);
            symbol_ids
                .iter()
                .copied()
                .filter(|id| registry.spots.remove(id))
                .collect()
        };
        match (gone.is_empty(), self.shared.client()) {
            (false, Some(client)) => {
                client
                    .account(self.shared.account_id)
                    .market()
                    .unsubscribe_spots(&gone)
                    .await
            }
            _ => Ok(()),
        }
    }

    /// Follows the live bar of a symbol at a period (needs the price subscription of the symbol),
    /// now and after every reconnect.
    ///
    /// # Errors
    ///
    /// A server error when the connection is up and the server refuses.
    pub async fn subscribe_live_bars(&self, symbol_id: i64, period: Period) -> Result<()> {
        if !lock(&self.shared.registry)
            .live_bars
            .insert((symbol_id, period))
        {
            return Ok(());
        }
        let Some(client) = self.shared.client() else {
            return Ok(());
        };
        let result = client
            .account(self.shared.account_id)
            .market()
            .subscribe_live_bars(symbol_id, period)
            .await;
        self.settle(result, |registry| {
            registry.live_bars.remove(&(symbol_id, period));
        })
    }

    /// Stops following the live bar of a symbol at a period and forgets it.
    ///
    /// # Errors
    ///
    /// A server error when the connection is up and the server refuses.
    pub async fn unsubscribe_live_bars(&self, symbol_id: i64, period: Period) -> Result<()> {
        let existed = lock(&self.shared.registry)
            .live_bars
            .remove(&(symbol_id, period));
        match (existed, self.shared.client()) {
            (true, Some(client)) => {
                client
                    .account(self.shared.account_id)
                    .market()
                    .unsubscribe_live_bars(symbol_id, period)
                    .await
            }
            _ => Ok(()),
        }
    }

    /// Follows the order book of some symbols, now and after every reconnect.
    ///
    /// # Errors
    ///
    /// A server error when the connection is up and the server refuses.
    pub async fn subscribe_depth(&self, symbol_ids: &[i64]) -> Result<()> {
        let fresh: Vec<i64> = {
            let mut registry = lock(&self.shared.registry);
            symbol_ids
                .iter()
                .copied()
                .filter(|id| registry.depth.insert(*id))
                .collect()
        };
        if fresh.is_empty() {
            return Ok(());
        }
        let Some(client) = self.shared.client() else {
            return Ok(());
        };
        let result = client
            .account(self.shared.account_id)
            .market()
            .subscribe_depth(&fresh)
            .await;
        self.settle(result, |registry| {
            for id in &fresh {
                registry.depth.remove(id);
            }
        })
    }

    /// Stops following the order book of some symbols and forgets them.
    ///
    /// # Errors
    ///
    /// A server error when the connection is up and the server refuses.
    pub async fn unsubscribe_depth(&self, symbol_ids: &[i64]) -> Result<()> {
        let gone: Vec<i64> = {
            let mut registry = lock(&self.shared.registry);
            symbol_ids
                .iter()
                .copied()
                .filter(|id| registry.depth.remove(id))
                .collect()
        };
        match (gone.is_empty(), self.shared.client()) {
            (false, Some(client)) => {
                client
                    .account(self.shared.account_id)
                    .market()
                    .unsubscribe_depth(&gone)
                    .await
            }
            _ => Ok(()),
        }
    }

    /// Keeps a subscription that the server refused out of the registry (it would be refused again
    /// after every reconnect), and passes the error on. A subscription that only failed because the
    /// connection dropped stays: the reconnect restores it.
    fn settle(&self, result: Result<()>, undo: impl FnOnce(&mut Registry)) -> Result<()> {
        match result {
            // Already following it is what was asked for (a reconnect may have restored it a
            // moment before this call went out).
            Err(error) if error.code() == Some("ALREADY_SUBSCRIBED") => Ok(()),
            Err(error) if !error_is_transient(&error) => {
                undo(&mut lock(&self.shared.registry));
                Err(error)
            }
            other => other,
        }
    }
}

/// Whether an error is about the connection or the load, not about the request itself.
fn error_is_transient(error: &OpenApiError) -> bool {
    matches!(
        error.kind(),
        ErrorKind::Transport
            | ErrorKind::Timeout
            | ErrorKind::Closed
            | ErrorKind::RateLimited
            | ErrorKind::Maintenance
    )
}

/// What to do after a connection attempt or a connection ended.
#[derive(Debug, PartialEq, Eq)]
enum Next {
    /// Try again after a wait.
    Retry,
    /// Refresh the tokens, then try again at once.
    RefreshThenRetry,
    /// End the session: only the user can fix this.
    Fail,
}

/// Decides what a failure means for the session.
fn classify(error: &OpenApiError) -> Next {
    match error.kind() {
        ErrorKind::TokenInvalid => Next::RefreshThenRetry,
        ErrorKind::NotAuthorized | ErrorKind::Config | ErrorKind::Rejected => Next::Fail,
        ErrorKind::Transport
        | ErrorKind::Timeout
        | ErrorKind::Closed
        | ErrorKind::RateLimited
        | ErrorKind::Maintenance
        | ErrorKind::Protocol => Next::Retry,
    }
}

/// A cheap source of jitter: the sub-second part of the clock.
fn noise() -> u32 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos())
}

/// Runs `future` unless the session is asked to stop first, which gives `None` and drops the future.
/// Every long step of the supervisor goes through it, so a stop is never held up by a connection
/// attempt or a token request that is still waiting for its timeout.
async fn or_stop<T>(
    stop: &mut watch::Receiver<bool>,
    future: impl Future<Output = T>,
) -> Option<T> {
    if *stop.borrow() {
        return None;
    }
    tokio::select! {
        value = future => Some(value),
        _ = stop.changed() => None,
    }
}

/// Waits for `duration`, or returns `true` early when the session is asked to stop.
async fn sleep_or_stop(duration: Duration, stop: &mut watch::Receiver<bool>) -> bool {
    if *stop.borrow() {
        return true;
    }
    tokio::select! {
        () = tokio::time::sleep(duration) => false,
        _ = stop.changed() => true,
    }
}

/// The supervisor: connect, serve, and start over until told to stop or something unfixable happens.
async fn supervise(
    config: SessionConfig,
    shared: Arc<Shared>,
    oauth: OAuthClient,
    store: Arc<dyn TokenStore>,
    mut tokens: TokenSet,
) {
    let mut stop = shared.stop.subscribe();
    let mut attempt: u32 = 0;
    let mut force_refresh = false;
    // A refresh forced by an invalid token is tried once; if the next connection still finds the
    // token invalid the refresh did not help and the session ends.
    let mut refreshed_for_invalid = false;

    let outcome: Option<OpenApiError> = 'session: loop {
        if *stop.borrow() {
            break None;
        }
        attempt += 1;
        shared.set_state(SessionState::Connecting { attempt });

        // 1. Tokens: refresh when they are about to expire, or when the server said they are bad.
        if force_refresh || tokens.expires_within(SystemTime::now(), config.refresh_margin) {
            let Some(refreshed) =
                or_stop(&mut stop, refresh(&oauth, &store, &shared, &tokens)).await
            else {
                break None;
            };
            match refreshed {
                Ok(fresh) => {
                    tokens = fresh;
                    force_refresh = false;
                }
                Err(error) => match classify_refresh_error(&error) {
                    Next::Fail => break 'session Some(error),
                    _ => {
                        // The endpoint could not be reached: wait and try again.
                        if let Some(end) =
                            wait_after_failure(&config, &shared, &mut stop, attempt, &error).await
                        {
                            break 'session end;
                        }
                        continue;
                    }
                },
            }
        }

        // 2. Connect and sign in.
        let Some(connected) = or_stop(&mut stop, connect(&config, &tokens)).await else {
            break None;
        };
        let client = match connected {
            Ok(client) => client,
            Err(error) => match classify(&error) {
                Next::Fail => break 'session Some(error),
                Next::RefreshThenRetry => {
                    if refreshed_for_invalid {
                        break 'session Some(error);
                    }
                    refreshed_for_invalid = true;
                    force_refresh = true;
                    attempt = attempt.saturating_sub(1);
                    continue;
                }
                Next::Retry => {
                    if let Some(end) =
                        wait_after_failure(&config, &shared, &mut stop, attempt, &error).await
                    {
                        break 'session end;
                    }
                    continue;
                }
            },
        };

        // 3. Serve: restore the subscriptions, announce, and forward events until it ends. The
        //    attempt counter is set below, by how the connection ends.
        refreshed_for_invalid = false;
        let mut events = client.events();
        shared.set_client(Some(client.clone()));
        if or_stop(&mut stop, restore_subscriptions(&client, &shared))
            .await
            .is_none()
        {
            client.close().await;
            break None;
        }
        shared.set_state(SessionState::Ready);
        shared.emit(SessionEvent::Ready);
        info!(account = shared.account_id, "the Open API session is ready");

        let ended = serve(&client, &mut events, &shared, &mut stop).await;
        shared.set_client(None);
        match ended {
            Served::Stopped => {
                client.close().await;
                break None;
            }
            Served::TokensInvalid => {
                client.close().await;
                force_refresh = true;
                // Not a failure of the connection: reconnect at once with fresh tokens.
                attempt = 0;
            }
            Served::Disconnected(reason) => {
                attempt = 1;
                let error = OpenApiError::Transport(reason.clone());
                if let Some(end) =
                    wait_after_failure(&config, &shared, &mut stop, attempt, &error).await
                {
                    break 'session end;
                }
            }
        }
    };

    shared.set_client(None);
    match outcome {
        Some(error) => {
            warn!(%error, "the Open API session ended");
            shared.set_state(SessionState::Failed(error.to_string()));
            shared.emit(SessionEvent::Failed(error));
        }
        None => {
            shared.set_state(SessionState::Stopped);
            shared.emit(SessionEvent::Stopped);
        }
    }
}

/// How the served connection ended.
enum Served {
    /// The session was asked to stop.
    Stopped,
    /// The server said the tokens are no longer valid.
    TokensInvalid,
    /// The connection ended.
    Disconnected(String),
}

/// Forwards the connection's events until it ends, the tokens are invalidated, or a stop is asked.
async fn serve(
    client: &Client,
    events: &mut broadcast::Receiver<Event>,
    shared: &Shared,
    stop: &mut watch::Receiver<bool>,
) -> Served {
    loop {
        tokio::select! {
            _ = stop.changed() => return Served::Stopped,
            received = events.recv() => match received {
                Ok(Event::Disconnected(reason)) => return Served::Disconnected(format!("{reason:?}")),
                Ok(Event::TokensInvalidated(notice)) => {
                    let ours = notice.ctid_trader_account_ids.is_empty()
                        || notice.ctid_trader_account_ids.contains(&shared.account_id);
                    shared.emit(SessionEvent::Data(Event::TokensInvalidated(notice)));
                    if ours {
                        return Served::TokensInvalid;
                    }
                }
                Ok(event) => shared.emit(SessionEvent::Data(event)),
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    warn!(missed, "the session fell behind and lost events");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Served::Disconnected("the event channel closed".to_owned());
                }
            },
        }
        if client.is_closed() && events.is_empty() {
            return Served::Disconnected("the connection is closed".to_owned());
        }
    }
}

/// Connects, identifies the application and authorizes the account.
async fn connect(config: &SessionConfig, tokens: &TokenSet) -> Result<Client> {
    let client = Client::connect(&config.connection).await?;
    let signed_in = async {
        client.authenticate_application(&config.credentials).await?;
        client
            .account(config.account_id)
            .authorize(tokens.access_token.expose_secret())
            .await
    }
    .await;
    match signed_in {
        Ok(()) => Ok(client),
        Err(error) => {
            client.close().await;
            Err(error)
        }
    }
}

/// Subscribes again to everything in the registry. A refusal is reported and skipped: one bad
/// symbol must not stop the others, or the session.
async fn restore_subscriptions(client: &Client, shared: &Shared) {
    let registry = lock(&shared.registry).clone();
    let market = client.account(shared.account_id).market();
    let report = |what: String, error: OpenApiError| {
        // Already subscribed is what we wanted.
        if error.code() != Some("ALREADY_SUBSCRIBED") {
            shared.emit(SessionEvent::SubscriptionFailed { what, error });
        }
    };
    if !registry.spots.is_empty() {
        let ids: Vec<i64> = registry.spots.iter().copied().collect();
        if let Err(error) = market.subscribe_spots(&ids).await {
            report(format!("prices of {ids:?}"), error);
        }
    }
    for (symbol, period) in &registry.live_bars {
        if let Err(error) = market.subscribe_live_bars(*symbol, *period).await {
            report(format!("live {} bars of {symbol}", period.label()), error);
        }
    }
    if !registry.depth.is_empty() {
        let ids: Vec<i64> = registry.depth.iter().copied().collect();
        if let Err(error) = market.subscribe_depth(&ids).await {
            report(format!("the order book of {ids:?}"), error);
        }
    }
}

/// Refreshes the tokens and saves the new pair before returning it.
async fn refresh(
    oauth: &OAuthClient,
    store: &Arc<dyn TokenStore>,
    shared: &Shared,
    tokens: &TokenSet,
) -> Result<TokenSet> {
    let fresh = oauth.refresh(tokens.refresh_token.expose_secret()).await?;
    // The old refresh token is dead now: the new pair must be safe before anything else.
    store.save(&fresh).await.map_err(|error| {
        OpenApiError::Auth(format!("the new tokens could not be saved: {error}"))
    })?;
    *lock(&shared.tokens) = fresh.clone();
    shared.emit(SessionEvent::TokensRefreshed);
    debug!("the Open API tokens were refreshed");
    Ok(fresh)
}

/// A refusal by the token endpoint ends the session; failing to reach it is retried.
fn classify_refresh_error(error: &OpenApiError) -> Next {
    match error {
        OpenApiError::Transport(_) | OpenApiError::Timeout { .. } => Next::Retry,
        _ => Next::Fail,
    }
}

/// Announces the failed attempt and waits before the next one. Returns `Some(end)` when the session
/// must end instead: `Some(None)` for a stop, `Some(Some(error))` for giving up.
async fn wait_after_failure(
    config: &SessionConfig,
    shared: &Shared,
    stop: &mut watch::Receiver<bool>,
    attempt: u32,
    error: &OpenApiError,
) -> Option<Option<OpenApiError>> {
    if config
        .max_reconnect_attempts
        .is_some_and(|max| attempt > max)
    {
        return Some(Some(OpenApiError::Transport(format!(
            "gave up after {} attempts: {error}",
            attempt.saturating_sub(1).max(1)
        ))));
    }
    // The server's own advice (rate limit, maintenance) is a floor for the wait.
    let mut wait = config.backoff.jittered(attempt, noise());
    if let Some(advice) = error.retry_after() {
        wait = wait.max(advice.min(config.backoff.max));
    }
    shared.set_state(SessionState::Waiting { attempt });
    shared.emit(SessionEvent::Reconnecting {
        attempt,
        retry_in: wait,
        reason: error.to_string(),
    });
    debug!(attempt, ?wait, %error, "the Open API connection is down, waiting");
    let started = Instant::now();
    if sleep_or_stop(wait, stop).await {
        debug!(waited = ?started.elapsed(), "stopped while waiting");
        return Some(None);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_are_sorted_into_retry_refresh_or_end() {
        let server = |code: &str| OpenApiError::server(code, None, None, None);
        assert_eq!(classify(&OpenApiError::Transport("x".into())), Next::Retry);
        assert_eq!(
            classify(&OpenApiError::Timeout { operation: "x" }),
            Next::Retry
        );
        assert_eq!(classify(&OpenApiError::Closed), Next::Retry);
        assert_eq!(classify(&server("REQUEST_FREQUENCY_EXCEEDED")), Next::Retry);
        assert_eq!(
            classify(&server("SERVER_IS_UNDER_MAINTENANCE")),
            Next::Retry
        );
        assert_eq!(
            classify(&server("CH_ACCESS_TOKEN_INVALID")),
            Next::RefreshThenRetry
        );
        assert_eq!(
            classify(&server("OA_AUTH_TOKEN_EXPIRED")),
            Next::RefreshThenRetry
        );
        assert_eq!(classify(&server("ACCOUNT_NOT_AUTHORIZED")), Next::Fail);
        assert_eq!(classify(&server("CH_CLIENT_AUTH_FAILURE")), Next::Fail);
        assert_eq!(classify(&OpenApiError::Config("x".into())), Next::Fail);
    }

    #[test]
    fn a_refusal_of_the_token_endpoint_ends_the_session_but_an_unreachable_one_does_not() {
        assert_eq!(
            classify_refresh_error(&OpenApiError::Auth("ACCESS_DENIED".into())),
            Next::Fail
        );
        assert_eq!(
            classify_refresh_error(&OpenApiError::Transport("unreachable".into())),
            Next::Retry
        );
    }

    #[test]
    fn transient_errors_are_the_ones_a_reconnect_can_cure() {
        assert!(error_is_transient(&OpenApiError::Closed));
        assert!(error_is_transient(&OpenApiError::Timeout {
            operation: "x"
        }));
        assert!(!error_is_transient(&OpenApiError::server(
            "SYMBOL_NOT_FOUND",
            None,
            None,
            None
        )));
    }

    #[tokio::test(start_paused = true)]
    async fn a_stop_cuts_a_wait_short() {
        let (tx, mut rx) = watch::channel(false);
        let waiting =
            tokio::spawn(async move { sleep_or_stop(Duration::from_secs(3600), &mut rx).await });
        tokio::time::sleep(Duration::from_secs(1)).await;
        tx.send_replace(true);
        assert!(waiting.await.unwrap(), "stopped before the hour was up");
    }

    #[tokio::test(start_paused = true)]
    async fn a_wait_that_is_not_interrupted_runs_its_course() {
        let (_tx, mut rx) = watch::channel(false);
        assert!(!sleep_or_stop(Duration::from_secs(5), &mut rx).await);
    }
}

#[cfg(test)]
mod stop_tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_future_that_finishes_gives_its_value() {
        let (_tx, mut rx) = watch::channel(false);
        assert_eq!(or_stop(&mut rx, async { 7 }).await, Some(7));
    }

    #[tokio::test(start_paused = true)]
    async fn a_stop_drops_a_future_that_would_take_an_hour() {
        let (tx, mut rx) = watch::channel(false);
        let running = tokio::spawn(async move {
            or_stop(&mut rx, tokio::time::sleep(Duration::from_secs(3600))).await
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
        tx.send_replace(true);
        assert_eq!(running.await.unwrap(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn a_session_already_asked_to_stop_does_not_start_anything() {
        let (tx, mut rx) = watch::channel(false);
        tx.send_replace(true);
        let mut ran = false;
        let result = or_stop(&mut rx, async {
            ran = true;
        })
        .await;
        assert_eq!(result, None);
        assert!(!ran, "the future was never polled");
    }
}

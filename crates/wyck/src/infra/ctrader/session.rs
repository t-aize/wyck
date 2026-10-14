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
//! use wyck::infra::ctrader::config::{ClientCredentials, ConnectionConfig, Environment};
//! use wyck::infra::ctrader::session::{MemoryTokenStore, Session, SessionConfig, SessionEvent};
//! # async fn demo(tokens: wyck::infra::ctrader::auth::TokenSet) -> wyck::infra::ctrader::Result<()> {
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
//! - **Reacts to the server's own disconnect notices**, not just a dropped socket: a
//!   `ProtoOAAccountDisconnectEvent` for this account re-sends the account auth on the same
//!   connection (the documented recovery: the account's session was torn down server-side, but
//!   the connection itself is still good), and a `ProtoOAClientDisconnectEvent` (the server ending
//!   every session on the connection) triggers a full reconnect, both without waiting for the
//!   socket to actually close.
//! - **Tells failures that will pass from those that will not.** A dropped connection, a timeout,
//!   maintenance or a rate limit are retried. A refused refresh token, an account that is not
//!   authorized, or a bad configuration end the session with [`SessionEvent::Failed`]: only the
//!   user can fix those (by signing in again).
//!
//! # What it does not do
//!
//! It does not replay requests that were in flight when the connection dropped (they fail with
//! [`Error::Closed`]; the caller may repeat them), and it does not keep data: events that
//! arrive while a reader is not listening are lost, as with any broadcast channel.
//!
//! # Why it stays flat
//!
//! [`Session`] keeps its own plain methods (`subscribe_spots`, `subscribe_live_bars`,
//! `subscribe_depth`) rather than adopting the [`MarketClient`](crate::infra::ctrader::market::MarketClient)-style
//! sub-client split the rest of the crate uses: its subscriptions have a different semantics
//! (recorded in a registry and replayed after every reconnect), and forcing the same split here
//! would not add anything real.

mod backoff;
pub mod supervisor;
mod token_store;

use crate::infra::ctrader::auth::{OAuthClient, TokenSet};
use crate::infra::ctrader::config::{ClientCredentials, ConnectionConfig};
use crate::infra::ctrader::error::{Error, Result};
use crate::infra::ctrader::event::Event;
use crate::infra::ctrader::transport::connection::Client;
pub use backoff::Backoff;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::Duration;
pub use token_store::{MemoryTokenStore, TokenStore};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tracing::debug;
use wyck_market::Period;

/// The settings of a [`Session`].
#[derive(Debug, Clone)]
#[non_exhaustive]
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
    /// The shortest wait between two renewals of the tokens on a live connection, and a fifth of
    /// the wait after one that failed. The default is one minute.
    pub min_refresh_wait: Duration,
    /// A connection that lived this long counts as good: the next drop starts the backoff over.
    /// A shorter one keeps the wait growing, so a server that accepts and drops at once is not
    /// hammered. The default is 30 seconds.
    pub stable_after: Duration,
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
            min_refresh_wait: Duration::from_secs(60),
            stable_after: Duration::from_secs(30),
            max_reconnect_attempts: None,
            token_url: None,
        }
    }
}

/// Where a session stands.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    /// The tokens were renewed but could not be saved. The session carries on with the new pair in
    /// memory; after a restart the user has to sign in again.
    TokensNotSaved(String),
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
        error: Error,
    },
    /// The session ended by a failure that will not pass; the user has to act (sign in again).
    Failed(Error),
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
    /// How many live [`Session`] values share this supervisor. See its [`Drop`] impl.
    handles: AtomicUsize,
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
///
/// Cheap to clone: every clone shares the same background task. Calling [`Session::stop`] is
/// still the right way to end a session on purpose (it also waits for the background task to
/// finish), but dropping every clone without it does not leak the task either: the last clone
/// going away asks the task to stop as a fallback (see the `Drop` impl below), the same signal
/// `stop()` sends, just without waiting for it to finish.
pub struct Session {
    shared: Arc<Shared>,
    task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Clone for Session {
    fn clone(&self) -> Self {
        // `Relaxed` is enough: this only counts clones, it establishes no ordering with anything
        // else the clone goes on to do.
        self.shared.handles.fetch_add(1, Ordering::Relaxed);
        Self {
            shared: Arc::clone(&self.shared),
            task: Arc::clone(&self.task),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // `AcqRel`: pairs with every other clone's decrement, so the one that observes the count
        // drop to zero has truly seen every earlier clone finish dropping.
        if self.shared.handles.fetch_sub(1, Ordering::AcqRel) == 1 {
            // The last external clone is gone. Ask the supervisor to stop, the same signal
            // `stop()` sends; the task notices on its own and winds down (closing its connection
            // in turn), so it does not keep reconnecting forever with nobody left to hear from it.
            self.shared.stop.send_replace(true);
        }
    }
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
    /// [`Error::Config`] for unusable settings.
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
            handles: AtomicUsize::new(1),
        });
        let task = tokio::spawn(supervisor::supervise(
            config,
            Arc::clone(&shared),
            oauth,
            store,
            tokens,
        ));
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
    /// [`Error::Timeout`] when it is not ready in time, and [`Error::Closed`] when the
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
        .map_err(|_| Error::Timeout {
            operation: "the session to be ready",
        })?
        .map_err(|_| Error::Closed)?
        .clone();
        match waited {
            SessionState::Ready => self.client().ok_or(Error::Closed),
            _ => Err(Error::Closed),
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
            debug!(symbols = ?fresh, "spot subscription recorded, queued for when the connection is up");
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
            debug!(
                symbol_id,
                ?period,
                "live bar subscription recorded, queued for when the connection is up"
            );
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
            debug!(symbols = ?fresh, "depth subscription recorded, queued for when the connection is up");
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
            Err(error) if !supervisor::error_is_transient(&error) => {
                undo(&mut lock(&self.shared.registry));
                Err(error)
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod stop_tests {}

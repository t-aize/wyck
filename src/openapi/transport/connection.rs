//! The connection to the Open API: one WebSocket, kept alive, with request matching.
//!
//! A [`Client`] owns one WebSocket to `demo` or `live`. A background task reads it, keeps it alive
//! with heartbeats, matches answers to requests, and broadcasts everything else as [`Event`]s.
//! The client itself is cheap to clone: every clone talks to the same connection.
//!
//! [`Client`] only covers the connection itself: opening it, identifying the application, listing
//! and authorizing accounts, refreshing tokens, the proxy version and the cTrader ID profile.
//! Everything that needs an authorized account is reached through [`Client::account`], which
//! returns an [`crate::openapi::AccountClient`] with a named sub-client per domain (market data, account
//! data, trading, margin).
//!
//! # A session, step by step
//!
//! 1. [`Client::connect`] opens the socket (or [`ClientBuilder`] for connecting and identifying the
//!    application in one step).
//! 2. [`Client::authenticate_application`] identifies the application (client id and secret).
//! 3. [`Client::accounts`] lists the trading accounts an access token covers, and
//!    [`Client::account`] plus [`crate::openapi::AccountClient::authorize`] authorizes one on this
//!    connection.
//! 4. Then the sub-clients: [`crate::openapi::AccountClient::market`], [`crate::openapi::AccountClient::account_data`],
//!    [`crate::openapi::AccountClient::trading`], [`crate::openapi::AccountClient::margin`].
//!
//! # How requests work
//!
//! Every request gets a unique `clientMsgId`; the answer carries it back, so many requests can be
//! in flight at once and answers may come in any order. A request waits for its turn at the
//! [rate limiter](crate::openapi::transport::rate_limit::RateLimiter) (50 per second, 5 for history), then
//! for its answer up to the configured timeout. A server error becomes an
//! [`OpenApiError::Server`].
//!
//! # When the connection ends
//!
//! Every waiting request fails with [`OpenApiError::Closed`], an [`Event::Disconnected`] is
//! broadcast, and [`Client::state`] turns to [`ConnectionState::Closed`]. **The client does not
//! reconnect by itself**: after a reconnect the application and the accounts must be authorized
//! again and the subscriptions renewed, which is the caller's business (see [`crate::openapi::session`] for
//! a client that does this by itself). Build a new [`Client`] and repeat the steps.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use secrecy::ExposeSecret;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, trace, warn};

use crate::openapi::config::{ClientCredentials, ConnectionConfig, Environment};
use crate::openapi::error::{ErrorKind, OpenApiError, Result};
use crate::openapi::event::{DisconnectReason, Event, error_of, event_from};
use crate::openapi::transport::messages::{
    AccountAuthReq, AccountAuthRes, AccountsRes, ApplicationAuthReq, CtidProfile, CtidProfileReq,
    CtidProfileRes, GetAccountsByAccessTokenReq, RefreshTokenReq, RefreshTokenRes, VersionReq,
    VersionRes,
};
use crate::openapi::transport::rate_limit::RateLimiter;
use crate::openapi::transport::wire::{Envelope, payload};

/// How many outgoing messages may queue before a sender waits.
const OUTGOING_QUEUE: usize = 256;

/// Which limit a request counts against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RateClass {
    /// Everything but history: 50 per second.
    Standard,
    /// Bars and ticks: 5 per second.
    Historical,
}

/// Whether the connection is usable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    /// The socket is open.
    Connected,
    /// The socket is closed, for this reason. It never opens again: build a new client.
    Closed(DisconnectReason),
}

/// A message on its way to the socket.
enum Outgoing {
    Text(String),
}

/// What every clone of a client shares.
struct Shared {
    outgoing: mpsc::Sender<Outgoing>,
    pending: Mutex<HashMap<String, oneshot::Sender<Result<Envelope>>>>,
    events: broadcast::Sender<Event>,
    state: watch::Sender<ConnectionState>,
    shutdown: watch::Sender<bool>,
    closed: AtomicBool,
    standard: RateLimiter,
    historical: RateLimiter,
    request_timeout: Duration,
    rate_limit_retries: u32,
    max_retry_wait: Duration,
    /// How many live [`Client`] values point at this connection, `run` itself excluded (it holds
    /// an `Arc<Shared>` of its own, which would otherwise hide the last external clone going away
    /// from `outgoing`'s sender count). See the [`Drop`] impl below.
    handles: AtomicUsize,
}

/// Removes a waiter even when its request future is cancelled while sending or waiting.
struct PendingRequest<'a> {
    shared: &'a Shared,
    id: String,
}

impl Drop for PendingRequest<'_> {
    fn drop(&mut self) {
        self.shared.pending().remove(&self.id);
    }
}

impl Shared {
    fn pending(&self) -> MutexGuard<'_, HashMap<String, oneshot::Sender<Result<Envelope>>>> {
        // A poisoned lock only means another task panicked while holding it; the map is still
        // a valid map of waiters, and failing every later request would help nobody.
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Hands a message from the server to whoever waits for it, or broadcasts it as an event.
    fn route(&self, envelope: Envelope) {
        if envelope.payload_type == payload::HEARTBEAT_EVENT {
            trace!("heartbeat received");
            return;
        }
        if let Some(id) = envelope.client_msg_id.as_deref() {
            let waiter = self.pending().remove(id);
            if let Some(waiter) = waiter {
                // The waiter may have timed out and gone: nothing to do then.
                let _ = waiter.send(Ok(envelope));
                return;
            }
            debug!(
                id,
                payload_type = envelope.payload_type,
                "answer with no waiting request"
            );
        }
        if let Some(event) = event_from(&envelope) {
            // No reader is not an error: events are for whoever cares to listen.
            let _ = self.events.send(event);
        }
    }

    /// Ends the connection for everyone: fails the waiting requests, publishes the state and the
    /// last event. Safe to call more than once; only the first call has an effect.
    fn finish(&self, reason: DisconnectReason) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        let waiters: Vec<_> = self.pending().drain().map(|(_, waiter)| waiter).collect();
        for waiter in waiters {
            let _ = waiter.send(Err(OpenApiError::Closed));
        }
        self.state
            .send_replace(ConnectionState::Closed(reason.clone()));
        let _ = self.events.send(Event::Disconnected(reason));
    }
}

/// A connection to the cTrader Open API. See the [module docs](self).
///
/// Cheap to clone: every clone (and every sub-client built from one, such as
/// [`AccountClient`](crate::openapi::AccountClient)) shares the same background task and socket. Calling
/// [`Client::close`] is still the right way to end a connection on purpose, but dropping every
/// clone without it does not leak the task or the socket either: the last clone going away closes
/// the connection as a fallback (see the `Drop` impl below), the same as an explicit `close()`.
pub struct Client {
    shared: Arc<Shared>,
}

impl Clone for Client {
    fn clone(&self) -> Self {
        // `Relaxed` is enough: this only counts clones, it establishes no ordering with anything
        // else the clone goes on to do.
        self.shared.handles.fetch_add(1, Ordering::Relaxed);
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // `AcqRel`: pairs with every other clone's decrement, so the one that observes the count
        // drop to zero has truly seen every earlier clone finish dropping.
        if self.shared.handles.fetch_sub(1, Ordering::AcqRel) == 1 {
            // The last external clone is gone. Ask the background task to close, the same way
            // `close()` does, so the socket and the task do not outlive every caller reference.
            self.shared.shutdown.send_replace(true);
        }
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("closed", &self.shared.closed.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Opens the connection. No message is sent yet: authenticate the application next (or use
    /// [`ClientBuilder`] to do both in one call).
    ///
    /// # Errors
    ///
    /// [`OpenApiError::Config`] for unusable settings, [`OpenApiError::Timeout`] when the connection
    /// takes longer than `connect_timeout`, [`OpenApiError::Transport`] when it fails (address, TLS,
    /// handshake).
    pub async fn connect(config: &ConnectionConfig) -> Result<Self> {
        config.validate()?;
        let (socket, _response) = tokio::time::timeout(
            config.connect_timeout,
            tokio_tungstenite::connect_async(config.url.as_str()),
        )
        .await
        .map_err(|_| OpenApiError::Timeout {
            operation: "the connection",
        })?
        .map_err(|e| OpenApiError::Transport(e.to_string()))?;

        let (outgoing_tx, outgoing_rx) = mpsc::channel(OUTGOING_QUEUE);
        let (events, _) = broadcast::channel(config.event_capacity);
        let (state, _) = watch::channel(ConnectionState::Connected);
        let (shutdown, _) = watch::channel(false);
        let shared = Arc::new(Shared {
            outgoing: outgoing_tx,
            pending: Mutex::new(HashMap::new()),
            events,
            state,
            shutdown,
            closed: AtomicBool::new(false),
            standard: RateLimiter::new(config.standard_rate),
            historical: RateLimiter::new(config.historical_rate),
            request_timeout: config.request_timeout,
            rate_limit_retries: config.rate_limit_retries,
            max_retry_wait: config.max_retry_wait,
            handles: AtomicUsize::new(1),
        });
        tokio::spawn(run(
            socket,
            outgoing_rx,
            Arc::clone(&shared),
            config.heartbeat_interval,
        ));
        Ok(Self { shared })
    }

    /// Starts a [`ClientBuilder`] for `environment`, the friendliest way to connect and (with
    /// [`ClientBuilder::credentials`]) identify the application in one step.
    #[must_use]
    pub fn builder(environment: Environment) -> ClientBuilder {
        ClientBuilder::new(environment)
    }

    /// The events the server sends: prices, order book changes, account and token notices, and
    /// finally [`Event::Disconnected`]. Each call gives an independent reader that sees the events
    /// from now on. A reader that falls more than `event_capacity` events behind loses the oldest
    /// ones (the receiver reports how many).
    #[must_use]
    pub fn events(&self) -> broadcast::Receiver<Event> {
        self.shared.events.subscribe()
    }

    /// Follows the state of the connection.
    #[must_use]
    pub fn state(&self) -> watch::Receiver<ConnectionState> {
        self.shared.state.subscribe()
    }

    /// Whether the connection has ended.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::SeqCst)
    }

    /// Closes the connection. Requests still waiting fail with [`OpenApiError::Closed`]. Calling it
    /// again, or on a closed client, does nothing.
    pub async fn close(&self) {
        if self.is_closed() {
            return;
        }
        self.shared.shutdown.send_replace(true);
    }

    // ---- the request machinery, shared with the sub-clients in crate::openapi::market, crate::openapi::account,
    // crate::openapi::trading and crate::openapi::margin ----

    /// One request with its answer, sent again when the server refuses it for its rate.
    pub(crate) async fn call<Req, Res>(
        &self,
        request_type: u32,
        response_type: u32,
        request: &Req,
        class: RateClass,
        operation: &'static str,
    ) -> Result<Res>
    where
        Req: Serialize,
        Res: DeserializeOwned,
    {
        let mut retries = 0;
        loop {
            let result = self
                .call_once(request_type, response_type, request, class, operation)
                .await;
            match result {
                Err(error)
                    if error.kind() == ErrorKind::RateLimited
                        && retries < self.shared.rate_limit_retries =>
                {
                    retries += 1;
                    // The server says how long (in seconds); without advice wait a second. A short
                    // margin covers the server counting slightly differently from the clock here.
                    let wait = error
                        .retry_after()
                        .unwrap_or(Duration::from_secs(1))
                        .clamp(Duration::from_millis(250), self.shared.max_retry_wait)
                        + Duration::from_millis(100);
                    warn!(
                        operation,
                        retries,
                        ?wait,
                        "refused for its rate, waiting to try again"
                    );
                    tokio::time::sleep(wait).await;
                }
                other => return other,
            }
        }
    }

    /// One attempt.
    async fn call_once<Req, Res>(
        &self,
        request_type: u32,
        response_type: u32,
        request: &Req,
        class: RateClass,
        operation: &'static str,
    ) -> Result<Res>
    where
        Req: Serialize,
        Res: DeserializeOwned,
    {
        let answer = self
            .exchange(request_type, request, class, operation)
            .await?;
        match answer.payload_type {
            payload::ERROR_RES | payload::PROXY_ERROR_RES => Err(error_of(&answer)),
            t if t == response_type => answer.decode(),
            other => Err(OpenApiError::Protocol(format!(
                "expected message {response_type} for {operation}, got {other}"
            ))),
        }
    }

    /// Sends a request and waits for its answer, whatever it is.
    async fn exchange<Req: Serialize>(
        &self,
        request_type: u32,
        request: &Req,
        class: RateClass,
        operation: &'static str,
    ) -> Result<Envelope> {
        if self.is_closed() {
            return Err(OpenApiError::Closed);
        }
        match class {
            RateClass::Standard => self.shared.standard.acquire().await,
            RateClass::Historical => self.shared.historical.acquire().await,
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let text = Envelope::request(request_type, id.clone(), request)?.to_text()?;

        let (tx, rx) = oneshot::channel();
        self.shared.pending().insert(id.clone(), tx);
        let _pending = PendingRequest {
            shared: &self.shared,
            id,
        };
        // A connection that ended between the check above and now would leave this waiter alone
        // forever, so look again after registering.
        if self.is_closed() {
            return Err(OpenApiError::Closed);
        }
        match tokio::time::timeout(
            self.shared.request_timeout,
            self.shared.outgoing.send(Outgoing::Text(text)),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return Err(OpenApiError::Closed),
            Err(_) => return Err(OpenApiError::Timeout { operation }),
        }

        match tokio::time::timeout(self.shared.request_timeout, rx).await {
            Ok(Ok(answer)) => answer,
            // The sender was dropped without an answer: the connection is gone.
            Ok(Err(_)) => Err(OpenApiError::Closed),
            Err(_) => Err(OpenApiError::Timeout { operation }),
        }
    }

    // ---- sign in ----

    /// Identifies the application to the server. Must come first on a connection.
    ///
    /// # Errors
    ///
    /// The server refuses unknown or unapproved credentials (`CH_CLIENT_AUTH_FAILURE`,
    /// `CH_OA_CLIENT_NOT_FOUND`), or the connection fails.
    pub async fn authenticate_application(&self, credentials: &ClientCredentials) -> Result<()> {
        let request = ApplicationAuthReq {
            client_id: credentials.client_id.clone(),
            client_secret: credentials.client_secret.expose_secret().to_owned(),
        };
        let _: serde_json::Value = self
            .call(
                payload::APPLICATION_AUTH_REQ,
                payload::APPLICATION_AUTH_RES,
                &request,
                RateClass::Standard,
                "the application sign in",
            )
            .await?;
        Ok(())
    }

    /// The trading accounts an access token covers, with the permission it grants.
    ///
    /// # Errors
    ///
    /// `CH_ACCESS_TOKEN_INVALID` or `OA_AUTH_TOKEN_EXPIRED` for a bad token.
    pub async fn accounts(&self, access_token: &str) -> Result<AccountsRes> {
        self.call(
            payload::GET_ACCOUNTS_BY_ACCESS_TOKEN_REQ,
            payload::GET_ACCOUNTS_BY_ACCESS_TOKEN_RES,
            &GetAccountsByAccessTokenReq {
                access_token: access_token.to_owned(),
            },
            RateClass::Standard,
            "the account list",
        )
        .await
    }

    /// Authorizes a trading account on this connection. Reached only through
    /// [`crate::openapi::AccountClient::authorize`], which is how every other call ends up bound to the
    /// right account.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED`, or a token error. A demo account cannot be authorized on a live
    /// connection, and the other way round.
    pub(crate) async fn authorize_account(
        &self,
        account_id: i64,
        access_token: &str,
    ) -> Result<i64> {
        let response: AccountAuthRes = self
            .call(
                payload::ACCOUNT_AUTH_REQ,
                payload::ACCOUNT_AUTH_RES,
                &AccountAuthReq {
                    ctid_trader_account_id: account_id,
                    access_token: access_token.to_owned(),
                },
                RateClass::Standard,
                "the account sign in",
            )
            .await?;
        Ok(response.ctid_trader_account_id)
    }

    /// Exchanges a refresh token for a new pair of tokens, over the connection. The old refresh
    /// token stops working: store the new one before doing anything else. The HTTP route in
    /// [`crate::openapi::auth`] does the same without a connection.
    ///
    /// # Errors
    ///
    /// A server error when the refresh token is unknown or already used.
    pub async fn refresh_tokens(&self, refresh_token: &str) -> Result<RefreshTokenRes> {
        self.call(
            payload::REFRESH_TOKEN_REQ,
            payload::REFRESH_TOKEN_RES,
            &RefreshTokenReq {
                refresh_token: refresh_token.to_owned(),
            },
            RateClass::Standard,
            "the token refresh",
        )
        .await
    }

    /// The version of the proxy the connection goes through.
    ///
    /// # Errors
    ///
    /// Only connection errors.
    pub async fn version(&self) -> Result<String> {
        let response: VersionRes = self
            .call(
                payload::VERSION_REQ,
                payload::VERSION_RES,
                &VersionReq {},
                RateClass::Standard,
                "the version",
            )
            .await?;
        Ok(response.version)
    }

    /// The profile of the cTrader ID an access token belongs to. Needs only an access token, no
    /// authorized account.
    ///
    /// # Errors
    ///
    /// A token error for a bad or expired token.
    pub async fn ctid_profile(&self, access_token: &str) -> Result<CtidProfile> {
        let response: CtidProfileRes = self
            .call(
                payload::GET_CTID_PROFILE_BY_TOKEN_REQ,
                payload::GET_CTID_PROFILE_BY_TOKEN_RES,
                &CtidProfileReq {
                    access_token: access_token.to_owned(),
                },
                RateClass::Standard,
                "the cTrader ID profile",
            )
            .await?;
        Ok(response.profile)
    }
}

/// Builds a [`Client`]: the friendliest entry point for a newcomer, connecting and (optionally)
/// identifying the application in one call. Skip it and call [`Client::connect`] directly when the
/// two steps are better kept apart (for example to report progress between them).
///
/// ```no_run
/// use wyck::openapi::{ClientBuilder, ClientCredentials, Environment};
///
/// # async fn demo() -> wyck::openapi::Result<()> {
/// let client = ClientBuilder::new(Environment::Demo)
///     .credentials(ClientCredentials::new("my-client-id", "my-client-secret"))
///     .connect()
///     .await?;
/// # let _ = client; Ok(()) }
/// ```
#[derive(Debug, Clone)]
pub struct ClientBuilder {
    config: ConnectionConfig,
    credentials: Option<ClientCredentials>,
}

impl ClientBuilder {
    /// A builder with the default settings for `environment`.
    #[must_use]
    pub fn new(environment: Environment) -> Self {
        Self {
            config: ConnectionConfig::new(environment),
            credentials: None,
        }
    }

    /// A builder with explicit connection settings (a test server, different timeouts, ...).
    #[must_use]
    pub fn with_config(config: ConnectionConfig) -> Self {
        Self {
            config,
            credentials: None,
        }
    }

    /// Identifies the application once connected. Without this, [`ClientBuilder::connect`] only
    /// opens the socket, same as [`Client::connect`].
    #[must_use]
    pub fn credentials(mut self, credentials: ClientCredentials) -> Self {
        self.credentials = Some(credentials);
        self
    }

    /// Connects, and identifies the application when [`ClientBuilder::credentials`] were given.
    ///
    /// # Errors
    ///
    /// Whatever [`Client::connect`] and [`Client::authenticate_application`] return.
    pub async fn connect(self) -> Result<Client> {
        let client = Client::connect(&self.config).await?;
        if let Some(credentials) = &self.credentials {
            client.authenticate_application(credentials).await?;
        }
        Ok(client)
    }
}

/// The connection task: reads the socket, writes what the client queues, sends heartbeats.
async fn run<S>(
    socket: tokio_tungstenite::WebSocketStream<S>,
    mut outgoing: mpsc::Receiver<Outgoing>,
    shared: Arc<Shared>,
    heartbeat: Duration,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut sink, mut stream) = socket.split();
    let mut beat = tokio::time::interval(heartbeat);
    beat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick of an interval is immediate; the connection is fresh, skip it.
    beat.tick().await;
    let mut shutdown = shared.shutdown.subscribe();

    let reason = loop {
        if *shutdown.borrow() {
            break DisconnectReason::ClosedByClient;
        }
        tokio::select! {
            biased;
            _ = shutdown.changed() => break DisconnectReason::ClosedByClient,
            message = outgoing.recv() => match message {
                Some(Outgoing::Text(text)) => {
                    let sent = tokio::select! {
                        _ = shutdown.changed() => break DisconnectReason::ClosedByClient,
                        sent = tokio::time::timeout(
                            shared.request_timeout,
                            sink.send(Message::text(text)),
                        ) => sent,
                    };
                    match sent {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => break DisconnectReason::Failed(error.to_string()),
                        Err(_) => break DisconnectReason::Failed("WebSocket write timed out".into()),
                    }
                }
                None => break DisconnectReason::ClosedByClient,
            },
            frame = stream.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    if let Some(reason) = handle_text(&shared, text.as_str()) {
                        break reason;
                    }
                }
                Some(Ok(Message::Binary(bytes))) => match std::str::from_utf8(&bytes) {
                    Ok(text) => {
                        if let Some(reason) = handle_text(&shared, text) {
                            break reason;
                        }
                    }
                    Err(_) => warn!("a binary frame that is not text was ignored"),
                },
                // The library answers pings by itself; pongs and raw frames need nothing.
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {}
                Some(Ok(Message::Close(_))) | None => break DisconnectReason::ClosedByServer,
                Some(Err(error)) => break DisconnectReason::Failed(error.to_string()),
            },
            _ = beat.tick() => {
                let text = match Envelope::heartbeat().to_text() {
                    Ok(text) => text,
                    Err(_) => continue,
                };
                let sent = tokio::select! {
                    _ = shutdown.changed() => break DisconnectReason::ClosedByClient,
                    sent = tokio::time::timeout(
                        shared.request_timeout,
                        sink.send(Message::text(text)),
                    ) => sent,
                };
                match sent {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => break DisconnectReason::Failed(error.to_string()),
                    Err(_) => break DisconnectReason::Failed("WebSocket heartbeat timed out".into()),
                }
            }
        }
    };
    if matches!(reason, DisconnectReason::ClosedByClient) {
        let _ = tokio::time::timeout(Duration::from_secs(1), sink.send(Message::Close(None))).await;
    }
    debug!(?reason, "the Open API connection ended");
    shared.finish(reason);
}

/// Reads one text frame from the server. Returns the reason when the server announced that it is
/// ending the connection, which is more telling than the socket closing after it.
fn handle_text(shared: &Shared, text: &str) -> Option<DisconnectReason> {
    let envelope = match Envelope::from_text(text) {
        Ok(envelope) => envelope,
        Err(error) => {
            warn!(%error, "a message that could not be read was ignored");
            return None;
        }
    };
    let announced = (envelope.payload_type == payload::CLIENT_DISCONNECT_EVENT).then(|| {
        DisconnectReason::ServerAnnounced(
            envelope
                .payload
                .get("reason")
                .and_then(|r| r.as_str())
                .map(str::to_owned),
        )
    });
    shared.route(envelope);
    announced
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    #[tokio::test]
    async fn cancelled_requests_leave_no_pending_waiters() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let (seen, mut received) = mpsc::unbounded_channel();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(socket).await.unwrap();
            while let Some(Ok(Message::Text(text))) = socket.next().await {
                let envelope = Envelope::from_text(&text).unwrap();
                if envelope.payload_type == payload::VERSION_REQ {
                    seen.send(()).unwrap();
                }
            }
        });
        let mut config = ConnectionConfig::with_url(url);
        config.heartbeat_interval = Duration::from_secs(5);
        let client = Client::connect(&config).await.unwrap();
        for _ in 0..20 {
            let call = tokio::spawn({
                let client = client.clone();
                async move { client.version().await }
            });
            tokio::time::timeout(Duration::from_secs(2), received.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(client.shared.pending().len(), 1);
            call.abort();
            let _ = call.await;
            assert!(client.shared.pending().is_empty());
        }
        client.close().await;
        server.abort();
    }
}

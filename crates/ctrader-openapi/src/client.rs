//! The connection to the Open API, and one method per call.
//!
//! A [`Client`] owns one WebSocket to `demo` or `live`. A background task reads it, keeps it alive
//! with heartbeats, matches answers to requests, and broadcasts everything else as [`Event`]s.
//! The client itself is cheap to clone: every clone talks to the same connection.
//!
//! # A session, step by step
//!
//! 1. [`Client::connect`] opens the socket.
//! 2. [`Client::authenticate_application`] identifies the application (client id and secret).
//! 3. [`Client::accounts`] lists the trading accounts an access token covers, and
//!    [`Client::authorize_account`] authorizes one on this connection.
//! 4. Then the data calls: [`Client::symbols`], [`Client::subscribe_spots`], [`Client::bars_page`],
//!    [`Client::tick_page`] (see [`crate::history`] for whole ranges), and so on.
//!
//! # How requests work
//!
//! Every request gets a unique `clientMsgId`; the answer carries it back, so many requests can be
//! in flight at once and answers may come in any order. A request waits for its turn at the
//! [rate limiter](crate::rate_limit::RateLimiter) (50 per second, 5 for history), then for its
//! answer up to the configured timeout. A server error becomes an [`OpenApiError::Server`].
//!
//! # When the connection ends
//!
//! Every waiting request fails with [`OpenApiError::Closed`], an [`Event::Disconnected`] is
//! broadcast, and [`Client::state`] turns to [`ConnectionState::Closed`]. **The client does not
//! reconnect by itself**: after a reconnect the application and the accounts must be authorized
//! again and the subscriptions renewed, which is the caller's business (the engine already has a
//! supervisor for that with the MCP sessions). Build a new [`Client`] and repeat the steps.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use secrecy::ExposeSecret;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, trace, warn};

use crate::config::{ClientCredentials, ConnectionConfig};
use crate::error::{OpenApiError, Result};
use crate::event::{DisconnectReason, Event, error_of, event_from};
use crate::model::{
    AccountAuthReq, AccountAuthRes, AccountsRes, ApplicationAuthReq, GetAccountsByAccessTokenReq,
    GetTickDataReq, GetTickDataRes, GetTrendbarsReq, GetTrendbarsRes, LightSymbol, LiveTrendbarReq,
    RefreshTokenReq, RefreshTokenRes, SubscribeSpotsReq, Symbol, SymbolByIdReq, SymbolByIdRes,
    SymbolsListReq, SymbolsListRes, SymbolsReq, VersionRes,
};
use crate::rate_limit::RateLimiter;
use crate::types::{Bar, Period, QuoteType, Tick, decode_bars, decode_ticks};
use crate::wire::{Envelope, payload};

/// How many outgoing messages may queue before a sender waits.
const OUTGOING_QUEUE: usize = 256;

/// Which limit a request counts against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RateClass {
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
    Close,
}

/// What every clone of a client shares.
struct Shared {
    outgoing: mpsc::Sender<Outgoing>,
    pending: Mutex<HashMap<String, oneshot::Sender<Result<Envelope>>>>,
    events: broadcast::Sender<Event>,
    state: watch::Sender<ConnectionState>,
    closed: AtomicBool,
    standard: RateLimiter,
    historical: RateLimiter,
    request_timeout: Duration,
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
#[derive(Clone)]
pub struct Client {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("closed", &self.shared.closed.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Opens the connection. No message is sent yet: authenticate the application next.
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
        let shared = Arc::new(Shared {
            outgoing: outgoing_tx,
            pending: Mutex::new(HashMap::new()),
            events,
            state,
            closed: AtomicBool::new(false),
            standard: RateLimiter::new(config.standard_rate),
            historical: RateLimiter::new(config.historical_rate),
            request_timeout: config.request_timeout,
        });
        tokio::spawn(run(
            socket,
            outgoing_rx,
            Arc::clone(&shared),
            config.heartbeat_interval,
        ));
        Ok(Self { shared })
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
        // If the queue is full or gone the connection is ending anyway.
        let _ = self.shared.outgoing.send(Outgoing::Close).await;
    }

    // ---- the request machinery ----

    async fn call<Req, Res>(
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
        // A connection that ended between the check above and now would leave this waiter alone
        // forever, so look again after registering.
        if self.is_closed() {
            self.shared.pending().remove(&id);
            return Err(OpenApiError::Closed);
        }
        if self
            .shared
            .outgoing
            .send(Outgoing::Text(text))
            .await
            .is_err()
        {
            self.shared.pending().remove(&id);
            return Err(OpenApiError::Closed);
        }

        match tokio::time::timeout(self.shared.request_timeout, rx).await {
            Ok(Ok(answer)) => answer,
            // The sender was dropped without an answer: the connection is gone.
            Ok(Err(_)) => Err(OpenApiError::Closed),
            Err(_) => {
                self.shared.pending().remove(&id);
                Err(OpenApiError::Timeout { operation })
            }
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

    /// Authorizes a trading account on this connection. Data calls for the account need it.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED`, or a token error. A demo account cannot be authorized on a live
    /// connection, and the other way round.
    pub async fn authorize_account(&self, account_id: i64, access_token: &str) -> Result<i64> {
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
    /// [`crate::auth`] does the same without a connection.
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
                &serde_json::json!({}),
                RateClass::Standard,
                "the version",
            )
            .await?;
        Ok(response.version)
    }

    // ---- symbols ----

    /// Every symbol of the account, in short form. Archived symbols are left out unless asked for.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn symbols(
        &self,
        account_id: i64,
        include_archived: bool,
    ) -> Result<Vec<LightSymbol>> {
        let response: SymbolsListRes = self
            .call(
                payload::SYMBOLS_LIST_REQ,
                payload::SYMBOLS_LIST_RES,
                &SymbolsListReq {
                    ctid_trader_account_id: account_id,
                    include_archived_symbols: include_archived.then_some(true),
                },
                RateClass::Standard,
                "the symbol list",
            )
            .await?;
        Ok(response.symbol)
    }

    /// The details of some symbols: decimals, pip position, volume rules.
    ///
    /// # Errors
    ///
    /// `SYMBOL_NOT_FOUND` for an unknown id.
    pub async fn symbol_details(&self, account_id: i64, symbol_ids: &[i64]) -> Result<Vec<Symbol>> {
        let response: SymbolByIdRes = self
            .call(
                payload::SYMBOL_BY_ID_REQ,
                payload::SYMBOL_BY_ID_RES,
                &SymbolByIdReq {
                    ctid_trader_account_id: account_id,
                    symbol_id: symbol_ids.to_vec(),
                },
                RateClass::Standard,
                "the symbol details",
            )
            .await?;
        Ok(response.symbol)
    }

    // ---- live data ----

    /// Follows the prices of some symbols. The first [`Event::Spot`] of each carries the latest
    /// price even when the market is closed; then one arrives at every change of bid or ask.
    /// `with_timestamp` asks the server to stamp each event with its time.
    ///
    /// # Errors
    ///
    /// `ALREADY_SUBSCRIBED` or `SYMBOL_NOT_FOUND` for a bad id.
    pub async fn subscribe_spots(
        &self,
        account_id: i64,
        symbol_ids: &[i64],
        with_timestamp: bool,
    ) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                payload::SUBSCRIBE_SPOTS_REQ,
                payload::SUBSCRIBE_SPOTS_RES,
                &SubscribeSpotsReq {
                    ctid_trader_account_id: account_id,
                    symbol_id: symbol_ids.to_vec(),
                    subscribe_to_spot_timestamp: with_timestamp.then_some(true),
                },
                RateClass::Standard,
                "the price subscription",
            )
            .await?;
        Ok(())
    }

    /// Stops following the prices of some symbols.
    ///
    /// # Errors
    ///
    /// `NOT_SUBSCRIBED_TO_SPOTS` when there was no subscription.
    pub async fn unsubscribe_spots(&self, account_id: i64, symbol_ids: &[i64]) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                payload::UNSUBSCRIBE_SPOTS_REQ,
                payload::UNSUBSCRIBE_SPOTS_RES,
                &SymbolsReq {
                    ctid_trader_account_id: account_id,
                    symbol_id: symbol_ids.to_vec(),
                },
                RateClass::Standard,
                "ending the price subscription",
            )
            .await?;
        Ok(())
    }

    /// Follows the live bar of a symbol at a period: the bar in progress arrives inside the
    /// [`Event::Spot`] events. Needs a price subscription on the same symbol first.
    ///
    /// # Errors
    ///
    /// `NOT_SUBSCRIBED_TO_SPOTS` without the price subscription.
    pub async fn subscribe_live_bars(
        &self,
        account_id: i64,
        symbol_id: i64,
        period: Period,
    ) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                payload::SUBSCRIBE_LIVE_TRENDBAR_REQ,
                payload::SUBSCRIBE_LIVE_TRENDBAR_RES,
                &LiveTrendbarReq {
                    ctid_trader_account_id: account_id,
                    period: period.number(),
                    symbol_id,
                },
                RateClass::Standard,
                "the live bar subscription",
            )
            .await?;
        Ok(())
    }

    /// Stops following the live bar of a symbol at a period.
    ///
    /// # Errors
    ///
    /// A server error when there was no such subscription.
    pub async fn unsubscribe_live_bars(
        &self,
        account_id: i64,
        symbol_id: i64,
        period: Period,
    ) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                payload::UNSUBSCRIBE_LIVE_TRENDBAR_REQ,
                payload::UNSUBSCRIBE_LIVE_TRENDBAR_RES,
                &LiveTrendbarReq {
                    ctid_trader_account_id: account_id,
                    period: period.number(),
                    symbol_id,
                },
                RateClass::Standard,
                "ending the live bar subscription",
            )
            .await?;
        Ok(())
    }

    /// Follows the order book of some symbols ([`Event::Depth`]). Not every broker offers it.
    ///
    /// # Errors
    ///
    /// A server error when the broker has no depth for the symbol.
    pub async fn subscribe_depth(&self, account_id: i64, symbol_ids: &[i64]) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                payload::SUBSCRIBE_DEPTH_QUOTES_REQ,
                payload::SUBSCRIBE_DEPTH_QUOTES_RES,
                &SymbolsReq {
                    ctid_trader_account_id: account_id,
                    symbol_id: symbol_ids.to_vec(),
                },
                RateClass::Standard,
                "the depth subscription",
            )
            .await?;
        Ok(())
    }

    /// Stops following the order book of some symbols.
    ///
    /// # Errors
    ///
    /// A server error when there was no such subscription.
    pub async fn unsubscribe_depth(&self, account_id: i64, symbol_ids: &[i64]) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                payload::UNSUBSCRIBE_DEPTH_QUOTES_REQ,
                payload::UNSUBSCRIBE_DEPTH_QUOTES_RES,
                &SymbolsReq {
                    ctid_trader_account_id: account_id,
                    symbol_id: symbol_ids.to_vec(),
                },
                RateClass::Standard,
                "ending the depth subscription",
            )
            .await?;
        Ok(())
    }

    // ---- history, one page ----

    /// One request for bars in `[from_ms, to_ms]`. Returns the bars, oldest first, and whether more
    /// exist in the range than were returned. The range is limited per period by the server (see
    /// [`crate::history::fetch_bars`] for a whole range in pages).
    ///
    /// # Errors
    ///
    /// `INCORRECT_BOUNDARIES` for a range the server refuses, and the usual account errors.
    pub async fn bars_page(
        &self,
        account_id: i64,
        symbol_id: i64,
        period: Period,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<(Vec<Bar>, bool)> {
        let response: GetTrendbarsRes = self
            .call(
                payload::GET_TRENDBARS_REQ,
                payload::GET_TRENDBARS_RES,
                &GetTrendbarsReq {
                    ctid_trader_account_id: account_id,
                    from_timestamp: Some(from_ms),
                    to_timestamp: Some(to_ms),
                    period: period.number(),
                    symbol_id,
                    count: None,
                },
                RateClass::Historical,
                "the bar history",
            )
            .await?;
        Ok((
            decode_bars(&response.trendbar),
            response.has_more.unwrap_or(false),
        ))
    }

    /// One request for the ticks of one side in `[from_ms, to_ms]`, which may span at most one
    /// week. Returns the ticks with absolute times, oldest first, and whether more exist in the
    /// range than were returned (the ones returned are the **newest**; see
    /// [`crate::history::fetch_ticks`] for a whole range).
    ///
    /// # Errors
    ///
    /// `INCORRECT_BOUNDARIES` for a range over a week, and the usual account errors.
    pub async fn tick_page(
        &self,
        account_id: i64,
        symbol_id: i64,
        side: QuoteType,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<(Vec<Tick>, bool)> {
        let response: GetTickDataRes = self
            .call(
                payload::GET_TICK_DATA_REQ,
                payload::GET_TICK_DATA_RES,
                &GetTickDataReq {
                    ctid_trader_account_id: account_id,
                    symbol_id,
                    r#type: side.number(),
                    from_timestamp: Some(from_ms),
                    to_timestamp: Some(to_ms),
                },
                RateClass::Historical,
                "the tick history",
            )
            .await?;
        Ok((decode_ticks(&response.tick_data), response.has_more))
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

    let reason = loop {
        tokio::select! {
            message = outgoing.recv() => match message {
                Some(Outgoing::Text(text)) => {
                    if let Err(error) = sink.send(Message::text(text)).await {
                        break DisconnectReason::Failed(error.to_string());
                    }
                }
                Some(Outgoing::Close) | None => {
                    let _ = sink.send(Message::Close(None)).await;
                    break DisconnectReason::ClosedByClient;
                }
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
                if let Err(error) = sink.send(Message::text(text)).await {
                    break DisconnectReason::Failed(error.to_string());
                }
            }
        }
    };
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

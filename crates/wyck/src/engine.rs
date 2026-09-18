//! The connection engine: the async task that owns the `ctrader-mcp` client and talks
//! to cTrader, decoupled from the render loop by two `tokio::mpsc` channels.
//!
//! # Why a separate task instead of calling `ctrader-mcp` straight from the UI loop
//!
//! [`crate::app::App::run`] has to stay responsive to keypresses every frame; an MCP
//! round-trip (connect, bootstrap, a balance refresh) must never block it. Running that
//! work on its own [`tokio::spawn`]ed task and shuttling [`EngineCommand`]s /
//! [`EngineEvent`]s across channels keeps the two concerns — "talk to cTrader" and
//! "render the terminal" — from ever blocking one another, and happens to be exactly
//! the shape the project's own README describes as the eventual target (a headless
//! engine the UI talks to over a typed channel instead of importing the trading logic
//! directly): this module is that split, just not yet pulled into its own crate/process.
//! Promoting it later is a mechanical move, not a rewrite.

use std::time::Duration;

use ctrader_mcp::workflows::{RemoteSessionContext, bootstrap_remote};
use ctrader_mcp::{CTraderError, ConnectionConfig, RemoteClient};
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

/// How often the engine refreshes account state from an already-connected client.
const REFRESH_INTERVAL: Duration = Duration::from_secs(10);

/// The size of both the command and event channels. Small on purpose: commands are rare
/// user-driven actions (connect, manual refresh) and events are consumed every frame, so
/// there is never a legitimate reason for either channel to need deep buffering — a full
/// channel is a sign something downstream has stopped consuming, which
/// [`mpsc::Sender::send`] surfaces to the caller as a `Result` rather than silently
/// growing memory.
const CHANNEL_CAPACITY: usize = 16;

/// A snapshot of account state, decoded to display values, for the UI to render.
#[derive(Debug, Clone)]
pub struct AccountSnapshot {
    pub trader_id: Option<i64>,
    pub account_currency: Option<String>,
    pub balance: Option<f64>,
    pub equity: Option<f64>,
    pub free_margin: Option<f64>,
    pub server_version: Option<String>,
}

impl AccountSnapshot {
    /// Builds the initial snapshot straight from a freshly bootstrapped session (W0) —
    /// every field it has is authoritative at this point.
    fn from_bootstrap(session: &RemoteSessionContext) -> Self {
        Self {
            trader_id: session.trader_id,
            account_currency: session.account_currency.clone(),
            balance: session.balance_display,
            equity: session.equity_display,
            free_margin: session.free_margin_display,
            server_version: session.version.clone(),
        }
    }
}

/// A request the UI sends to the engine.
#[derive(Debug)]
pub enum EngineCommand {
    /// Connect to `endpoint` with `token`, then run session bootstrap (W0).
    Connect {
        endpoint: String,
        token: SecretString,
    },
    /// Re-fetch balance from the currently connected client, if any. A no-op (silently
    /// ignored) if nothing is connected yet.
    RefreshAccount,
}

/// A notification the engine sends to the UI.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// A `Connect` command was accepted and the connection attempt has started.
    Connecting,
    /// Connection and bootstrap both succeeded.
    Connected(AccountSnapshot),
    /// Connection or bootstrap failed. The engine drops back to disconnected state —
    /// the UI should offer the user a way to retry (e.g. re-submit the first-run form).
    ConnectionFailed(String),
    /// A periodic or manual refresh succeeded.
    AccountRefreshed(AccountSnapshot),
    /// A periodic or manual refresh failed. The engine keeps the previous connection
    /// (a single failed refresh isn't treated as a disconnect — transient network
    /// blips shouldn't drop the user back to the connect screen).
    RefreshFailed(String),
}

/// Everything the engine keeps about the current connection, bundled so
/// [`Engine::run`]'s `select!` loop has one `Option` to check rather than several that
/// could (by a future edit) drift out of sync with each other.
struct Connection {
    client: RemoteClient,
    /// Cached from the bootstrap that produced `client` — `account_currency` and
    /// `server_version` in particular are session-stable (per
    /// `references/remote-http-server.md` "Symbol cache discipline", the same
    /// discipline W0 applies to the account/asset lookups this crate derives them
    /// from), so a periodic balance-only refresh reuses them instead of re-deriving
    /// them from a fresh `get_assets`/`get_symbols` round-trip every tick.
    session: RemoteSessionContext,
}

/// The engine's own handle to its channels, plus the two ends the caller keeps.
pub struct Engine {
    commands: mpsc::Receiver<EngineCommand>,
    events: mpsc::Sender<EngineEvent>,
}

impl Engine {
    /// Builds a new engine and the channel endpoints [`crate::app::App`] uses to drive
    /// it: a [`Sender`](mpsc::Sender) for [`EngineCommand`]s and a
    /// [`Receiver`](mpsc::Receiver) for [`EngineEvent`]s.
    pub fn new() -> (
        Self,
        mpsc::Sender<EngineCommand>,
        mpsc::Receiver<EngineEvent>,
    ) {
        let (command_tx, command_rx) = mpsc::channel(CHANNEL_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel(CHANNEL_CAPACITY);
        (
            Self {
                commands: command_rx,
                events: event_tx,
            },
            command_tx,
            event_rx,
        )
    }

    /// Runs the engine until its command channel closes (i.e. every [`mpsc::Sender`]
    /// clone was dropped — in practice, when [`crate::app::App::run`] returns and drops
    /// its sender). Intended to be driven by [`tokio::spawn`].
    pub async fn run(mut self) {
        let mut connection: Option<Connection> = None;

        let mut refresh_ticker = tokio::time::interval(REFRESH_INTERVAL);
        refresh_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                command = self.commands.recv() => {
                    let Some(command) = command else {
                        tracing::debug!("engine command channel closed; shutting down");
                        break;
                    };
                    self.handle_command(command, &mut connection).await;
                }
                _ = refresh_ticker.tick(), if connection.is_some() => {
                    if let Some(active) = &connection {
                        Self::refresh(active, &self.events).await;
                    }
                }
            }
        }
    }

    async fn handle_command(
        &mut self,
        command: EngineCommand,
        connection: &mut Option<Connection>,
    ) {
        match command {
            EngineCommand::Connect { endpoint, token } => {
                let _ = self.events.send(EngineEvent::Connecting).await;
                match connect_and_bootstrap(&endpoint, token).await {
                    Ok((client, session)) => {
                        tracing::info!(%endpoint, trader_id = ?session.trader_id, "connected to cTrader");
                        let snapshot = AccountSnapshot::from_bootstrap(&session);
                        *connection = Some(Connection { client, session });
                        let _ = self.events.send(EngineEvent::Connected(snapshot)).await;
                    }
                    Err(source) => {
                        tracing::warn!(%endpoint, error = %source, "connection failed");
                        *connection = None;
                        let _ = self
                            .events
                            .send(EngineEvent::ConnectionFailed(source.to_string()))
                            .await;
                    }
                }
            }
            EngineCommand::RefreshAccount => {
                if let Some(active) = connection.as_ref() {
                    Self::refresh(active, &self.events).await;
                }
            }
        }
    }

    async fn refresh(connection: &Connection, events: &mpsc::Sender<EngineEvent>) {
        match connection.client.get_balance().await {
            Ok(balance) => {
                let snapshot = AccountSnapshot {
                    trader_id: balance.trader_id,
                    account_currency: connection.session.account_currency.clone(),
                    balance: balance.display_balance(),
                    equity: balance.display_equity(),
                    free_margin: balance.display_free_margin(),
                    server_version: connection.session.version.clone(),
                };
                let _ = events.send(EngineEvent::AccountRefreshed(snapshot)).await;
            }
            Err(source) => {
                tracing::warn!(error = %source, "account refresh failed");
                let _ = events
                    .send(EngineEvent::RefreshFailed(source.to_string()))
                    .await;
            }
        }
    }
}

async fn connect_and_bootstrap(
    endpoint: &str,
    token: SecretString,
) -> Result<(RemoteClient, RemoteSessionContext), CTraderError> {
    let connection = ConnectionConfig::new(endpoint).with_bearer_token(token.expose_secret());
    let client = RemoteClient::connect(&connection).await?;
    let session = bootstrap_remote(&client).await?;
    Ok((client, session))
}

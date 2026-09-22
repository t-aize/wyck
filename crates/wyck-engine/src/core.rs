//! The engine's shared core: state publication, event emission, and the broker session
//! (connect, refresh, supervise, reconnect).
//!
//! Everything here runs on the engine's own Tokio runtime. Public handles reach it only by
//! spawning onto that runtime (see [`EngineHandle`](crate::EngineHandle)), which is what
//! keeps the engine usable from a front end whose executor is not Tokio.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::{broadcast, watch};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::broker::{Broker, ConnectRequest, Connector};
use crate::config::EngineConfig;
use crate::domain::{
    AccountSnapshot, Candle, Instrument, PendingOrder, Period, Position, Quote, SymbolInfo,
    UnixMillis, now_millis,
};
use crate::error::{EngineError, Result};
use crate::event::{Event, EventKind};
use crate::ids::{AccountId, CommandId, PositionId};
use crate::risk::{OrderPlan, PlanId};
use crate::state::{EngineState, SessionState, TradingMode, Warning, WarningKind};
use crate::trading::{FlattenPreview, UnknownOrder};

/// The bookkeeping the order pipeline needs between calls.
#[derive(Default)]
pub(crate) struct Gate {
    pub(crate) last_order_at: HashMap<String, Instant>,
    pub(crate) in_flight: HashSet<String>,
    pub(crate) unknown: Vec<UnknownOrder>,
    pub(crate) flatten: Option<(FlattenPreview, Instant)>,
}

pub(crate) struct StoredPlan {
    pub(crate) plan: OrderPlan,
    pub(crate) at: Instant,
}

#[derive(Default)]
struct Conn {
    generation: u64,
    token: Option<CancellationToken>,
}

/// Shared by every task and handle. See the module docs.
pub(crate) struct Inner {
    pub(crate) config: EngineConfig,
    connector: Arc<dyn Connector>,
    state: watch::Sender<Arc<EngineState>>,
    events: broadcast::Sender<Event>,
    activity: Mutex<VecDeque<Event>>,
    broker: Mutex<Option<Arc<dyn Broker>>>,
    conn: Mutex<Conn>,
    pub(crate) gate: Mutex<Gate>,
    pub(crate) plans: Mutex<HashMap<PlanId, StoredPlan>>,
    instruments: Mutex<HashMap<String, Instrument>>,
    /// The symbol list of the current session, with the connection generation it was read for.
    catalog: Mutex<Option<(u64, Arc<Vec<SymbolInfo>>)>>,
    watched: Mutex<BTreeSet<String>>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) tracker: TaskTracker,
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Inner {
    pub(crate) fn new(config: EngineConfig, connector: Arc<dyn Connector>) -> Arc<Self> {
        let (events, _) = broadcast::channel(config.event_buffer);
        let (state, _) = watch::channel(Arc::new(EngineState::default()));
        Arc::new(Self {
            config,
            connector,
            state,
            events,
            activity: Mutex::new(VecDeque::new()),
            broker: Mutex::new(None),
            conn: Mutex::new(Conn::default()),
            gate: Mutex::new(Gate::default()),
            plans: Mutex::new(HashMap::new()),
            instruments: Mutex::new(HashMap::new()),
            catalog: Mutex::new(None),
            watched: Mutex::new(BTreeSet::new()),
            shutdown: CancellationToken::new(),
            tracker: TaskTracker::new(),
        })
    }

    // ---- state and events ----

    pub(crate) fn snapshot(&self) -> Arc<EngineState> {
        self.state.borrow().clone()
    }

    pub(crate) fn watch_state(&self) -> watch::Receiver<Arc<EngineState>> {
        self.state.subscribe()
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub(crate) fn recent_events(&self) -> Vec<Event> {
        lock(&self.activity).iter().cloned().collect()
    }

    /// Applies `f` to the state and publishes it with a new revision.
    pub(crate) fn update(&self, f: impl FnOnce(&mut EngineState)) {
        self.state.send_modify(|arc| {
            let s = Arc::make_mut(arc);
            f(s);
            s.revision = s.revision.next();
        });
    }

    pub(crate) fn emit(&self, command: Option<CommandId>, kind: EventKind) {
        let event = Event {
            at: now_millis(),
            account: self.snapshot().account_id.clone(),
            command,
            kind,
        };
        {
            let mut log = lock(&self.activity);
            if log.len() >= self.config.activity_log_len.max(1) {
                log.pop_front();
            }
            log.push_back(event.clone());
        }
        // No receiver is fine: events are for whoever is listening.
        let _ = self.events.send(event);
    }

    pub(crate) fn raise_warning(&self, id: String, kind: WarningKind, message: String) {
        let warning = Warning {
            id,
            kind,
            message,
            raised_at: now_millis(),
        };
        let mut is_new = false;
        self.update(|s| {
            if let Some(existing) = s.warnings.iter_mut().find(|w| w.id == warning.id) {
                existing.message.clone_from(&warning.message);
            } else {
                s.warnings.push(warning.clone());
                is_new = true;
            }
        });
        if is_new {
            self.emit(None, EventKind::WarningRaised(warning));
        }
    }

    pub(crate) fn clear_warning(&self, id: &str) -> bool {
        let mut removed = false;
        self.update(|s| {
            let before = s.warnings.len();
            s.warnings.retain(|w| w.id != id);
            removed = s.warnings.len() != before;
        });
        if removed {
            self.emit(None, EventKind::WarningCleared { id: id.to_owned() });
        }
        removed
    }

    // ---- mode ----

    pub(crate) fn set_mode(&self, mode: TradingMode, reason: &str, command: Option<CommandId>) {
        let mut changed = false;
        self.update(|s| {
            if s.mode != mode {
                s.mode = mode;
                changed = true;
            }
        });
        if changed {
            self.emit(
                command,
                EventKind::ModeChanged {
                    mode,
                    reason: reason.to_owned(),
                },
            );
        }
    }

    fn set_session(&self, session: SessionState, command: Option<CommandId>) {
        let ready = session.is_ready();
        self.update(|s| s.session = session.clone());
        self.emit(command, EventKind::SessionChanged(session));
        if !ready && self.snapshot().mode == TradingMode::Armed {
            self.set_mode(
                TradingMode::DryRun,
                "the session is no longer ready",
                command,
            );
        }
    }

    // ---- broker access ----

    pub(crate) fn current_broker(&self) -> Option<Arc<dyn Broker>> {
        lock(&self.broker).clone()
    }

    /// The broker, if the session is `Ready`.
    pub(crate) fn ready_broker(&self) -> Result<Arc<dyn Broker>> {
        if self.shutdown.is_cancelled() {
            return Err(EngineError::ShuttingDown);
        }
        let session = self.snapshot().session.clone();
        match (session.is_ready(), self.current_broker()) {
            (true, Some(broker)) => Ok(broker),
            (false, _) if session == SessionState::Disconnected => Err(EngineError::NotConnected),
            (false, _) => Err(EngineError::NotReady {
                state: session.describe(),
            }),
            (true, None) => Err(EngineError::NotConnected),
        }
    }

    /// Runs a broker call under the configured request timeout.
    pub(crate) async fn io<T>(
        &self,
        operation: &'static str,
        call: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        tokio::time::timeout(self.config.session.request_timeout, call)
            .await
            .map_err(|_| EngineError::Timeout { operation })?
    }

    // ---- instruments and watch list ----

    /// The symbols this session can trade. Read once per connection, then served from memory.
    pub(crate) async fn catalog(&self) -> Result<Arc<Vec<SymbolInfo>>> {
        let broker = self.ready_broker()?;
        let generation = lock(&self.conn).generation;
        if let Some((cached, list)) = lock(&self.catalog).as_ref()
            && *cached == generation
        {
            return Ok(Arc::clone(list));
        }
        let list = Arc::new(self.io("the symbol list", broker.catalog()).await?);
        *lock(&self.catalog) = Some((generation, Arc::clone(&list)));
        Ok(list)
    }

    /// One quote, read now, without touching the watch list.
    pub(crate) async fn quote_of(&self, symbol: &str) -> Result<Option<Quote>> {
        let broker = self.ready_broker()?;
        let quotes = self
            .io("a quote", broker.quotes(&[symbol.to_owned()]))
            .await?;
        Ok(quotes.into_iter().next())
    }

    /// Bars of one symbol, read now. The whole span runs under one request timeout, so callers
    /// ask for a page at a time.
    pub(crate) async fn bars_of(
        &self,
        symbol: &str,
        period: Period,
        from: UnixMillis,
        to: UnixMillis,
    ) -> Result<Vec<Candle>> {
        let broker = self.ready_broker()?;
        self.io("the price history", broker.bars(symbol, period, from, to))
            .await
    }

    /// The details of one symbol, from the cache or the broker.
    pub(crate) async fn details_of(&self, symbol: &str) -> Result<Instrument> {
        let broker = self.ready_broker()?;
        self.instrument(&broker, symbol).await
    }

    pub(crate) fn cached_instrument(&self, symbol: &str) -> Option<Instrument> {
        lock(&self.instruments)
            .get(&symbol.to_ascii_uppercase())
            .cloned()
    }

    pub(crate) async fn instrument(
        &self,
        broker: &Arc<dyn Broker>,
        symbol: &str,
    ) -> Result<Instrument> {
        if let Some(found) = self.cached_instrument(symbol) {
            return Ok(found);
        }
        let instrument = self
            .io("instrument details", broker.instrument(symbol))
            .await?;
        lock(&self.instruments).insert(symbol.to_ascii_uppercase(), instrument.clone());
        self.update(|s| {
            s.instruments
                .insert(instrument.symbol.clone(), instrument.clone());
        });
        Ok(instrument)
    }

    pub(crate) fn set_watched(&self, symbols: impl IntoIterator<Item = String>) {
        let set: BTreeSet<String> = symbols
            .into_iter()
            .map(|s| s.trim().to_ascii_uppercase())
            .filter(|s| !s.is_empty())
            .collect();
        *lock(&self.watched) = set.clone();
        self.update(|s| s.watched = set.into_iter().collect());
    }

    /// Watched symbols plus the symbols of open positions.
    pub(crate) fn quote_symbols(&self) -> Vec<String> {
        let mut all: BTreeSet<String> = lock(&self.watched).clone();
        for p in &self.snapshot().positions {
            all.insert(p.symbol.to_ascii_uppercase());
        }
        all.into_iter().collect()
    }

    // ---- refresh ----

    pub(crate) async fn refresh_all(&self, broker: &Arc<dyn Broker>) -> Result<()> {
        let account = self.io("the account", broker.account()).await?;
        let positions = self.io("positions", broker.positions()).await?;
        let orders = self.io("pending orders", broker.pending_orders()).await?;
        for symbol in positions
            .iter()
            .map(|p| p.symbol.clone())
            .collect::<BTreeSet<_>>()
        {
            // Instruments for open positions, so sizing and display have their rules.
            if let Err(error) = self.instrument(broker, &symbol).await {
                tracing::warn!(%symbol, %error, "could not load an instrument for an open position");
            }
        }
        self.apply_refresh(account, positions, orders);
        Ok(())
    }

    fn apply_refresh(
        &self,
        account: AccountSnapshot,
        positions: Vec<Position>,
        orders: Vec<PendingOrder>,
    ) {
        let old = self.snapshot().positions.clone();
        let diff = diff_positions(&old, &positions);
        let account_id = account.account_id.clone();
        self.update(|s| {
            s.account_id = Some(account_id);
            s.account = Some(account);
            s.positions.clone_from(&positions);
            s.pending_orders = orders;
            s.last_error = None;
            s.last_refresh = Some(now_millis());
        });
        self.emit(None, EventKind::AccountUpdated);
        if !diff.is_empty() {
            self.emit(
                None,
                EventKind::PositionsChanged {
                    opened: diff.opened.clone(),
                    modified: diff.modified,
                    closed: diff.closed,
                },
            );
        }
        self.reconcile_unknown(&positions);
    }

    /// Re-reads account and positions right now, ignoring failures (the supervisor will
    /// record them on its own schedule). Used after an order to show the result quickly.
    pub(crate) async fn refresh_soon(&self) {
        if let Some(broker) = self.current_broker()
            && let Err(error) = self.refresh_all(&broker).await
        {
            tracing::debug!(%error, "post-order refresh failed");
        }
    }

    pub(crate) async fn refresh_quotes(&self, broker: &Arc<dyn Broker>) -> Result<()> {
        let symbols = self.quote_symbols();
        if symbols.is_empty() {
            return Ok(());
        }
        let quotes = self.io("quotes", broker.quotes(&symbols)).await?;
        self.update(|s| {
            for q in quotes {
                s.quotes.insert(q.symbol.clone(), q);
            }
        });
        Ok(())
    }

    // ---- connect, disconnect, supervise ----

    pub(crate) async fn connect(
        self: &Arc<Self>,
        request: ConnectRequest,
        command: CommandId,
    ) -> Result<()> {
        if self.shutdown.is_cancelled() {
            return Err(EngineError::ShuttingDown);
        }
        self.teardown("a new connection was requested", Some(command))
            .await;

        let token = self.shutdown.child_token();
        let generation = {
            let mut conn = lock(&self.conn);
            conn.generation += 1;
            conn.token = Some(token.clone());
            conn.generation
        };

        self.set_session(
            SessionState::Connecting {
                label: request.label.clone(),
            },
            Some(command),
        );
        let connect_timeout = self.config.session.request_timeout * 2;
        let connected =
            tokio::time::timeout(connect_timeout, self.connector.connect(&request)).await;
        let broker = match connected {
            Ok(Ok(broker)) => broker,
            Ok(Err(error)) => return Err(self.fail(generation, &error, command)),
            Err(_) => {
                let error = EngineError::Timeout {
                    operation: "the connection",
                };
                return Err(self.fail(generation, &error, command));
            }
        };

        if !self.is_current(generation) {
            let _ = broker.close().await;
            return Err(EngineError::Internal(
                "superseded by a newer connection".to_owned(),
            ));
        }
        *lock(&self.broker) = Some(Arc::clone(&broker));
        self.set_session(SessionState::Bootstrapping, Some(command));

        if let Err(error) = self.bootstrap(&broker).await {
            let _ = broker.close().await;
            *lock(&self.broker) = None;
            return Err(self.fail(generation, &error, command));
        }
        self.set_session(SessionState::Ready, Some(command));

        let me = Arc::clone(self);
        self.tracker
            .spawn(async move { me.supervise(generation, token, request).await });
        Ok(())
    }

    /// Loads the account, positions and the state a front end needs to start.
    async fn bootstrap(&self, broker: &Arc<dyn Broker>) -> Result<()> {
        self.update(|s| {
            s.service = Some(broker.service());
            s.account_id = Some(broker.account_id().clone());
        });
        // Check the computer clock against the broker's clock when available.
        match self.io("the server time", broker.server_time()).await {
            Ok(server) => {
                let offset = server - now_millis();
                if offset.abs() > 5_000 {
                    self.raise_warning(
                        "clock-skew".to_owned(),
                        WarningKind::Data,
                        format!(
                            "this computer's clock differs from the broker's by {} s",
                            offset / 1000
                        ),
                    );
                }
            }
            Err(error) => {
                tracing::warn!(%error, "no server time; using the local clock");
            }
        }
        self.refresh_all(broker).await?;
        // Best effort: a failed first quote read is retried by the supervisor.
        if let Err(error) = self.refresh_quotes(broker).await {
            tracing::debug!(%error, "initial quote refresh failed");
        }
        Ok(())
    }

    fn fail(&self, generation: u64, error: &EngineError, command: CommandId) -> EngineError {
        tracing::warn!(%error, "connection failed");
        if self.is_current(generation) {
            self.set_session(
                SessionState::Failed {
                    reason: error.to_string(),
                },
                Some(command),
            );
        }
        error.clone()
    }

    fn is_current(&self, generation: u64) -> bool {
        lock(&self.conn).generation == generation
    }

    /// Stops the supervisor, closes the broker and clears session data.
    pub(crate) async fn teardown(&self, reason: &str, command: Option<CommandId>) {
        let token = {
            let mut conn = lock(&self.conn);
            conn.generation += 1;
            conn.token.take()
        };
        if let Some(token) = token {
            token.cancel();
        }
        let broker = lock(&self.broker).take();
        if let Some(broker) = broker
            && let Err(error) = broker.close().await
        {
            tracing::debug!(%error, "closing the broker session failed");
        }
        let had_session = self.snapshot().session != SessionState::Disconnected;
        if had_session {
            self.update(|s| {
                s.account = None;
                s.positions.clear();
                s.pending_orders.clear();
                s.quotes.clear();
                s.account_id = None;
                s.service = None;
            });
            self.set_session(SessionState::Disconnected, command);
        }
        if self.snapshot().mode == TradingMode::Armed {
            self.set_mode(TradingMode::DryRun, reason, command);
        }
        lock(&self.plans).clear();
        let mut gate = lock(&self.gate);
        gate.flatten = None;
    }

    async fn supervise(
        self: Arc<Self>,
        generation: u64,
        token: CancellationToken,
        request: ConnectRequest,
    ) {
        let cfg = self.config.session.clone();
        let start = |period: Duration| {
            let mut i = tokio::time::interval_at(Instant::now() + period, period);
            i.set_missed_tick_behavior(MissedTickBehavior::Delay);
            i
        };
        let mut refresh = start(cfg.refresh_interval);
        let mut quotes = start(cfg.quote_interval);
        let mut ping = start(cfg.ping_interval);
        // Counted separately: a healthy ping must not hide a data refresh that keeps failing.
        let mut refresh_failures = 0u32;
        let mut ping_failures = 0u32;

        loop {
            let check = tokio::select! {
                () = token.cancelled() => break,
                _ = refresh.tick() => Check::Refresh,
                _ = quotes.tick() => Check::Quotes,
                _ = ping.tick() => Check::Ping,
            };
            if !self.is_current(generation) {
                break;
            }
            let Some(broker) = self.current_broker() else {
                break;
            };
            let (outcome, counter) = match check {
                Check::Refresh => (self.refresh_all(&broker).await, &mut refresh_failures),
                Check::Quotes => {
                    if let Err(error) = self.refresh_quotes(&broker).await {
                        tracing::debug!(%error, "quote refresh failed");
                    }
                    continue;
                }
                Check::Ping => (self.io("the ping", broker.ping()).await, &mut ping_failures),
            };
            match outcome {
                Ok(()) => *counter = 0,
                Err(error) => {
                    *counter += 1;
                    let failures = *counter;
                    tracing::warn!(%error, failures, "refresh failed; keeping the last data");
                    self.update(|s| s.last_error = Some(error.to_string()));
                    self.emit(
                        None,
                        EventKind::RefreshFailed {
                            message: error.to_string(),
                        },
                    );
                    if failures >= cfg.max_refresh_failures {
                        if !self.reconnect(generation, &token, &request).await {
                            break;
                        }
                        refresh_failures = 0;
                        ping_failures = 0;
                        refresh.reset();
                        quotes.reset();
                        ping.reset();
                    }
                }
            }
        }
    }

    /// Reconnects with backoff. Returns `false` when the session was cancelled or the
    /// attempts ran out (the state is `Failed` in the latter case).
    async fn reconnect(
        &self,
        generation: u64,
        token: &CancellationToken,
        request: &ConnectRequest,
    ) -> bool {
        let cfg = &self.config.session;
        // Drop the broken session first; the mode falls back to dry-run.
        let old = lock(&self.broker).take();
        if let Some(old) = old {
            let _ = old.close().await;
        }
        let mut delay = cfg.reconnect_initial;
        for attempt in 1..=cfg.reconnect_attempts {
            if !self.is_current(generation) {
                return false;
            }
            self.set_session(SessionState::Reconnecting { attempt }, None);
            tokio::select! {
                () = token.cancelled() => return false,
                () = tokio::time::sleep(delay) => {}
            }
            let connect =
                tokio::time::timeout(cfg.request_timeout * 2, self.connector.connect(request))
                    .await;
            match connect {
                Ok(Ok(broker)) => {
                    if !self.is_current(generation) {
                        let _ = broker.close().await;
                        return false;
                    }
                    *lock(&self.broker) = Some(Arc::clone(&broker));
                    match self.bootstrap(&broker).await {
                        Ok(()) => {
                            self.set_session(SessionState::Ready, None);
                            return true;
                        }
                        Err(error) => {
                            tracing::warn!(%error, attempt, "reconnected but bootstrap failed");
                            *lock(&self.broker) = None;
                            let _ = broker.close().await;
                        }
                    }
                }
                Ok(Err(error)) => tracing::warn!(%error, attempt, "reconnect attempt failed"),
                Err(_) => tracing::warn!(attempt, "reconnect attempt timed out"),
            }
            delay = (delay * 2).min(cfg.reconnect_max);
        }
        if self.is_current(generation) {
            self.set_session(
                SessionState::Failed {
                    reason: format!(
                        "could not reconnect after {} attempts",
                        cfg.reconnect_attempts
                    ),
                },
                None,
            );
        }
        false
    }

    // ---- shutdown ----

    pub(crate) async fn shutdown(&self) {
        self.teardown("the engine is shutting down", None).await;
        self.shutdown.cancel();
        self.tracker.close();
        self.tracker.wait().await;
    }

    // ---- reconciliation of orders whose outcome was unknown ----

    fn reconcile_unknown(&self, positions: &[Position]) {
        let matched: Vec<(UnknownOrder, PositionId)> = {
            let mut gate = lock(&self.gate);
            let mut matched = Vec::new();
            // A position already claimed by an earlier order in this pass is not offered to
            // the next one.
            let mut claimed: Vec<PositionId> = Vec::new();
            gate.unknown.retain(|u| {
                let by_label = positions
                    .iter()
                    .find(|p| p.label.as_deref() == Some(u.label.as_str()));
                // Remote never echoes the label, so fall back to what the order was: a
                // position that did not exist when it was sent, with its symbol, side and
                // volume. This can clear the warning for a look-alike opened by hand, which
                // is harmless: the exposure is on the account either way.
                let by_shape = || {
                    positions.iter().find(|p| {
                        !u.before.contains(&p.id)
                            && !claimed.contains(&p.id)
                            && p.label.is_none()
                            && p.symbol.eq_ignore_ascii_case(&u.symbol)
                            && p.side == u.side
                            && p.volume == u.volume
                    })
                };
                if let Some(p) = by_label.or_else(by_shape) {
                    claimed.push(p.id);
                    matched.push((u.clone(), p.id));
                    false
                } else {
                    true
                }
            });
            matched
        };
        for (order, position) in matched {
            self.clear_warning(&format!("unknown-order:{}", order.label));
            self.emit(
                None,
                EventKind::Reconciled {
                    label: order.label,
                    position,
                },
            );
        }
    }

    /// The account the session is bound to.
    pub(crate) fn account_id(&self) -> Option<AccountId> {
        self.snapshot().account_id.clone()
    }
}

enum Check {
    Refresh,
    Quotes,
    Ping,
}

/// How positions changed between two reads.
pub(crate) struct PositionDiff {
    pub(crate) opened: Vec<Position>,
    pub(crate) modified: Vec<Position>,
    pub(crate) closed: Vec<PositionId>,
}

impl PositionDiff {
    fn is_empty(&self) -> bool {
        self.opened.is_empty() && self.modified.is_empty() && self.closed.is_empty()
    }
}

/// Compares two position lists. A position counts as modified when its volume or its
/// protective levels change; P&L drift alone is not a modification.
pub(crate) fn diff_positions(old: &[Position], new: &[Position]) -> PositionDiff {
    let opened = new
        .iter()
        .filter(|n| !old.iter().any(|o| o.id == n.id))
        .cloned()
        .collect();
    let modified = new
        .iter()
        .filter(|n| {
            old.iter().any(|o| {
                o.id == n.id
                    && (o.volume != n.volume
                        || o.stop_loss != n.stop_loss
                        || o.take_profit != n.take_profit)
            })
        })
        .cloned()
        .collect();
    let closed = old
        .iter()
        .filter(|o| !new.iter().any(|n| n.id == o.id))
        .map(|o| o.id)
        .collect();
    PositionDiff {
        opened,
        modified,
        closed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Side, Volume};

    fn pos(id: i64, volume: i64, sl: Option<f64>) -> Position {
        Position {
            id: PositionId(id),
            symbol: "EURUSD".into(),
            side: Side::Buy,
            volume: Volume::from_units(volume),
            entry_price: Some(1.0),
            stop_loss: sl,
            take_profit: None,
            swap: None,
            commission: None,
            unrealized_pnl: Some(0.0),
            label: None,
        }
    }

    #[test]
    fn diff_finds_opened_modified_and_closed() {
        let old = vec![pos(1, 1000, None), pos(2, 1000, None), pos(3, 1000, None)];
        let mut moved = pos(2, 1000, None);
        moved.unrealized_pnl = Some(55.0);
        let new = vec![moved, pos(3, 500, None), pos(4, 1000, Some(0.9))];
        let d = diff_positions(&old, &new);
        assert_eq!(d.opened.iter().map(|p| p.id.get()).collect::<Vec<_>>(), [4]);
        assert_eq!(
            d.modified.iter().map(|p| p.id.get()).collect::<Vec<_>>(),
            [3],
            "P&L drift alone is not a modification"
        );
        assert_eq!(d.closed, [PositionId(1)]);
    }

    #[test]
    fn identical_reads_produce_an_empty_diff() {
        let a = vec![pos(1, 1000, Some(0.9))];
        assert!(diff_positions(&a, &a).is_empty());
    }
}

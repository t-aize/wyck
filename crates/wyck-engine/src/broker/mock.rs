//! A scriptable in-memory [`Broker`] for tests (behind the `testing` feature).
//!
//! It behaves like a small, well-mannered exchange: market orders fill at the current quote
//! and open a position carrying the order's label, protection changes and closes update or
//! remove that position. On top of that, tests can script trouble:
//!
//! - [`MockBroker::fail_next`] makes the next call of a kind return an error
//! - [`MockBroker::drop_reply_next`] performs a mutating call but reports a timeout, the
//!   exact "did it or didn't it" situation the order pipeline must survive
//! - [`MockBroker::set_delay`] adds latency to a call kind (pair with paused Tokio time)
//! - [`MockBroker::calls`] records every call, so a test can assert that nothing mutating
//!   was sent
//!
//! ```
//! use wyck_engine::broker::{Broker, MockBroker};
//!
//! # tokio_test_block(async {
//! let broker = MockBroker::new();
//! assert!(broker.positions().await.unwrap().is_empty());
//! # });
//! # fn tokio_test_block<F: std::future::Future>(f: F) {
//! #     tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f);
//! # }
//! ```

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;

use super::{Broker, BrokerCall, MarketOrder, PlacedOrder, ServiceKind};
use crate::domain::{
    AccountKind, AccountSnapshot, Instrument, PendingOrder, Position, Quote, SpecsSource,
    UnixMillis, Volume, VolumeSpecs, now_millis,
};
use crate::error::{EngineError, Result};
use crate::ids::{AccountId, OrderId, PositionId};

/// The kind of call a failure or delay applies to. Same type as [`BrokerCall`].
pub type MockOp = BrokerCall;

#[derive(Debug)]
struct State {
    account: AccountSnapshot,
    instruments: Vec<Instrument>,
    positions: Vec<Position>,
    orders: Vec<PendingOrder>,
    quotes: HashMap<String, Quote>,
    next_position_id: i64,
    failures: HashMap<BrokerCall, VecDeque<EngineError>>,
    dropped_replies: HashMap<BrokerCall, u32>,
    delays: HashMap<BrokerCall, Duration>,
    calls: Vec<BrokerCall>,
    closed: bool,
}

/// A scriptable in-memory broker: market orders fill at the current quote, and failures,
/// lost replies and latency can be injected per call kind. See [`MockBroker::fail_next`],
/// [`MockBroker::drop_reply_next`], [`MockBroker::set_delay`] and [`MockBroker::calls`].
#[derive(Debug)]
pub struct MockBroker {
    account_id: AccountId,
    can_trade: bool,
    service: ServiceKind,
    state: Mutex<State>,
}

impl Default for MockBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBroker {
    /// A demo account with 10,000 USD, an `EURUSD` instrument (0.01 lot step) and a
    /// 1.08499 / 1.08501 quote.
    #[must_use]
    pub fn new() -> Self {
        let account_id = AccountId::new("mock-1");
        let eurusd = Instrument {
            symbol: "EURUSD".to_owned(),
            symbol_id: Some(1),
            price_digits: 5,
            pip_size: 0.0001,
            base_currency: Some("EUR".to_owned()),
            quote_currency: Some("USD".to_owned()),
            enabled: true,
            volume: VolumeSpecs {
                lot_size: 100_000.0,
                min: Volume::from_units(1_000),
                step: Volume::from_units(1_000),
                max: Some(Volume::from_units(100_000_000)),
            },
            specs_source: SpecsSource::Broker,
        };
        let mut quotes = HashMap::new();
        quotes.insert(
            "EURUSD".to_owned(),
            Quote {
                symbol: "EURUSD".to_owned(),
                bid: 1.08499,
                ask: 1.08501,
                timestamp: Some(now_millis()),
            },
        );
        Self {
            can_trade: true,
            service: ServiceKind::CtraderRemote,
            state: Mutex::new(State {
                account: AccountSnapshot {
                    account_id: account_id.clone(),
                    kind: AccountKind::Demo,
                    currency: Some("USD".to_owned()),
                    balance: Some(10_000.0),
                    equity: Some(10_000.0),
                    free_margin: Some(10_000.0),
                    used_margin: Some(0.0),
                    margin_level_pct: None,
                    server_version: Some("mock".to_owned()),
                    captured_at: now_millis(),
                },
                instruments: vec![eurusd],
                positions: Vec::new(),
                orders: Vec::new(),
                quotes,
                next_position_id: 1,
                failures: HashMap::new(),
                dropped_replies: HashMap::new(),
                delays: HashMap::new(),
                calls: Vec::new(),
                closed: false,
            }),
            account_id,
        }
    }

    /// Makes this connection read-only ([`Broker::can_trade`] returns `false`).
    #[must_use]
    pub fn read_only(mut self) -> Self {
        self.can_trade = false;
        self
    }

    /// Reports a different service family.
    #[must_use]
    pub fn with_service(mut self, service: ServiceKind) -> Self {
        self.service = service;
        self
    }

    /// Replaces the account snapshot's balance, equity and free margin.
    #[must_use]
    pub fn with_balance(self, balance: f64) -> Self {
        {
            let mut s = self.lock();
            s.account.balance = Some(balance);
            s.account.equity = Some(balance);
            s.account.free_margin = Some(balance);
        }
        self
    }

    /// Adds (or replaces) an instrument.
    #[must_use]
    pub fn with_instrument(self, instrument: Instrument) -> Self {
        {
            let mut s = self.lock();
            s.instruments.retain(|i| i.symbol != instrument.symbol);
            s.instruments.push(instrument);
        }
        self
    }

    /// Sets a quote.
    #[must_use]
    pub fn with_quote(self, symbol: &str, bid: f64, ask: f64) -> Self {
        self.set_quote(symbol, bid, ask);
        self
    }

    /// Adds an already-open position.
    #[must_use]
    pub fn with_position(self, position: Position) -> Self {
        {
            let mut s = self.lock();
            s.next_position_id = s.next_position_id.max(position.id.get() + 1);
            s.positions.push(position);
        }
        self
    }

    /// Adds an open position after construction (an order filled elsewhere, for example).
    pub fn push_position(&self, position: Position) {
        let mut s = self.lock();
        s.next_position_id = s.next_position_id.max(position.id.get() + 1);
        s.positions.push(position);
    }

    /// Updates a quote after construction.
    pub fn set_quote(&self, symbol: &str, bid: f64, ask: f64) {
        self.lock().quotes.insert(
            symbol.to_owned(),
            Quote {
                symbol: symbol.to_owned(),
                bid,
                ask,
                timestamp: Some(now_millis()),
            },
        );
    }

    /// Makes the next call of kind `op` fail with `error`. Queues: call it twice to fail
    /// two calls in a row.
    pub fn fail_next(&self, op: BrokerCall, error: EngineError) {
        self.lock().failures.entry(op).or_default().push_back(error);
    }

    /// Makes the next `count` calls of kind `op` perform their effect and then report
    /// [`EngineError::Timeout`]: the request reached the server, the reply was lost.
    pub fn drop_reply_next(&self, op: BrokerCall, count: u32) {
        *self.lock().dropped_replies.entry(op).or_default() += count;
    }

    /// Adds `delay` of latency to every call of kind `op`.
    pub fn set_delay(&self, op: BrokerCall, delay: Duration) {
        self.lock().delays.insert(op, delay);
    }

    /// Every call made so far, in order.
    #[must_use]
    pub fn calls(&self) -> Vec<BrokerCall> {
        self.lock().calls.clone()
    }

    /// How many calls of kind `op` were made.
    #[must_use]
    pub fn call_count(&self, op: BrokerCall) -> usize {
        self.lock().calls.iter().filter(|c| **c == op).count()
    }

    /// Whether [`Broker::close`] was called.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.lock().closed
    }

    /// The current positions, without going through the (recorded) trait method.
    #[must_use]
    pub fn peek_positions(&self) -> Vec<Position> {
        self.lock().positions.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Records the call, applies any scripted delay, and pops a scripted failure.
    async fn enter(&self, op: BrokerCall) -> Result<()> {
        let (delay, failure) = {
            let mut s = self.lock();
            s.calls.push(op);
            (
                s.delays.get(&op).copied(),
                s.failures.get_mut(&op).and_then(VecDeque::pop_front),
            )
        };
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
        failure.map_or(Ok(()), Err)
    }

    /// Whether this call's reply should be dropped (after performing it).
    fn take_dropped_reply(&self, op: BrokerCall) -> bool {
        let mut s = self.lock();
        match s.dropped_replies.get_mut(&op) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        }
    }
}

const LOST_REPLY: EngineError = EngineError::Timeout {
    operation: "the broker reply",
};

#[async_trait]
impl Broker for MockBroker {
    fn service(&self) -> ServiceKind {
        self.service
    }

    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn can_trade(&self) -> bool {
        self.can_trade
    }

    fn stop_granularity(&self, instrument: &Instrument) -> f64 {
        instrument.pip_size / 10.0
    }

    fn label_prefix(&self) -> String {
        "mock".to_owned()
    }

    async fn account(&self) -> Result<AccountSnapshot> {
        self.enter(BrokerCall::Account).await?;
        let mut snapshot = self.lock().account.clone();
        snapshot.captured_at = now_millis();
        Ok(snapshot)
    }

    async fn symbols(&self) -> Result<Vec<String>> {
        self.enter(BrokerCall::Symbols).await?;
        Ok(self
            .lock()
            .instruments
            .iter()
            .map(|i| i.symbol.clone())
            .collect())
    }

    async fn instrument(&self, symbol: &str) -> Result<Instrument> {
        self.enter(BrokerCall::Instrument).await?;
        self.lock()
            .instruments
            .iter()
            .find(|i| i.symbol.eq_ignore_ascii_case(symbol))
            .cloned()
            .ok_or_else(|| EngineError::Invalid(format!("unknown symbol `{symbol}`")))
    }

    async fn positions(&self) -> Result<Vec<Position>> {
        self.enter(BrokerCall::Positions).await?;
        Ok(self.lock().positions.clone())
    }

    async fn pending_orders(&self) -> Result<Vec<PendingOrder>> {
        self.enter(BrokerCall::PendingOrders).await?;
        Ok(self.lock().orders.clone())
    }

    async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>> {
        self.enter(BrokerCall::Quotes).await?;
        let s = self.lock();
        Ok(symbols
            .iter()
            .filter_map(|sym| s.quotes.get(sym).cloned())
            .collect())
    }

    async fn server_time(&self) -> Result<UnixMillis> {
        self.enter(BrokerCall::ServerTime).await?;
        Ok(now_millis())
    }

    async fn ping(&self) -> Result<()> {
        self.enter(BrokerCall::Ping).await
    }

    async fn place_market(&self, order: &MarketOrder) -> Result<PlacedOrder> {
        self.enter(BrokerCall::PlaceMarket).await?;
        let placed = {
            let mut s = self.lock();
            let instrument = s
                .instruments
                .iter()
                .find(|i| i.symbol == order.symbol)
                .cloned()
                .ok_or_else(|| EngineError::Invalid(format!("unknown symbol {}", order.symbol)))?;
            let quote =
                s.quotes.get(&order.symbol).cloned().ok_or_else(|| {
                    EngineError::Invalid(format!("no quote for {}", order.symbol))
                })?;
            let entry = quote.price_for(order.side);
            let sign = match order.side {
                crate::domain::Side::Buy => 1.0,
                crate::domain::Side::Sell => -1.0,
            };
            let id = PositionId(s.next_position_id);
            s.next_position_id += 1;
            s.positions.push(Position {
                id,
                symbol: order.symbol.clone(),
                side: order.side,
                volume: order.volume,
                entry_price: Some(entry),
                stop_loss: order
                    .stop_loss_distance
                    .map(|d| instrument.round_price(entry - sign * d)),
                take_profit: order
                    .take_profit_distance
                    .map(|d| instrument.round_price(entry + sign * d)),
                swap: Some(0.0),
                commission: Some(0.0),
                unrealized_pnl: Some(0.0),
                label: Some(order.label.clone()),
            });
            PlacedOrder {
                position_id: Some(id),
                order_id: None,
            }
        };
        if self.take_dropped_reply(BrokerCall::PlaceMarket) {
            return Err(LOST_REPLY);
        }
        Ok(placed)
    }

    async fn set_protection(
        &self,
        position: &Position,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> Result<()> {
        self.enter(BrokerCall::SetProtection).await?;
        {
            let mut s = self.lock();
            let target = s
                .positions
                .iter_mut()
                .find(|p| p.id == position.id)
                .ok_or_else(|| EngineError::Invalid(format!("no position {}", position.id)))?;
            if stop_loss.is_some() {
                target.stop_loss = stop_loss;
            }
            if take_profit.is_some() {
                target.take_profit = take_profit;
            }
        }
        if self.take_dropped_reply(BrokerCall::SetProtection) {
            return Err(LOST_REPLY);
        }
        Ok(())
    }

    async fn close_position(&self, position: &Position, volume: Option<Volume>) -> Result<()> {
        self.enter(BrokerCall::ClosePosition).await?;
        {
            let mut s = self.lock();
            let index = s
                .positions
                .iter()
                .position(|p| p.id == position.id)
                .ok_or_else(|| EngineError::Invalid(format!("no position {}", position.id)))?;
            match volume {
                Some(v) if v < s.positions[index].volume => {
                    s.positions[index].volume = s.positions[index].volume - v
                }
                _ => {
                    s.positions.remove(index);
                }
            }
        }
        if self.take_dropped_reply(BrokerCall::ClosePosition) {
            return Err(LOST_REPLY);
        }
        Ok(())
    }

    async fn cancel_order(&self, order: OrderId) -> Result<()> {
        self.enter(BrokerCall::CancelOrder).await?;
        let removed = {
            let mut s = self.lock();
            let before = s.orders.len();
            s.orders.retain(|o| o.id != order);
            s.orders.len() != before
        };
        if !removed {
            return Err(EngineError::Invalid(format!("no order {order}")));
        }
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        self.lock().closed = true;
        Ok(())
    }
}

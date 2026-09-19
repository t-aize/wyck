//! The trading pipeline: plan, gate, send, confirm, report.
//!
//! # Safety model
//!
//! 1. **Dry-run by default.** The engine starts disarmed. A plan can always be made and
//!    submitted, but while disarmed submission returns [`OrderOutcome::DryRun`] and sends
//!    nothing. Arming needs a ready session, a trading-capable connection, and an
//!    acknowledgement of the account kind (so a live account cannot be armed by a front end
//!    that thinks it is demo). Leaving `Ready` disarms.
//! 2. **Plans are single use and short lived.** Only a plan the engine created, that has not
//!    expired and has not been submitted before, can be sent. A plan is priced from a live
//!    quote; an old one must be re-planned.
//! 3. **One order in flight per symbol, and a minimum interval between orders**, so a
//!    held-down or double-tapped hotkey cannot double the position.
//! 4. **A mutating call is attempted exactly once.** If the reply is lost, the order is not
//!    replayed. The engine instead reads positions back, matching by the order's label, and
//!    reports the truth. If it still cannot tell, the outcome is [`OrderOutcome::Unknown`],
//!    a persistent warning is raised, and later refreshes reconcile it automatically.
//! 5. **Nothing here can be blocked by a guardrail.** Guardrails only add warnings.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::Instant;

use crate::broker::{Broker, MarketOrder};
use crate::core::{Inner, StoredPlan, lock};
use crate::domain::{AccountKind, PendingOrder, Position, Side, UnixMillis, Volume, now_millis};
use crate::error::{BrokerErrorKind, EngineError, Result};
use crate::event::EventKind;
use crate::guardrails::{self, estimate_open_risk};
use crate::ids::{AccountId, CommandId, OrderId, PositionId};
use crate::risk::{EntryIntent, OrderPlan, PlanContext, PlanId, build_plan};
use crate::state::{TradingMode, WarningKind};

/// The essentials of a plan, for events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanSummary {
    /// The plan.
    pub plan: PlanId,
    /// The ticker.
    pub symbol: String,
    /// Buy or sell.
    pub side: Side,
    /// The volume.
    pub volume: Volume,
    /// Stop loss price.
    pub stop_loss_price: Option<f64>,
    /// Take profit price.
    pub take_profit_price: Option<f64>,
    /// Loss at the stop in account currency.
    pub risk_amount: Option<f64>,
    /// Loss at the stop as a percentage of balance.
    pub risk_percent: Option<f64>,
    /// Notes for the user.
    pub warnings: Vec<String>,
}

impl From<&OrderPlan> for PlanSummary {
    fn from(p: &OrderPlan) -> Self {
        Self {
            plan: p.id,
            symbol: p.symbol.clone(),
            side: p.side,
            volume: p.volume,
            stop_loss_price: p.stop_loss_price,
            take_profit_price: p.take_profit_price,
            risk_amount: p.risk_amount,
            risk_percent: p.risk_percent,
            warnings: p.warnings.clone(),
        }
    }
}

/// How an order ended, or what is known about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OrderOutcome {
    /// The order filled in full; here is the position.
    Filled {
        /// The plan.
        plan: PlanId,
        /// The resulting position, as read back from the broker.
        position: Position,
    },
    /// The position exists but is smaller than requested.
    PartiallyFilled {
        /// The plan.
        plan: PlanId,
        /// The resulting position.
        position: Position,
        /// What was asked for.
        requested: Volume,
    },
    /// The broker refused the order, or it was never sent.
    Rejected {
        /// The plan.
        plan: PlanId,
        /// Why.
        reason: String,
    },
    /// The engine is disarmed: the order was fully prepared and **not** sent.
    DryRun {
        /// What would have been sent.
        would_send: PlanSummary,
    },
    /// The order may or may not have been executed. It is tracked, and a warning stays up
    /// until a refresh finds the matching position (`Reconciled`) or the user dismisses it.
    Unknown {
        /// The plan.
        plan: PlanId,
        /// The label the order carried, to find it by.
        label: String,
        /// Why the outcome is unknown.
        reason: String,
    },
}

/// An order whose outcome is not known yet.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UnknownOrder {
    pub(crate) label: String,
    /// What the order was, for the servers that do not echo the label on positions (Remote
    /// does not): a new position with the same symbol, side and volume is taken to be it.
    pub(crate) symbol: String,
    pub(crate) side: crate::domain::Side,
    pub(crate) volume: crate::domain::Volume,
    /// The positions that existed when the order was sent, none of which can be it.
    pub(crate) before: Vec<PositionId>,
}

/// Asks to enable real order sending.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmRequest {
    /// The account being armed. Must be the connected one.
    pub account: AccountId,
    /// The account kind the caller believes it is arming. Must equal what the engine sees,
    /// including `Unknown`: a front end has to acknowledge it is about to trade a live
    /// account, or an account of unknown kind.
    pub acknowledged_kind: AccountKind,
}

/// How much of a position to close.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CloseSize {
    /// All of it.
    Full,
    /// This much, which must be a valid volume smaller than the open one.
    Volume(Volume),
}

/// What a flatten covers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FlattenScope {
    /// Every position and working order.
    All,
    /// Only this symbol.
    Symbol(String),
}

/// What a flatten would do, and the single-use token that confirms it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlattenPreview {
    /// Pass this to [`EngineHandle::flatten`](crate::EngineHandle::flatten).
    pub token: String,
    /// Positions that would be closed.
    pub positions: Vec<Position>,
    /// Working orders that would be cancelled.
    pub orders: Vec<PendingOrder>,
    /// When the token stops working, in Unix milliseconds.
    pub expires_at: UnixMillis,
}

/// What a flatten did. Failures are per item: one stuck position does not stop the rest.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FlattenReport {
    /// Positions closed.
    pub closed: Vec<PositionId>,
    /// Orders cancelled.
    pub cancelled: Vec<OrderId>,
    /// One message per item that failed.
    pub errors: Vec<String>,
}

impl FlattenReport {
    /// Whether everything succeeded.
    #[must_use]
    pub fn fully_flattened(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Errors after which an order is certain not to have reached the broker.
fn certainly_not_sent(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError::Invalid(_)
            | EngineError::TradingUnavailable(_)
            | EngineError::NotReady { .. }
            | EngineError::NotConnected
            | EngineError::Broker {
                kind: BrokerErrorKind::Rejected,
                ..
            }
    )
}

/// Marks a symbol as having an order in flight, and clears it when dropped, whatever
/// happens in between.
struct InFlight<'a> {
    inner: &'a Inner,
    symbol: String,
}

impl<'a> InFlight<'a> {
    fn acquire(inner: &'a Inner, symbol: &str) -> Result<Self> {
        let key = symbol.to_ascii_uppercase();
        {
            let mut gate = lock(&inner.gate);
            if gate.in_flight.contains(&key) {
                return Err(EngineError::Busy(format!(
                    "an order for {symbol} is already in flight"
                )));
            }
            if let Some(last) = gate.last_order_at.get(&key)
                && last.elapsed() < inner.config.trading.min_order_interval
            {
                return Err(EngineError::Busy(format!(
                    "the last order for {symbol} was sent {} ms ago; wait {} ms between orders",
                    last.elapsed().as_millis(),
                    inner.config.trading.min_order_interval.as_millis()
                )));
            }
            gate.in_flight.insert(key.clone());
            gate.last_order_at.insert(key.clone(), Instant::now());
        }
        inner.sync_in_flight();
        Ok(Self { inner, symbol: key })
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        lock(&self.inner.gate).in_flight.remove(&self.symbol);
        self.inner.sync_in_flight();
    }
}

impl Inner {
    fn sync_in_flight(&self) {
        let symbols: Vec<String> = lock(&self.gate).in_flight.iter().cloned().collect();
        self.update(|s| s.orders_in_flight = symbols);
    }

    // ---- arming ----

    pub(crate) fn arm(&self, request: &ArmRequest, command: CommandId) -> Result<()> {
        let broker = self
            .ready_broker()
            .map_err(|e| EngineError::ArmRefused(e.to_string()))?;
        let state = self.snapshot();
        if state.account_id.as_ref() != Some(&request.account) {
            return Err(EngineError::ArmRefused(format!(
                "the connected account is {}, not {}",
                state
                    .account_id
                    .as_ref()
                    .map_or("unknown".to_owned(), ToString::to_string),
                request.account
            )));
        }
        if !broker.can_trade() {
            return Err(EngineError::ArmRefused(
                "this connection is read-only".to_owned(),
            ));
        }
        let actual = state
            .account
            .as_ref()
            .map_or(AccountKind::Unknown, |a| a.kind);
        if request.acknowledged_kind != actual {
            return Err(EngineError::ArmRefused(format!(
                "the account is {actual:?} but {:?} was acknowledged",
                request.acknowledged_kind
            )));
        }
        self.set_mode(TradingMode::Armed, "armed by request", Some(command));
        Ok(())
    }

    pub(crate) fn disarm(&self, command: CommandId) {
        self.set_mode(TradingMode::DryRun, "disarmed by request", Some(command));
    }

    /// The broker, if the engine is ready **and armed** and the connection can trade.
    fn armed_broker(&self) -> Result<Arc<dyn Broker>> {
        let broker = self.ready_broker()?;
        if self.snapshot().mode != TradingMode::Armed {
            return Err(EngineError::NotArmed);
        }
        if !broker.can_trade() {
            return Err(EngineError::TradingUnavailable(
                "this connection is read-only".to_owned(),
            ));
        }
        Ok(broker)
    }

    // ---- planning ----

    /// The account-currency value of one unit of `currency`, from live quotes.
    async fn conversion_rate(
        &self,
        broker: &Arc<dyn Broker>,
        currency: Option<&str>,
        account_currency: Option<&str>,
    ) -> Result<Option<f64>> {
        let (Some(from), Some(to)) = (currency, account_currency) else {
            return Ok(None);
        };
        if from.eq_ignore_ascii_case(to) {
            return Ok(Some(1.0));
        }
        let direct = format!("{from}{to}").to_ascii_uppercase();
        let inverse = format!("{to}{from}").to_ascii_uppercase();
        let quotes = self
            .io(
                "conversion quotes",
                broker.quotes(&[direct.clone(), inverse.clone()]),
            )
            .await?;
        for q in quotes.iter().filter(|q| q.is_valid()) {
            if q.symbol.eq_ignore_ascii_case(&direct) {
                return Ok(Some(q.mid()));
            }
            if q.symbol.eq_ignore_ascii_case(&inverse) {
                return Ok(Some(1.0 / q.mid()));
            }
        }
        Ok(None)
    }

    pub(crate) async fn plan_entry(
        &self,
        intent: EntryIntent,
        command: CommandId,
    ) -> Result<OrderPlan> {
        let broker = self.ready_broker()?;
        let instrument = self.instrument(&broker, &intent.symbol).await?;
        let quote = self
            .io(
                "the quote",
                broker.quotes(std::slice::from_ref(&instrument.symbol)),
            )
            .await?
            .into_iter()
            .find(|q| q.symbol.eq_ignore_ascii_case(&instrument.symbol))
            .ok_or_else(|| {
                EngineError::Invalid(format!("no quote is available for {}", instrument.symbol))
            })?;

        // A fresh account read: sizing on a stale balance would be wrong.
        let account = self.io("the account", broker.account()).await?;
        let rate = self
            .conversion_rate(
                &broker,
                instrument.quote_currency.as_deref(),
                account.currency.as_deref(),
            )
            .await?;

        let mut plan = build_plan(
            &intent,
            &PlanContext {
                account: broker.account_id(),
                account_currency: account.currency.as_deref(),
                instrument: &instrument,
                quote: &quote,
                balance: account.balance,
                conversion_rate: rate,
                stop_granularity: broker.stop_granularity(&instrument),
            },
        )?;

        // Guardrails: sentences only.
        let state = self.snapshot();
        let open_risk = estimate_open_risk(
            &state.positions,
            |s| {
                state
                    .instruments
                    .get(&s.to_ascii_uppercase())
                    .or_else(|| state.instruments.get(s))
            },
            account.currency.as_deref(),
        );
        let currencies: Vec<String> = [&instrument.base_currency, &instrument.quote_currency]
            .into_iter()
            .flatten()
            .map(|c| c.to_ascii_uppercase())
            .collect();
        let news: Vec<&crate::state::Warning> = state
            .warnings
            .iter()
            .filter(|w| guardrails::is_news(w))
            .filter(|w| {
                currencies.is_empty() || currencies.iter().any(|c| w.message.contains(c.as_str()))
            })
            .collect();
        plan.warnings.extend(guardrails::plan_warnings(
            &plan,
            &self.config.guardrails,
            Some(&account),
            open_risk,
            &news,
            now_millis(),
        ));

        lock(&self.plans).insert(
            plan.id,
            StoredPlan {
                plan: plan.clone(),
                at: Instant::now(),
            },
        );
        self.emit(
            Some(command),
            EventKind::OrderPlanned(PlanSummary::from(&plan)),
        );
        Ok(plan)
    }

    /// Removes and returns a stored plan if it is known and fresh.
    fn take_plan(&self, id: PlanId) -> Result<OrderPlan> {
        let stored = lock(&self.plans).remove(&id).ok_or_else(|| {
            EngineError::ConfirmationRejected(format!("{id} is unknown or was already submitted"))
        })?;
        if stored.at.elapsed() > self.config.trading.plan_ttl {
            return Err(EngineError::ConfirmationRejected(format!(
                "{id} expired after {} s; plan again from a fresh quote",
                self.config.trading.plan_ttl.as_secs()
            )));
        }
        Ok(stored.plan)
    }

    // ---- submitting ----

    pub(crate) async fn submit(&self, id: PlanId, command: CommandId) -> Result<OrderOutcome> {
        let broker = self.ready_broker()?;
        let plan = self.take_plan(id)?;
        if self.account_id().as_ref() != Some(&plan.account) {
            return Err(EngineError::ConfirmationRejected(
                "the plan belongs to a different account".to_owned(),
            ));
        }
        // A dry run touches nothing, so it takes no lock and does not count toward the
        // minimum interval between real orders.
        if self.snapshot().mode != TradingMode::Armed {
            let outcome = OrderOutcome::DryRun {
                would_send: PlanSummary::from(&plan),
            };
            self.emit(Some(command), EventKind::OrderResult(outcome.clone()));
            return Ok(outcome);
        }
        let _flight = InFlight::acquire(self, &plan.symbol)?;
        if !broker.can_trade() {
            return Err(EngineError::TradingUnavailable(
                "this connection is read-only".to_owned(),
            ));
        }

        // Positions before the order, to recognize the new one afterwards. If this read
        // fails nothing has been sent yet, so failing here is safe.
        let before: Vec<PositionId> = self
            .io("positions", broker.positions())
            .await?
            .iter()
            .map(|p| p.id)
            .collect();

        let label = order_label(&broker.label_prefix(), plan.id);
        let market = MarketOrder {
            symbol: plan.symbol.clone(),
            side: plan.side,
            volume: plan.volume,
            stop_loss_distance: plan.stop_loss_distance,
            take_profit_distance: plan.take_profit_distance,
            label: label.clone(),
            slippage_points: self.config.trading.market_slippage_points,
        };
        self.emit(
            Some(command),
            EventKind::OrderSubmitted {
                label: label.clone(),
            },
        );

        let sent = tokio::time::timeout(
            self.config.trading.order_timeout,
            broker.place_market(&market),
        )
        .await;
        let uncertainty = match sent {
            Ok(Ok(_)) => None,
            Ok(Err(error)) if certainly_not_sent(&error) => {
                let outcome = OrderOutcome::Rejected {
                    plan: plan.id,
                    reason: error.to_string(),
                };
                self.emit(Some(command), EventKind::OrderResult(outcome.clone()));
                return Ok(outcome);
            }
            Ok(Err(error)) => Some(error.to_string()),
            Err(_) => Some(format!(
                "no answer within {} s",
                self.config.trading.order_timeout.as_secs()
            )),
        };

        let outcome = match self.confirm(&broker, &plan, &label, &before).await {
            Some(position) if position.volume >= plan.volume => OrderOutcome::Filled {
                plan: plan.id,
                position,
            },
            Some(position) => OrderOutcome::PartiallyFilled {
                requested: plan.volume,
                plan: plan.id,
                position,
            },
            None => {
                let reason = uncertainty.unwrap_or_else(|| {
                    "the broker accepted the order but no matching position appeared".to_owned()
                });
                self.track_unknown(&label, &plan, &before);
                OrderOutcome::Unknown {
                    plan: plan.id,
                    label,
                    reason,
                }
            }
        };
        self.emit(Some(command), EventKind::OrderResult(outcome.clone()));
        self.refresh_soon().await;
        Ok(outcome)
    }

    /// Looks for the position an order opened, by reading positions back a few times.
    async fn confirm(
        &self,
        broker: &Arc<dyn Broker>,
        plan: &OrderPlan,
        label: &str,
        before: &[PositionId],
    ) -> Option<Position> {
        for attempt in 0..self.config.trading.confirm_attempts {
            tokio::time::sleep(self.config.trading.confirm_delay).await;
            let positions = match self.io("positions", broker.positions()).await {
                Ok(p) => p,
                Err(error) => {
                    tracing::debug!(%error, attempt, "confirmation read failed");
                    continue;
                }
            };
            if let Some(found) = find_new_position(&positions, before, plan, label) {
                return Some(found);
            }
        }
        None
    }

    fn track_unknown(&self, label: &str, plan: &OrderPlan, before: &[PositionId]) {
        lock(&self.gate).unknown.push(UnknownOrder {
            label: label.to_owned(),
            symbol: plan.symbol.clone(),
            side: plan.side,
            volume: plan.volume,
            before: before.to_vec(),
        });
        self.raise_warning(
            format!("unknown-order:{label}"),
            WarningKind::UnknownOrder,
            format!(
                "the {} {} order for {} could not be confirmed. Check your positions in the platform (label {label}).",
                plan.volume, plan.side, plan.symbol
            ),
        );
    }

    pub(crate) fn dismiss_warning(&self, id: &str) -> bool {
        let removed = self.clear_warning(id);
        if let Some(label) = id.strip_prefix("unknown-order:") {
            lock(&self.gate).unknown.retain(|u| u.label != label);
        }
        removed
    }

    // ---- modifying ----

    pub(crate) async fn set_protection(
        &self,
        position: PositionId,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        command: CommandId,
    ) -> Result<()> {
        let broker = self.armed_broker()?;
        if stop_loss.is_none() && take_profit.is_none() {
            return Err(EngineError::Invalid(
                "nothing to change: give a stop loss or a take profit".to_owned(),
            ));
        }
        for (name, value) in [("stop loss", stop_loss), ("take profit", take_profit)] {
            if let Some(v) = value
                && !(v.is_finite() && v > 0.0)
            {
                return Err(EngineError::Invalid(format!(
                    "{name} must be a positive price, got {v}"
                )));
            }
        }
        let current = self
            .snapshot()
            .position(position)
            .cloned()
            .ok_or_else(|| EngineError::Invalid(format!("no open position {position}")))?;
        let sl = stop_loss.or(current.stop_loss);
        let tp = take_profit.or(current.take_profit);
        if let (Some(sl), Some(tp)) = (sl, tp) {
            let ok = match current.side {
                Side::Buy => sl < tp,
                Side::Sell => sl > tp,
            };
            if !ok {
                return Err(EngineError::Invalid(format!(
                    "for a {} the stop loss ({sl}) and take profit ({tp}) are on the wrong sides of each other",
                    current.side
                )));
            }
        }
        let result = tokio::time::timeout(
            self.config.trading.order_timeout,
            broker.set_protection(&current, stop_loss, take_profit),
        )
        .await
        .unwrap_or(Err(EngineError::Timeout {
            operation: "the protection change",
        }));
        // Whatever happened, show the truth.
        self.refresh_soon().await;
        result?;
        self.emit(Some(command), EventKind::ProtectionChanged { position });
        Ok(())
    }

    pub(crate) async fn close_position(
        &self,
        position: PositionId,
        size: CloseSize,
        command: CommandId,
    ) -> Result<()> {
        let broker = self.armed_broker()?;
        let current = self
            .snapshot()
            .position(position)
            .cloned()
            .ok_or_else(|| EngineError::Invalid(format!("no open position {position}")))?;
        let volume = match size {
            CloseSize::Full => None,
            CloseSize::Volume(v) => {
                let instrument = self.instrument(&broker, &current.symbol).await?;
                instrument.check_volume(v).map_err(EngineError::Invalid)?;
                if v >= current.volume {
                    return Err(EngineError::Invalid(format!(
                        "{v} is not smaller than the open volume ({}); use a full close",
                        current.volume
                    )));
                }
                Some(v)
            }
        };
        let result = tokio::time::timeout(
            self.config.trading.order_timeout,
            broker.close_position(&current, volume),
        )
        .await
        .unwrap_or(Err(EngineError::Timeout {
            operation: "the close",
        }));
        self.refresh_soon().await;
        result?;
        self.emit(Some(command), EventKind::PositionClosed { position });
        Ok(())
    }

    pub(crate) async fn cancel_order(&self, order: OrderId, _command: CommandId) -> Result<()> {
        let broker = self.armed_broker()?;
        if !self.snapshot().pending_orders.iter().any(|o| o.id == order) {
            return Err(EngineError::Invalid(format!("no working order {order}")));
        }
        let result = tokio::time::timeout(
            self.config.trading.order_timeout,
            broker.cancel_order(order),
        )
        .await
        .unwrap_or(Err(EngineError::Timeout {
            operation: "the cancel",
        }));
        self.refresh_soon().await;
        result
    }

    // ---- flatten ----

    pub(crate) async fn preview_flatten(&self, scope: FlattenScope) -> Result<FlattenPreview> {
        let broker = self.ready_broker()?;
        // Read fresh: a preview built from a stale list would mislead the confirmation.
        let positions = self.io("positions", broker.positions()).await?;
        let orders = self.io("pending orders", broker.pending_orders()).await?;
        let keep = |symbol: &str| match &scope {
            FlattenScope::All => true,
            FlattenScope::Symbol(s) => s.eq_ignore_ascii_case(symbol),
        };
        let positions: Vec<Position> = positions.into_iter().filter(|p| keep(&p.symbol)).collect();
        let orders: Vec<PendingOrder> = orders.into_iter().filter(|o| keep(&o.symbol)).collect();
        let ttl = self.config.trading.flatten_preview_ttl;
        let preview = FlattenPreview {
            token: uuid::Uuid::new_v4().to_string(),
            positions,
            orders,
            expires_at: now_millis() + i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX),
        };
        lock(&self.gate).flatten = Some((preview.clone(), Instant::now()));
        Ok(preview)
    }

    pub(crate) async fn flatten(&self, token: &str, command: CommandId) -> Result<FlattenReport> {
        let broker = self.armed_broker()?;
        let preview = {
            let mut gate = lock(&self.gate);
            match gate.flatten.take() {
                Some((p, at)) if p.token == token => {
                    if at.elapsed() > self.config.trading.flatten_preview_ttl {
                        return Err(EngineError::ConfirmationRejected(
                            "the flatten preview expired; preview again".to_owned(),
                        ));
                    }
                    p
                }
                other => {
                    gate.flatten = other;
                    return Err(EngineError::ConfirmationRejected(
                        "unknown or already used flatten token".to_owned(),
                    ));
                }
            }
        };

        // Act on what exists now, restricted to what the person was shown.
        let live = self.io("positions", broker.positions()).await?;
        let live_orders = self.io("pending orders", broker.pending_orders()).await?;
        let mut report = FlattenReport::default();
        for shown in &preview.positions {
            let Some(current) = live.iter().find(|p| p.id == shown.id) else {
                continue; // already gone (stop hit, closed elsewhere)
            };
            match self
                .timed(broker.close_position(current, None), "a close")
                .await
            {
                Ok(()) => report.closed.push(current.id),
                Err(error) => report
                    .errors
                    .push(format!("position {}: {error}", current.id)),
            }
        }
        for shown in &preview.orders {
            if !live_orders.iter().any(|o| o.id == shown.id) {
                continue;
            }
            match self.timed(broker.cancel_order(shown.id), "a cancel").await {
                Ok(()) => report.cancelled.push(shown.id),
                Err(error) => report.errors.push(format!("order {}: {error}", shown.id)),
            }
        }
        self.emit(Some(command), EventKind::Flattened(report.clone()));
        self.refresh_soon().await;
        Ok(report)
    }

    async fn timed(
        &self,
        call: impl std::future::Future<Output = Result<()>>,
        operation: &'static str,
    ) -> Result<()> {
        tokio::time::timeout(self.config.trading.order_timeout, call)
            .await
            .unwrap_or(Err(EngineError::Timeout { operation }))
    }
}

/// Builds the label that tags an order: the session prefix plus the plan id, capped at the
/// 100 characters Remote allows.
fn order_label(prefix: &str, plan: PlanId) -> String {
    let mut label = format!("{prefix}-{plan}");
    label.truncate(100);
    label
}

/// Finds the position an order opened: a position that did not exist before, preferring one
/// with the order's label, then one with the plan's symbol, side and volume, then any with
/// the plan's symbol and side.
fn find_new_position(
    positions: &[Position],
    before: &[PositionId],
    plan: &OrderPlan,
    label: &str,
) -> Option<Position> {
    let fresh: Vec<&Position> = positions
        .iter()
        .filter(|p| !before.contains(&p.id))
        .collect();
    fresh
        .iter()
        .find(|p| p.label.as_deref() == Some(label))
        .or_else(|| {
            fresh.iter().find(|p| {
                p.symbol.eq_ignore_ascii_case(&plan.symbol)
                    && p.side == plan.side
                    && p.volume == plan.volume
            })
        })
        .or_else(|| {
            fresh
                .iter()
                .find(|p| p.symbol.eq_ignore_ascii_case(&plan.symbol) && p.side == plan.side)
        })
        .map(|p| (*p).clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SpecsSource;

    fn plan() -> OrderPlan {
        OrderPlan {
            id: PlanId::next(),
            account: AccountId::new("a"),
            symbol: "EURUSD".into(),
            side: Side::Buy,
            volume: Volume::from_units(10_000),
            entry_reference: 1.1,
            stop_loss_distance: None,
            take_profit_distance: None,
            stop_loss_price: None,
            take_profit_price: None,
            risk_amount: None,
            risk_percent: None,
            account_currency: None,
            spread: 0.0,
            specs_source: SpecsSource::Broker,
            warnings: vec![],
            created_at: 0,
        }
    }

    fn position(id: i64, label: Option<&str>, volume: i64, side: Side) -> Position {
        Position {
            id: PositionId(id),
            symbol: "EURUSD".into(),
            side,
            volume: Volume::from_units(volume),
            entry_price: Some(1.1),
            stop_loss: None,
            take_profit: None,
            swap: None,
            commission: None,
            unrealized_pnl: None,
            label: label.map(str::to_owned),
        }
    }

    #[test]
    fn the_new_position_is_found_by_label_first() {
        let p = plan();
        let positions = [
            position(1, None, 10_000, Side::Buy),
            position(2, None, 10_000, Side::Buy),
            position(3, Some("mine"), 5_000, Side::Buy),
        ];
        let found = find_new_position(&positions, &[PositionId(1)], &p, "mine").unwrap();
        assert_eq!(found.id, PositionId(3));
    }

    #[test]
    fn without_a_label_symbol_side_and_volume_identify_it_then_symbol_and_side() {
        let p = plan();
        let positions = [
            position(1, None, 10_000, Side::Buy),
            position(2, None, 7_000, Side::Buy),
            position(3, None, 10_000, Side::Buy),
        ];
        assert_eq!(
            find_new_position(&positions, &[PositionId(1)], &p, "x")
                .unwrap()
                .id,
            PositionId(3)
        );
        assert_eq!(
            find_new_position(&positions[..2], &[PositionId(1)], &p, "x")
                .unwrap()
                .id,
            PositionId(2)
        );
    }

    #[test]
    fn a_pre_existing_position_is_never_mistaken_for_the_new_one() {
        let p = plan();
        let positions = [position(1, Some("mine"), 10_000, Side::Buy)];
        assert!(find_new_position(&positions, &[PositionId(1)], &p, "mine").is_none());
        assert!(
            find_new_position(&[position(9, None, 10_000, Side::Sell)], &[], &p, "x").is_none(),
            "wrong side"
        );
    }

    #[test]
    fn labels_are_capped_at_one_hundred_characters() {
        let label = order_label(&"p".repeat(200), PlanId::next());
        assert_eq!(label.len(), 100);
        assert!(order_label("sess-1a2b3c4d", PlanId::next()).starts_with("sess-1a2b3c4d-plan-"));
    }

    #[test]
    fn only_definitive_errors_mean_not_sent() {
        assert!(certainly_not_sent(&EngineError::Invalid("x".into())));
        assert!(certainly_not_sent(&EngineError::Broker {
            kind: BrokerErrorKind::Rejected,
            retryable: false,
            message: String::new()
        }));
        assert!(!certainly_not_sent(&EngineError::Timeout {
            operation: "x"
        }));
        assert!(!certainly_not_sent(&EngineError::Broker {
            kind: BrokerErrorKind::Connection,
            retryable: true,
            message: String::new()
        }));
        assert!(!certainly_not_sent(&EngineError::Broker {
            kind: BrokerErrorKind::Unavailable,
            retryable: true,
            message: String::new()
        }));
    }

    #[test]
    fn flatten_report_knows_when_it_is_clean() {
        let mut r = FlattenReport::default();
        assert!(r.fully_flattened());
        r.errors.push("boom".into());
        assert!(!r.fully_flattened());
    }
}

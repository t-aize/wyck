//! Sending orders: one at a time or in a batch, the queue, the plans and their time stops.

use super::{
    Account, Busy, CALL_TIMEOUT, HISTORY_DAYS, Notice, Status, TIME_STOP_RETRY, Tone, describe,
    fingerprint, flatten,
};
use crate::app::broker::trading::NewOrderReq;
use crate::app::broker::{Error as ApiError, Event, Result as ApiResult};
use crate::app::system::runtime;
use gpui::Context;
use std::collections::HashMap;
use std::time::Instant;
use wyck_trading::ExecutionType;
use wyck_trading::PositionStatus;
use wyck_trading::guard::Verdict;
use wyck_trading::math::Contract;

impl Account {
    /// Runs a trading call; on failure says why and reads the account again (the call may have
    /// reached the server even so).
    pub(super) fn trade<F, Fut>(&mut self, busy: Busy, cx: &mut Context<Self>, call: F)
    where
        F: FnOnce(crate::app::broker::trading::TradingClient) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ApiResult<wyck_trading::ExecutionEvent>> + Send + 'static,
    {
        if !self.busy.insert(busy) {
            return;
        }
        self.call_seq += 1;
        let seq = self.call_seq;
        self.calls.insert(busy, seq);
        cx.notify();
        // No answer in time: the button is free again, but the account cannot tell what became of
        // the call, so it says so and reads the account.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CALL_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| {
                if this.calls.get(&busy) == Some(&seq) {
                    this.calls.remove(&busy);
                    this.busy.remove(&busy);
                    this.uncertain = true;
                    this.queue.clear();
                    this.tell(
                        Notice::new(
                            Tone::Warning,
                            "No answer from the server",
                            "The request may or may not have gone through. Do not send it again yet.",
                        )
                        .hint(Some(
                            "Check the positions and orders of the account, then try again if it is not there."
                                .to_owned(),
                        )),
                        cx,
                    );
                    this.on_ready(cx);
                    cx.notify();
                }
            });
        })
        .detach();
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime::spawn(async move {
                let client = session.client().ok_or(ApiError::Closed)?;
                call(client.account(session.account_id()).trading()).await
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                // A call that was given up on has nothing to release: a newer one may hold it.
                if this.calls.get(&busy) == Some(&seq) {
                    this.calls.remove(&busy);
                    this.busy.remove(&busy);
                }
                match flatten(result) {
                    // The first execution event answers the request and goes only to it; the
                    // ones after it (a fill after the acceptance) come on the event stream.
                    Ok(event) => {
                        let reverse_order = match busy {
                            Busy::Closing(id) => this.reversals.acknowledged(
                                id,
                                event.kind(),
                                event.order.as_ref().map(|o| o.order_id),
                            ),
                            _ => None,
                        };
                        this.on_event(&Event::Execution(Box::new(event)), cx);
                        if let Some(order) = reverse_order {
                            this.market(order.symbol, order.buy, order.volume, cx);
                        }
                        if let Busy::Closing(id) = busy
                            && let Some(pending) = this.reversals.0.get(&id)
                        {
                            this.watch_reverse(id, pending.started, cx);
                        }
                        if busy == Busy::Placing {
                            this.send_next(cx);
                        }
                    }
                    Err(error) => {
                        if let Busy::Closing(id) = busy {
                            this.reversals.0.remove(&id);
                        }
                        if busy == Busy::Placing {
                            this.drop_queue(cx);
                        }
                        let reason = describe(&error);
                        let title = match busy {
                            Busy::Placing => "The order was not sent",
                            Busy::Closing(_) => "The position was not closed",
                            Busy::Cancelling(_) => "The order was not cancelled",
                            Busy::Amending(_) => "The change was not applied",
                        };
                        this.tell(
                            Notice::new(Tone::Error, title, reason.message)
                                .hint(reason.hint)
                                .details(error.to_string()),
                            cx,
                        );
                        this.on_ready(cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Sends a new order.
    pub fn place(&mut self, order: NewOrderReq, cx: &mut Context<Self>) {
        self.place_with(order, false, cx);
    }

    /// Sends a new order. `one_click` says it was sent without a confirmation, which the safety
    /// limits treat more strictly. The order is refused when a hard limit says so, and dropped
    /// when it repeats the one just sent.
    pub fn place_with(&mut self, mut order: NewOrderReq, one_click: bool, cx: &mut Context<Self>) {
        order.label.get_or_insert_with(|| "wyck".into());
        if let Err(error) = order.validate() {
            self.tell(
                Notice::new(Tone::Error, "The order is not complete", error.to_string()),
                cx,
            );
            return;
        }
        if let Verdict::Block(reason) = self.assess(&order, one_click) {
            self.tell(
                Notice::new(Tone::Warning, "Blocked by your safety settings", reason)
                    .hint(Some("Change the limits in Settings, Safety.".to_owned())),
                cx,
            );
            return;
        }
        let now = crate::app::market_data::now_ms();
        let fingerprint = fingerprint(&order);
        if self.busy.contains(&Busy::Placing) || self.duplicates.is_repeat(fingerprint, now) {
            self.tell(
                Notice::new(
                    Tone::Info,
                    "Order not sent again",
                    "The same order was sent a moment ago.",
                ),
                cx,
            );
            return;
        }
        let today = self.zone.day(now);
        if self.sent_today.0 != today {
            self.sent_today = (today, 0);
        }
        self.sent_today.1 += 1;
        self.trade(Busy::Placing, cx, move |trading| async move {
            trading.new_order(order).await
        });
    }

    /// What the safety checks say about the orders of a plan together: the side that sends the
    /// most is checked with its volumes added up (an OCO pair fills on one side only).
    pub fn assess_batch(&self, orders: &[NewOrderReq], one_click: bool) -> Verdict {
        let mut totals: Vec<(i32, i64, usize)> = Vec::new();
        for (index, order) in orders.iter().enumerate() {
            match totals
                .iter_mut()
                .find(|(side, _, _)| *side == order.trade_side)
            {
                Some(entry) => entry.1 += order.volume,
                None => totals.push((order.trade_side, order.volume, index)),
            }
        }
        let Some(&(_, volume, first)) = totals.iter().max_by_key(|(_, volume, _)| *volume) else {
            return Verdict::Ok;
        };
        let mut probe = orders[first].clone();
        probe.volume = volume;
        self.assess(&probe, one_click)
    }

    /// Sends the orders of a plan one after the other, each once the one before it was answered.
    /// They are checked and counted as one order. If one fails, the ones after it are not sent.
    pub fn place_batch(
        &mut self,
        orders: Vec<NewOrderReq>,
        one_click: bool,
        cx: &mut Context<Self>,
    ) {
        if orders.len() <= 1 {
            if let Some(order) = orders.into_iter().next() {
                self.place_with(order, one_click, cx);
            }
            return;
        }
        for order in &orders {
            if let Err(error) = order.validate() {
                self.tell(
                    Notice::new(Tone::Error, "The order is not complete", error.to_string()),
                    cx,
                );
                return;
            }
        }
        if let Verdict::Block(reason) = self.assess_batch(&orders, one_click) {
            self.tell(
                Notice::new(Tone::Warning, "Blocked by your safety settings", reason)
                    .hint(Some("Change the limits in Settings, Safety.".to_owned())),
                cx,
            );
            return;
        }
        let now = crate::app::market_data::now_ms();
        if self.busy.contains(&Busy::Placing)
            || !self.queue.is_empty()
            || self.duplicates.is_repeat(fingerprint(&orders[0]), now)
        {
            self.tell(
                Notice::new(
                    Tone::Info,
                    "Order not sent again",
                    "The same order was sent a moment ago.",
                ),
                cx,
            );
            return;
        }
        let today = self.zone.day(now);
        if self.sent_today.0 != today {
            self.sent_today = (today, 0);
        }
        self.sent_today.1 += 1;
        for mut order in orders {
            order.label.get_or_insert_with(|| "wyck".into());
            self.queue.push_back(order);
        }
        self.send_next(cx);
    }

    /// Sends the next order of a plan, when none is in flight.
    pub(super) fn send_next(&mut self, cx: &mut Context<Self>) {
        if self.busy.contains(&Busy::Placing) {
            return;
        }
        if let Some(order) = self.queue.pop_front() {
            self.trade(Busy::Placing, cx, move |trading| async move {
                trading.new_order(order).await
            });
        }
    }

    /// Drops the orders of a plan that were waiting, and says so.
    pub(super) fn drop_queue(&mut self, cx: &mut Context<Self>) {
        let left = self.queue.len();
        self.queue.clear();
        if left > 0 {
            self.tell(
                Notice::new(
                    Tone::Warning,
                    "The rest of the plan was not sent",
                    format!("{left} order(s) of the plan were left out."),
                )
                .hint(Some(
                    "Check what is open before sending it again.".to_owned(),
                )),
                cx,
            );
        }
    }

    /// What the app adds to the orders of a plan (see [`super::plan`]): an OCO pair whose one
    /// side filled loses the other, and a leg that closed in profit moves the stops of the legs
    /// left to their entry.
    pub(super) fn manage(
        &mut self,
        execution: &wyck_trading::ExecutionEvent,
        cx: &mut Context<Self>,
    ) {
        use wyck_trading::plan;
        use wyck_trading::plan::{Label, Open};
        if execution.kind() != Some(ExecutionType::OrderFilled) {
            return;
        }
        if let Some(order) = &execution.order
            && let Some(label) = order.trade_data.label.as_deref().and_then(Label::decode)
            && label.oco
        {
            let buy = order.trade_data.is_buy();
            let working: Vec<(i64, Label, bool)> = self
                .book
                .orders
                .values()
                .filter_map(|o| {
                    Some((
                        o.order_id,
                        Label::decode(o.trade_data.label.as_deref()?)?,
                        o.trade_data.is_buy(),
                    ))
                })
                .collect();
            for id in plan::oco_siblings(&label, buy, &working) {
                self.cancel_order(id, cx);
            }
        }
        if let (Some(position), Some(deal)) = (&execution.position, &execution.deal)
            && position.status() == Some(PositionStatus::Closed)
            && let Some(profit) = deal.realized_pnl()
            && let Some(label) = position.trade_data.label.as_deref().and_then(Label::decode)
        {
            let pip = self.book.contract(position.trade_data.symbol_id).pip();
            let open: Vec<Open> = self
                .book
                .positions
                .values()
                .filter_map(|p| {
                    Some(Open {
                        position: p.position_id,
                        label: Label::decode(p.trade_data.label.as_deref()?)?,
                        buy: p.trade_data.is_buy(),
                        entry: p.price?,
                        stop_loss: p.stop_loss,
                    })
                })
                .collect();
            for (id, stop) in plan::break_even_moves(&label, profit, &open, pip) {
                let take_profit = self.book.positions.get(&id).and_then(|p| p.take_profit);
                self.protect_position(id, Some(stop), take_profit, cx);
            }
        }
    }

    /// Closes the positions whose time stop is up (see [`super::plan::TimeStop`]). It runs with
    /// every reading of the profit, so a time that ran out while the app was closed is caught at
    /// the next start. Closing is never held back by the safety limits: they are for new orders.
    pub(super) fn enforce_time_stops(&mut self, cx: &mut Context<Self>) {
        use wyck_trading::plan::Label;
        // While a call is unanswered the account is not known well enough to close on its own.
        if self.status != Status::Ready || self.uncertain {
            return;
        }
        let now = crate::app::market_data::now_ms();
        self.time_stops
            .retain(|id, _| self.book.positions.contains_key(id));
        let mut due = Vec::new();
        for position in self.book.positions.values() {
            let id = position.position_id;
            let Some(rule) = position
                .trade_data
                .label
                .as_deref()
                .and_then(Label::decode)
                .and_then(|l| l.time_stop)
            else {
                continue;
            };
            let Some(opened) = position.trade_data.open_timestamp else {
                continue;
            };
            let retry = self
                .time_stops
                .get(&id)
                .is_none_or(|at| at.elapsed() >= TIME_STOP_RETRY);
            if retry
                && !self.is_busy(Busy::Closing(id))
                && rule.due(opened, now, self.net_profit(id))
            {
                due.push((id, rule, position.trade_data.symbol_id));
            }
        }
        for (id, rule, symbol) in due {
            self.time_stops.insert(id, Instant::now());
            let name = self.book.name(symbol);
            self.tell(
                Notice::new(
                    Tone::Info,
                    "Time stop",
                    format!("Closing {name}, {}.", rule.describe()),
                ),
                cx,
            );
            self.close_position(id, None, cx);
        }
    }

    /// Does what a plan asked while the app was closed or cut off (see [`super::plan::catch_up`]),
    /// and says so once.
    pub(super) fn catch_up(
        &mut self,
        missed: Vec<wyck_trading::plan::Catch>,
        cx: &mut Context<Self>,
    ) {
        use wyck_trading::plan::Catch;
        let (mut stops, mut cancels) = (0, 0);
        for step in missed {
            match step {
                Catch::Stop { position, stop } => {
                    let Some(open) = self.book.positions.get(&position) else {
                        continue;
                    };
                    let take_profit = open.take_profit;
                    stops += 1;
                    self.protect_position(position, Some(stop), take_profit, cx);
                }
                Catch::Cancel { order } => {
                    if self.book.orders.contains_key(&order) {
                        cancels += 1;
                        self.cancel_order(order, cx);
                    }
                }
            }
        }
        if stops + cancels == 0 {
            return;
        }
        let mut parts = Vec::new();
        if stops > 0 {
            parts.push(format!(
                "{stops} stop loss{} moved to the entry",
                if stops == 1 { "" } else { "es" }
            ));
        }
        if cancels > 0 {
            parts.push(format!(
                "{cancels} order{} of an OCO pair cancelled",
                if cancels == 1 { "" } else { "s" }
            ));
        }
        self.tell(
            Notice::new(
                Tone::Info,
                "Plan caught up",
                format!("While the app was away: {}.", parts.join(", ")),
            ),
            cx,
        );
    }
}

/// What the plans of the account missed while the app was away, read from the broker. It asks
/// for the recent orders only when something open belongs to a plan with a rule to keep, and
/// gives nothing when any call fails: the next connection tries again.
pub(super) async fn missed_exits(
    account: &crate::app::broker::AccountClient,
    positions: &[wyck_trading::Position],
    orders: &[wyck_trading::Order],
    deals: &[wyck_trading::Deal],
    now: i64,
) -> Vec<wyck_trading::plan::Catch> {
    use wyck_trading::OrderStatus;
    use wyck_trading::plan;
    use wyck_trading::plan::{Label, Open, Past};
    let label_of = |text: &Option<String>| text.as_deref().and_then(Label::decode);
    let rules = positions
        .iter()
        .filter_map(|p| label_of(&p.trade_data.label))
        .any(|l| l.break_even.is_some())
        || orders
            .iter()
            .filter_map(|o| label_of(&o.trade_data.label))
            .any(|l| l.oco);
    if !rules {
        return Vec::new();
    }
    let Ok((history, _)) = account
        .account_data()
        .orders(now - HISTORY_DAYS * 86_400_000, now)
        .await
    else {
        return Vec::new();
    };
    let mut past: Vec<Past> = history
        .iter()
        .filter(|o| o.status() == Some(OrderStatus::Filled))
        .filter_map(|o| {
            let label = label_of(&o.trade_data.label)?;
            (label.oco || label.break_even.is_some()).then(|| Past {
                label,
                buy: o.trade_data.is_buy(),
                filled: true,
                position: o.position_id,
                pip: 0.0,
            })
        })
        .collect();
    if past.is_empty() {
        return Vec::new();
    }
    let symbols: Vec<i64> = {
        let mut ids: Vec<i64> = history
            .iter()
            .filter(|o| {
                o.position_id
                    .is_some_and(|id| past.iter().any(|p| p.position == Some(id)))
            })
            .map(|o| o.trade_data.symbol_id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    let Ok(details) = account.market().symbol_details(&symbols).await else {
        return Vec::new();
    };
    let pip_of = |symbol: i64| {
        details
            .iter()
            .find(|s| s.symbol_id == symbol)
            .map(|s| Contract::from_symbol(s).pip())
    };
    for old in &mut past {
        let symbol = history
            .iter()
            .find(|o| o.position_id == old.position && old.position.is_some())
            .map(|o| o.trade_data.symbol_id);
        old.pip = symbol.and_then(pip_of).unwrap_or(0.0);
    }
    // A pip we could not read would put the offset and the tolerance at zero: leave those out.
    past.retain(|p| p.pip > 0.0);
    let mut profits: HashMap<i64, f64> = HashMap::new();
    for deal in deals {
        if let Some(profit) = deal.realized_pnl() {
            *profits.entry(deal.position_id).or_default() += profit;
        }
    }
    let open: Vec<Open> = positions
        .iter()
        .filter_map(|p| {
            Some(Open {
                position: p.position_id,
                label: label_of(&p.trade_data.label)?,
                buy: p.trade_data.is_buy(),
                entry: p.price?,
                stop_loss: p.stop_loss,
            })
        })
        .collect();
    let working: Vec<(i64, Label, bool)> = orders
        .iter()
        .filter(|o| o.status() == Some(OrderStatus::Accepted))
        .filter_map(|o| {
            Some((
                o.order_id,
                label_of(&o.trade_data.label)?,
                o.trade_data.is_buy(),
            ))
        })
        .collect();
    plan::catch_up(&past, &profits, &open, &working)
}

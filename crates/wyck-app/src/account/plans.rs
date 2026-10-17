//! What the app does for the plans it sent (see [`wyck_trading::plan`]): cancelling the other side
//! of an OCO pair, moving the stops to break-even, closing at a time stop, and catching up on what
//! was missed while the app was away.

use super::{Account, Busy, HISTORY_DAYS, Notice, Status, TIME_STOP_RETRY, Tone};
use gpui::Context;
use std::collections::HashMap;
use std::time::Instant;
use wyck_trading::ExecutionType;
use wyck_trading::PositionStatus;
use wyck_trading::contract::Contract;

impl Account {
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
            let open: Vec<Open> = self.book.positions.values().filter_map(open_of).collect();
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
        let now = crate::market_data::now_ms();
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
            Self::tell(
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
        Self::tell(
            Notice::new(
                Tone::Info,
                "Plan caught up",
                format!("While the app was away: {}.", parts.join(", ")),
            ),
            cx,
        );
    }
}

/// A position as the break-even plans see it: only one this app opened (its label says so) and
/// that has an entry price.
fn open_of(p: &wyck_trading::Position) -> Option<wyck_trading::plan::Open> {
    Some(wyck_trading::plan::Open {
        position: p.position_id,
        label: wyck_trading::plan::Label::decode(p.trade_data.label.as_deref()?)?,
        buy: p.trade_data.is_buy(),
        entry: p.price?,
        stop_loss: p.stop_loss,
    })
}

/// What the plans of the account missed while the app was away, read from the broker. It asks
/// for the recent orders only when something open belongs to a plan with a rule to keep, and
/// gives nothing when any call fails: the next connection tries again.
pub(super) async fn missed_exits(
    account: &crate::broker::AccountClient,
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
    let open: Vec<Open> = positions.iter().filter_map(open_of).collect();
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

//! What the account holds, kept current from the server's answers and events: the balance, the
//! open positions, the working orders, the recent deals, and each position's profit.
//!
//! It is plain data: a reconcile answer, an execution event or a profit answer comes in, the
//! state changes, and an [`Outcome`] says what became of the order. The gpui entity around it
//! (the account client) only fetches and forwards.

use std::collections::{BTreeMap, HashMap};

use crate::domain::trading::{
    Deal, Order, OrderStatus, OrderType, Position, PositionStatus, PositionUnrealizedPnL,
    TradeSide, Trader, money,
};
use crate::domain::trading::{ExecutionEvent, ExecutionType};

use crate::domain::trading::contract as math;
use crate::domain::trading::contract::{Contract, PnlMark, Summary};

/// The most recent deals kept for the history.
const MAX_DEALS: usize = 500;

/// What an execution event came to, for whoever tells the user. The words are not here: the order
/// is described by its side, size, symbol and price, and the screen decides how to say it.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The order was filled, or the position closed. `position` is the position that stays open.
    Filled {
        /// The order in a few words.
        what: String,
        /// The execution price, formatted.
        price: Option<String>,
        /// Whether a position was closed by it.
        closed: bool,
        /// The id of the position left open by it, if one.
        open_position: Option<i64>,
    },
    /// Part of the order was filled.
    PartlyFilled {
        /// The order in a few words.
        what: String,
    },
    /// A working order was placed.
    Placed {
        /// The order in a few words.
        what: String,
        /// Its id.
        order_id: i64,
    },
    /// A working order was changed.
    Changed {
        /// The order in a few words.
        what: String,
    },
    /// A working order was cancelled.
    Cancelled {
        /// The order in a few words.
        what: String,
    },
    /// A working order expired.
    Expired {
        /// The order in a few words.
        what: String,
    },
    /// The server refused the order or its cancellation.
    Refused {
        /// The order in a few words, empty when the event carried none.
        what: String,
        /// The error code of the server.
        code: String,
    },
    /// A deposit or a withdrawal.
    BalanceChanged,
}

/// What an execution event changed, beyond the positions and orders themselves.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Applied {
    /// What happened to the order, if it is worth telling the user.
    pub outcome: Option<Outcome>,
    /// The balance may have changed: ask for the account again.
    pub balance_changed: bool,
}

/// The account's state, rebuilt from a reconcile answer and kept current by execution events.
///
/// ```no_run
/// # async fn demo(account: wyck::infra::ctrader::AccountClient) -> wyck::infra::ctrader::Result<()> {
/// use wyck::infra::ctrader::Event;
/// use wyck::domain::trading::book::AccountBook;
///
/// let mut book = AccountBook::default();
/// let mut events = account.client().events();
/// let (positions, orders) = account.account_data().open_positions_and_orders(false).await?;
/// book.reconcile(positions, orders);
/// while let Ok(event) = events.recv().await {
///     if let Event::Execution(execution) = event {
///         if let Some(outcome) = book.apply(&execution).outcome {
///             println!("{outcome:?}");
///         }
///     }
/// }
/// # Ok(()) }
/// ```
#[derive(Debug, Clone, Default)]
pub struct AccountBook {
    /// The account itself, once the server sent it.
    pub trader: Option<Trader>,
    /// The deposit currency, when known: `USD`.
    pub currency: String,
    /// The open positions, by id.
    pub positions: BTreeMap<i64, Position>,
    /// The working orders, by id.
    pub orders: BTreeMap<i64, Order>,
    /// The server's last word on each position's profit.
    pub marks: HashMap<i64, PnlMark>,
    /// Recent deals, newest first.
    pub deals: Vec<Deal>,
    /// How each symbol trades, once the broker said.
    pub contracts: HashMap<i64, Contract>,
    /// The broker's name of each symbol.
    pub names: HashMap<i64, String>,
    /// The currency each symbol is quoted in, by name.
    pub quote_currency: HashMap<i64, String>,
}

/// The side of a position or order.
pub fn is_buy(side: i64) -> bool {
    TradeSide::from_number(side) != Some(TradeSide::Sell)
}

/// Whether an order is a working order the user placed (not the protection of a position).
fn is_working(order: &Order) -> bool {
    order.status() == Some(OrderStatus::Accepted)
        && !matches!(
            order.kind(),
            Some(OrderType::StopLossTakeProfit | OrderType::Market) | None
        )
}

impl AccountBook {
    /// The broker's name of a symbol, or `#id` before it is known.
    pub fn name(&self, symbol_id: i64) -> String {
        self.names
            .get(&symbol_id)
            .cloned()
            .unwrap_or_else(|| format!("#{symbol_id}"))
    }

    /// How a symbol trades, or a forex pair's contract before it is known.
    pub fn contract(&self, symbol_id: i64) -> Contract {
        self.contracts.get(&symbol_id).copied().unwrap_or_default()
    }

    /// Decimals of the account's money.
    pub fn money_digits(&self) -> Option<u32> {
        self.trader.as_ref().and_then(|t| t.digits())
    }

    /// The balance in the deposit currency, `0.0` before the account is known.
    pub fn balance(&self) -> f64 {
        self.trader.as_ref().map_or(0.0, Trader::balance_amount)
    }

    /// Replaces everything with what the server says is open now.
    pub fn reconcile(&mut self, positions: Vec<Position>, orders: Vec<Order>) {
        self.positions = positions
            .into_iter()
            .filter(|p| p.status() == Some(PositionStatus::Open))
            .map(|p| (p.position_id, p))
            .collect();
        self.orders = orders
            .into_iter()
            .filter(is_working)
            .map(|o| (o.order_id, o))
            .collect();
        self.marks.retain(|id, _| self.positions.contains_key(id));
    }

    /// Every symbol the account holds or has orders on.
    pub fn symbols(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .positions
            .values()
            .map(|p| p.trade_data.symbol_id)
            .chain(self.orders.values().map(|o| o.trade_data.symbol_id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn describe_order(&self, order: &Order) -> String {
        let contract = self.contract(order.trade_data.symbol_id);
        let side = if is_buy(order.trade_data.trade_side) {
            "Buy"
        } else {
            "Sell"
        };
        let lots = math::format_lots(contract.lots_of_volume(order.trade_data.volume));
        let kind = order.kind().map_or("order", OrderType::label);
        let price = order
            .limit_price
            .or(order.stop_price)
            .map(|p| format!(" at {}", contract.format_price(p)))
            .unwrap_or_default();
        format!(
            "{side} {lots} {} {kind}{price}",
            self.name(order.trade_data.symbol_id)
        )
    }

    /// Applies an execution event: the answer to an order, or something the server did (a stop
    /// out, a swap, a deposit).
    pub fn apply(&mut self, event: &ExecutionEvent) -> Applied {
        let mut applied = Applied::default();
        if let Some(position) = &event.position {
            if position.status() == Some(PositionStatus::Open) {
                self.positions
                    .insert(position.position_id, position.clone());
            } else {
                self.positions.remove(&position.position_id);
                self.marks.remove(&position.position_id);
            }
        }
        let order_text = event.order.as_ref().map(|o| self.describe_order(o));
        if let Some(order) = &event.order {
            if is_working(order) {
                self.orders.insert(order.order_id, order.clone());
            } else {
                self.orders.remove(&order.order_id);
            }
        }
        if let Some(deal) = &event.deal {
            self.deals.retain(|d| d.deal_id != deal.deal_id);
            self.deals.insert(0, deal.clone());
            self.deals.truncate(MAX_DEALS);
            applied.balance_changed = true;
        }
        let what = order_text.unwrap_or_default();
        applied.outcome = match event.kind() {
            Some(ExecutionType::OrderFilled) => {
                let price = event.deal.as_ref().and_then(|d| {
                    let p = d.execution_price?;
                    Some(self.contract(d.symbol_id).format_price(p))
                });
                Some(Outcome::Filled {
                    what,
                    price,
                    closed: event
                        .position
                        .as_ref()
                        .is_some_and(|p| p.status() == Some(PositionStatus::Closed)),
                    open_position: event
                        .position
                        .as_ref()
                        .filter(|p| p.status() == Some(PositionStatus::Open))
                        .map(|p| p.position_id),
                })
            }
            Some(ExecutionType::OrderPartialFill) => Some(Outcome::PartlyFilled { what }),
            Some(ExecutionType::OrderAccepted) => event
                .order
                .as_ref()
                .filter(|o| is_working(o))
                .map(|o| Outcome::Placed {
                    what,
                    order_id: o.order_id,
                }),
            Some(ExecutionType::OrderReplaced) => Some(Outcome::Changed { what }),
            Some(ExecutionType::OrderCancelled) => Some(Outcome::Cancelled { what }),
            Some(ExecutionType::OrderExpired) => Some(Outcome::Expired { what }),
            Some(ExecutionType::OrderRejected | ExecutionType::OrderCancelRejected) => {
                Some(Outcome::Refused {
                    what,
                    code: event
                        .error_code
                        .clone()
                        .unwrap_or_else(|| "refused by the server".to_owned()),
                })
            }
            Some(ExecutionType::DepositWithdraw | ExecutionType::BonusDepositWithdraw) => {
                applied.balance_changed = true;
                Some(Outcome::BalanceChanged)
            }
            Some(ExecutionType::Swap) => {
                applied.balance_changed = true;
                None
            }
            None => None,
        };
        applied
    }

    /// Takes the server's answer on the positions' profit, noting the market at that moment.
    pub fn set_pnl(
        &mut self,
        answer: &[PositionUnrealizedPnL],
        money_digits: Option<u32>,
        quotes: &dyn Fn(i64) -> (Option<f64>, Option<f64>),
    ) {
        for pnl in answer {
            let Some(position) = self.positions.get(&pnl.position_id) else {
                continue;
            };
            let symbol = position.trade_data.symbol_id;
            let gross = money(pnl.gross_unrealized_pnl, money_digits);
            // Quoted in the deposit currency: the rate is one. Otherwise it is read from the
            // answer when the move is large enough to tell, and kept from the last answer that
            // was.
            let rate = if self.quoted_in_deposit(symbol) {
                Some(1.0)
            } else {
                let pip = self.contract(symbol).pip() * position.trade_data.units();
                self.quote_profit(position, quotes)
                    .and_then(|quote| math::implied_rate(gross, quote, pip))
                    .or_else(|| self.marks.get(&pnl.position_id).and_then(|m| m.rate))
            };
            self.marks.insert(
                pnl.position_id,
                PnlMark {
                    gross,
                    net: money(pnl.net_unrealized_pnl, money_digits),
                    rate,
                },
            );
        }
    }

    /// Whether a symbol's prices are in the account's currency.
    fn quoted_in_deposit(&self, symbol_id: i64) -> bool {
        !self.currency.is_empty()
            && self
                .quote_currency
                .get(&symbol_id)
                .is_some_and(|quote| quote.eq_ignore_ascii_case(&self.currency))
    }

    /// A position's profit in the quote currency at the current market.
    fn quote_profit(
        &self,
        position: &Position,
        quotes: &dyn Fn(i64) -> (Option<f64>, Option<f64>),
    ) -> Option<f64> {
        let buy = is_buy(position.trade_data.trade_side);
        let (bid, ask) = quotes(position.trade_data.symbol_id);
        let close = if buy { bid } else { ask }?;
        Some(math::quote_profit(
            buy,
            position.price?,
            close,
            position.trade_data.units(),
        ))
    }

    /// A position's profit after costs, now.
    pub fn net_profit(
        &self,
        position_id: i64,
        quotes: &dyn Fn(i64) -> (Option<f64>, Option<f64>),
    ) -> Option<f64> {
        let position = self.positions.get(&position_id)?;
        let mark = self.marks.get(&position_id)?;
        Some(match self.quote_profit(position, quotes) {
            Some(now) => math::live_net(mark, now),
            None => mark.net,
        })
    }

    /// The totals of the account, now.
    pub fn summary(&self, quotes: &dyn Fn(i64) -> (Option<f64>, Option<f64>)) -> Summary {
        let digits = self.money_digits();
        let unrealized: f64 = self
            .positions
            .keys()
            .filter_map(|id| self.net_profit(*id, quotes))
            .sum();
        let margin: f64 = self
            .positions
            .values()
            .filter_map(|p| p.used_margin.map(|m| money(m, digits)))
            .sum();
        math::summary(self.balance(), unrealized, margin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn position(id: i64, symbol: i64, side: i64, price: f64, status: i64) -> Position {
        serde_json::from_value(json!({
            "positionId": id,
            "tradeData": {"symbolId": symbol, "volume": 10_000_000, "tradeSide": side},
            "positionStatus": status,
            "price": price,
            "usedMargin": 3_300,
            "moneyDigits": 2
        }))
        .unwrap()
    }

    fn order(id: i64, kind: i64, status: i64) -> Order {
        serde_json::from_value(json!({
            "orderId": id,
            "tradeData": {"symbolId": 1, "volume": 100_000, "tradeSide": 1},
            "orderType": kind,
            "orderStatus": status,
            "limitPrice": 1.05
        }))
        .unwrap()
    }

    fn event(kind: i64, position: Option<Position>, order: Option<Order>) -> ExecutionEvent {
        ExecutionEvent {
            ctid_trader_account_id: Some(1),
            execution_type: kind,
            position,
            order,
            deal: None,
            error_code: None,
            is_server_event: None,
        }
    }

    #[test]
    fn a_reconcile_keeps_open_positions_and_working_orders_only() {
        let mut book = AccountBook::default();
        book.reconcile(
            vec![position(1, 1, 1, 1.1, 1), position(2, 1, 1, 1.1, 2)],
            vec![order(10, 2, 1), order(11, 4, 1), order(12, 2, 5)],
        );
        assert_eq!(book.positions.keys().copied().collect::<Vec<_>>(), vec![1]);
        assert_eq!(book.orders.keys().copied().collect::<Vec<_>>(), vec![10]);
        assert_eq!(book.symbols(), vec![1]);
    }

    #[test]
    fn a_fill_opens_a_position_and_removes_the_order() {
        let mut book = AccountBook::default();
        book.names.insert(1, "EURUSD".into());
        book.apply(&event(2, None, Some(order(10, 2, 1))));
        assert!(book.orders.contains_key(&10));
        let filled_order = order(10, 2, 2);
        let applied = book.apply(&event(
            3,
            Some(position(5, 1, 1, 1.05, 1)),
            Some(filled_order),
        ));
        assert!(book.positions.contains_key(&5));
        assert!(!book.orders.contains_key(&10));
        let Some(Outcome::Filled { what, closed, .. }) = applied.outcome else {
            panic!("a fill is an outcome");
        };
        assert!(!closed);
        assert!(what.contains("Buy 0.01 EURUSD limit at 1.05000"), "{what}");
    }

    #[test]
    fn a_closing_fill_removes_the_position_and_says_so() {
        let mut book = AccountBook::default();
        book.reconcile(vec![position(5, 1, 1, 1.05, 1)], vec![]);
        book.marks.insert(
            5,
            PnlMark {
                gross: 1.0,
                net: 1.0,
                rate: Some(1.0),
            },
        );
        let applied = book.apply(&event(
            3,
            Some(position(5, 1, 1, 1.05, 2)),
            Some(order(20, 1, 2)),
        ));
        assert!(book.positions.is_empty());
        assert!(book.marks.is_empty());
        assert!(matches!(
            applied.outcome,
            Some(Outcome::Filled { closed: true, .. })
        ));
    }

    #[test]
    fn a_refusal_keeps_its_code_and_drops_the_order() {
        let mut book = AccountBook::default();
        let mut refused = event(7, None, Some(order(30, 2, 3)));
        refused.error_code = Some("NOT_ENOUGH_MONEY".into());
        let applied = book.apply(&refused);
        assert!(matches!(
            applied.outcome,
            Some(Outcome::Refused { code, .. }) if code == "NOT_ENOUGH_MONEY"
        ));
        assert!(book.orders.is_empty());
    }

    #[test]
    fn a_cancel_removes_the_order() {
        let mut book = AccountBook::default();
        book.apply(&event(2, None, Some(order(10, 3, 1))));
        assert_eq!(book.orders.len(), 1);
        book.apply(&event(5, None, Some(order(10, 3, 5))));
        assert!(book.orders.is_empty());
    }

    #[test]
    fn the_profit_follows_the_market_between_answers() {
        let mut book = AccountBook::default();
        book.reconcile(vec![position(5, 1, 1, 1.1000, 1)], vec![]);
        let quotes_then = |_: i64| (Some(1.1002), Some(1.1004));
        book.set_pnl(
            &[PositionUnrealizedPnL {
                position_id: 5,
                gross_unrealized_pnl: 2_000,
                net_unrealized_pnl: 1_800,
            }],
            Some(2),
            &quotes_then,
        );
        assert!((book.net_profit(5, &quotes_then).unwrap() - 18.0).abs() < 1e-6);
        let quotes_now = |_: i64| (Some(1.1010), Some(1.1012));
        assert!((book.net_profit(5, &quotes_now).unwrap() - 98.0).abs() < 1e-6);
        let summary = book.summary(&quotes_now);
        assert!((summary.unrealized - 98.0).abs() < 1e-6);
        assert!((summary.margin - 33.0).abs() < 1e-6);
    }
}

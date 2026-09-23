//! What the account holds, kept current from the server's answers and events: the balance, the
//! open positions, the working orders, the recent deals, and each position's profit.
//!
//! It is plain data: a reconcile answer, an execution event or a profit answer comes in, the
//! state changes, and a [`Notice`] says what the user should be told. The gpui entity around it
//! ([`super::account`]) only fetches and forwards.

use std::collections::{BTreeMap, HashMap};

use wyck::openapi::account::{
    Deal, Order, OrderStatus, OrderType, Position, PositionStatus, PositionUnrealizedPnL,
    TradeSide, Trader, money,
};
use wyck::openapi::trading::{ExecutionEvent, ExecutionType};

use super::math::{self, Contract, PnlMark, Summary};

/// The most recent deals kept for the history.
const MAX_DEALS: usize = 500;

/// How serious a notice is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Success,
    Warning,
    Error,
}

/// Something to tell the user about what happened to their orders.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub tone: Tone,
    pub title: String,
    pub message: String,
}

/// What an execution event changed, beyond the positions and orders themselves.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Applied {
    pub notice: Option<Notice>,
    /// The balance may have changed: ask for the account again.
    pub balance_changed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AccountBook {
    pub trader: Option<Trader>,
    /// The deposit currency, when known: `USD`.
    pub currency: String,
    pub positions: BTreeMap<i64, Position>,
    pub orders: BTreeMap<i64, Order>,
    /// The server's last word on each position's profit.
    pub marks: HashMap<i64, PnlMark>,
    /// Recent deals, newest first.
    pub deals: Vec<Deal>,
    /// How each symbol trades, once the broker said.
    pub contracts: HashMap<i64, Contract>,
    /// The broker's name of each symbol.
    pub names: HashMap<i64, String>,
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
    pub fn name(&self, symbol_id: i64) -> String {
        self.names
            .get(&symbol_id)
            .cloned()
            .unwrap_or_else(|| format!("#{symbol_id}"))
    }

    pub fn contract(&self, symbol_id: i64) -> Contract {
        self.contracts.get(&symbol_id).copied().unwrap_or_default()
    }

    /// Decimals of the account's money.
    pub fn money_digits(&self) -> Option<u32> {
        self.trader.as_ref().and_then(|t| t.digits())
    }

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
        let text = order_text.unwrap_or_default();
        let notice = |tone: Tone, title: &str, message: String| Notice {
            tone,
            title: title.to_owned(),
            message,
        };
        applied.notice = match event.kind() {
            Some(ExecutionType::OrderFilled) => {
                let price = event
                    .deal
                    .as_ref()
                    .and_then(|d| d.execution_price)
                    .map(|p| {
                        let symbol = event.deal.as_ref().map_or(0, |d| d.symbol_id);
                        format!(" at {}", self.contract(symbol).format_price(p))
                    })
                    .unwrap_or_default();
                let closing = event
                    .position
                    .as_ref()
                    .is_some_and(|p| p.status() == Some(PositionStatus::Closed));
                Some(notice(
                    Tone::Success,
                    if closing {
                        "Position closed"
                    } else {
                        "Order filled"
                    },
                    format!("{text}{price}"),
                ))
            }
            Some(ExecutionType::OrderPartialFill) => {
                Some(notice(Tone::Info, "Order partly filled", text))
            }
            Some(ExecutionType::OrderAccepted) => event
                .order
                .as_ref()
                .filter(|o| is_working(o))
                .map(|_| notice(Tone::Info, "Order placed", text)),
            Some(ExecutionType::OrderReplaced) => Some(notice(Tone::Info, "Order changed", text)),
            Some(ExecutionType::OrderCancelled) => {
                Some(notice(Tone::Info, "Order cancelled", text))
            }
            Some(ExecutionType::OrderExpired) => Some(notice(Tone::Warning, "Order expired", text)),
            Some(ExecutionType::OrderRejected | ExecutionType::OrderCancelRejected) => {
                Some(notice(
                    Tone::Error,
                    "Order refused",
                    format!(
                        "{text}{}{}",
                        if text.is_empty() { "" } else { ": " },
                        explain(
                            event
                                .error_code
                                .as_deref()
                                .unwrap_or("refused by the server")
                        )
                    ),
                ))
            }
            Some(ExecutionType::DepositWithdraw | ExecutionType::BonusDepositWithdraw) => {
                applied.balance_changed = true;
                Some(notice(
                    Tone::Info,
                    "Balance changed",
                    "A deposit or withdrawal".into(),
                ))
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
            let quote = self.quote_profit(position, quotes).unwrap_or(0.0);
            self.marks.insert(
                pnl.position_id,
                PnlMark {
                    gross: money(pnl.gross_unrealized_pnl, money_digits),
                    net: money(pnl.net_unrealized_pnl, money_digits),
                    quote,
                },
            );
        }
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

/// A server refusal in words.
pub fn explain(code: &str) -> String {
    match code {
        "NOT_ENOUGH_MONEY" => "not enough free margin".to_owned(),
        "TRADING_BAD_VOLUME" => "the volume is not one the broker accepts".to_owned(),
        "TRADING_BAD_STOPS" => "the stop loss or take profit is not allowed there".to_owned(),
        "TRADING_DISABLED" => "trading is disabled for this symbol or account".to_owned(),
        "MARKET_CLOSED" => "the market is closed".to_owned(),
        "PROTECTION_IS_TOO_CLOSE_TO_MARKET" => {
            "the protection is too close to the price".to_owned()
        }
        "POSITION_NOT_FOUND" => "the position is already closed".to_owned(),
        "ORDER_NOT_FOUND" => "the order no longer exists".to_owned(),
        "MAX_EXPOSURE_REACHED" => "the most this account may hold is reached".to_owned(),
        "ACCOUNT_NOT_AUTHORIZED" | "CH_ACCESS_TOKEN_INVALID" => {
            "this sign-in has no trading permission: disconnect and sign in again".to_owned()
        }
        other => other.to_lowercase().replace('_', " "),
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
        let notice = applied.notice.unwrap();
        assert_eq!(notice.title, "Order filled");
        assert!(
            notice.message.contains("Buy 0.01 EURUSD limit at 1.05000"),
            "{}",
            notice.message
        );
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
                quote: 1.0,
            },
        );
        let applied = book.apply(&event(
            3,
            Some(position(5, 1, 1, 1.05, 2)),
            Some(order(20, 1, 2)),
        ));
        assert!(book.positions.is_empty());
        assert!(book.marks.is_empty());
        assert_eq!(applied.notice.unwrap().title, "Position closed");
    }

    #[test]
    fn a_refusal_is_explained() {
        let mut book = AccountBook::default();
        let mut refused = event(7, None, Some(order(30, 2, 3)));
        refused.error_code = Some("NOT_ENOUGH_MONEY".into());
        let notice = book.apply(&refused).notice.unwrap();
        assert_eq!(notice.tone, Tone::Error);
        assert!(
            notice.message.ends_with("not enough free margin"),
            "{}",
            notice.message
        );
        assert!(book.orders.is_empty());
        assert_eq!(explain("SOMETHING_ELSE"), "something else");
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

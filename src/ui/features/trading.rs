//! Trading from the app: the live account ([`account`]), what it holds and how that changes
//! ([`book`]), the arithmetic of lots, pips and profit ([`math`]), the order ticket ([`ticket`]),
//! and the account panel under the charts ([`panel`]).
//!
//! On the charts, positions, their stop loss and take profit, working orders, alerts and the
//! ticket's pending prices show as lines ([`lines`] builds them); dragging one moves what it
//! stands for.

pub use crate::app::account;
pub use crate::domain::trading::book;
use crate::domain::trading::math;
pub mod panel;
pub mod ticket;

use std::collections::HashMap;

use crate::app::alerts::AlertBook;
use crate::domain::drawings::model::Dash;
use crate::ui::features::chart::{ChartLine, LineId};

use self::book::{AccountBook, is_buy};

/// The lines of every symbol: positions (with their profit), their protection, working orders
/// and their protection, and active alerts. `profit` gives a position's profit now, `currency`
/// the account's.
pub fn lines(
    book: &AccountBook,
    alerts: &AlertBook,
    profit: &dyn Fn(i64) -> Option<f64>,
    currency: &str,
) -> HashMap<i64, Vec<ChartLine>> {
    let palette = crate::ui::kit::theme::colors();
    let mut out: HashMap<i64, Vec<ChartLine>> = HashMap::new();
    let line = |id, price, color, label: String, dash, draggable, closable| ChartLine {
        id,
        price,
        color,
        label,
        detail: None,
        dash,
        draggable,
        closable,
    };
    for position in book.positions.values() {
        let symbol = position.trade_data.symbol_id;
        let contract = book.contract(symbol);
        let side = if is_buy(position.trade_data.trade_side) {
            "Buy"
        } else {
            "Sell"
        };
        let lots = math::format_lots(contract.lots_of_volume(position.trade_data.volume));
        let list = out.entry(symbol).or_default();
        if let Some(price) = position.price {
            let mut entry = line(
                LineId::Position(position.position_id),
                price,
                palette.line,
                format!("{side} {lots}"),
                Dash::Solid,
                false,
                true,
            );
            entry.detail = profit(position.position_id).map(|p| {
                (
                    math::format_money(p, currency),
                    if p >= 0.0 { palette.up } else { palette.down },
                )
            });
            list.push(entry);
            if let Some(sl) = position.stop_loss {
                let mut stop = line(
                    LineId::StopLoss(position.position_id),
                    sl,
                    palette.down,
                    "SL".into(),
                    Dash::Dashed,
                    true,
                    false,
                );
                stop.detail = Some((
                    format!("{:.1} pips", contract.pips((price - sl).abs())),
                    palette.down,
                ));
                list.push(stop);
            }
            if let Some(tp) = position.take_profit {
                let mut target = line(
                    LineId::TakeProfit(position.position_id),
                    tp,
                    palette.up,
                    "TP".into(),
                    Dash::Dashed,
                    true,
                    false,
                );
                target.detail = Some((
                    format!("{:.1} pips", contract.pips((tp - price).abs())),
                    palette.up,
                ));
                list.push(target);
            }
        }
    }
    for order in book.orders.values() {
        let symbol = order.trade_data.symbol_id;
        let contract = book.contract(symbol);
        let side = if is_buy(order.trade_data.trade_side) {
            "Buy"
        } else {
            "Sell"
        };
        let kind = order
            .kind()
            .map_or("order", crate::domain::trading::OrderType::label);
        let lots = math::format_lots(contract.lots_of_volume(order.trade_data.volume));
        let list = out.entry(symbol).or_default();
        if let Some(price) = order.limit_price.or(order.stop_price) {
            list.push(line(
                LineId::Order(order.order_id),
                price,
                palette.amber,
                format!("{side} {kind} {lots}"),
                Dash::Dashed,
                true,
                true,
            ));
        }
        if let Some(sl) = order.stop_loss {
            list.push(line(
                LineId::OrderStopLoss(order.order_id),
                sl,
                palette.down,
                "SL".into(),
                Dash::Dotted,
                true,
                false,
            ));
        }
        if let Some(tp) = order.take_profit {
            list.push(line(
                LineId::OrderTakeProfit(order.order_id),
                tp,
                palette.up,
                "TP".into(),
                Dash::Dotted,
                true,
                false,
            ));
        }
    }
    // Only an alert on a price at a level of its own has a line: the others are in indicator
    // units, or follow a drawing that is shown already.
    for alert in alerts.alerts.iter().filter(|a| {
        a.active
            && a.versus.is_none()
            && matches!(a.source, crate::app::alerts::Source::Price { .. })
            && a.condition.needs_level()
    }) {
        out.entry(alert.symbol_id).or_default().push(line(
            LineId::Alert(alert.id),
            alert.price,
            palette.amber,
            "Alert".into(),
            Dash::Dotted,
            true,
            true,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::alerts::Condition;
    use crate::domain::trading::{Order, Position};
    use serde_json::json;

    #[test]
    fn every_position_order_and_alert_gets_its_lines() {
        let mut book = AccountBook::default();
        let position: Position = serde_json::from_value(json!({
            "positionId": 5,
            "tradeData": {"symbolId": 1, "volume": 10_000_000, "tradeSide": 1},
            "positionStatus": 1,
            "price": 1.1000,
            "stopLoss": 1.0950,
            "takeProfit": 1.1100
        }))
        .unwrap();
        let order: Order = serde_json::from_value(json!({
            "orderId": 9,
            "tradeData": {"symbolId": 2, "volume": 100_000, "tradeSide": 2},
            "orderType": 3,
            "orderStatus": 1,
            "stopPrice": 1.2500,
            "stopLoss": 1.2600
        }))
        .unwrap();
        book.reconcile(vec![position], vec![order]);
        let mut alerts = AlertBook::default();
        alerts.add(1, "EURUSD", 1.1200, Condition::Crossing, 0);
        let lines = lines(&book, &alerts, &|_| Some(-12.5), "USD");

        let first = &lines[&1];
        assert_eq!(first.len(), 4);
        let entry = first.iter().find(|l| l.id == LineId::Position(5)).unwrap();
        assert_eq!(entry.label, "Buy 1");
        assert_eq!(
            entry.detail,
            Some(("-12.50 USD".into(), crate::ui::kit::theme::colors().down))
        );
        assert!(!entry.draggable && entry.closable);
        let stop = first.iter().find(|l| l.id == LineId::StopLoss(5)).unwrap();
        assert!(stop.draggable);
        assert_eq!(stop.detail.as_ref().unwrap().0, "50.0 pips");
        assert!(first.iter().any(|l| l.id == LineId::Alert(1)));

        let second = &lines[&2];
        assert_eq!(second.len(), 2);
        assert_eq!(second[0].label, "Sell stop 0.01");
        assert_eq!(second[1].id, LineId::OrderStopLoss(9));
    }
}

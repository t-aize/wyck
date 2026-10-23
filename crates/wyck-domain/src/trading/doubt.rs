//! An order whose answer never came: what became of it.
//!
//! A timeout is not a refusal. When a new order gets no answer (the call timed out, the connection
//! dropped), the order may have reached the broker. It is never sent again on that doubt; the
//! account is read again and [`InDoubt::find`] says what the reading shows, so the user is told
//! whether the order is working, was probably executed, or is not on the account.

use crate::trading::enums::TradeSide;
use crate::trading::types::{Deal, Order, Position};

/// How far the broker's clock may be behind ours before a trade it made after we sent the order
/// looks older than the order. Wide on purpose: a trade wrongly taken for ours only makes the
/// warning louder, an order wrongly taken for missing could be sent twice.
const CLOCK_SLACK_MS: i64 = 120_000;

/// A new order that was sent and not answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InDoubt {
    /// The id the order was sent with (`guard::client_order_id`).
    pub client_order_id: String,
    pub symbol: i64,
    pub buy: bool,
    pub volume: i64,
    /// When it was sent, in Unix milliseconds.
    pub sent_at: i64,
}

/// What the account shows of an order that was in doubt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// A working order carries its id: it reached the broker and waits.
    Working { order_id: i64 },
    /// A trade of the same symbol, side and volume was made after it was sent: it was probably
    /// executed (a market order leaves no order to find, only its position and its deal).
    Executed { at: i64 },
    /// Nothing of it on the account.
    Missing,
}

impl InDoubt {
    /// What `orders`, `positions` and `deals` (the account as it was read again) show of it.
    pub fn find<'a>(
        &self,
        orders: impl IntoIterator<Item = &'a Order>,
        positions: impl IntoIterator<Item = &'a Position>,
        deals: impl IntoIterator<Item = &'a Deal>,
    ) -> Found {
        if let Some(order) = orders
            .into_iter()
            .find(|o| o.client_order_id.as_deref() == Some(self.client_order_id.as_str()))
        {
            return Found::Working {
                order_id: order.order_id,
            };
        }
        let side = if self.buy {
            TradeSide::Buy
        } else {
            TradeSide::Sell
        }
        .number();
        let side = i64::from(side);
        let since = self.sent_at.saturating_sub(CLOCK_SLACK_MS);
        let opened = positions
            .into_iter()
            .filter(|p| {
                p.trade_data.symbol_id == self.symbol
                    && p.trade_data.trade_side == side
                    && p.trade_data.volume == self.volume
            })
            .filter_map(|p| p.trade_data.open_timestamp)
            .filter(|&at| at >= since)
            .max();
        let dealt = deals
            .into_iter()
            .filter(|d| {
                d.symbol_id == self.symbol
                    && d.trade_side == side
                    && d.filled_volume == self.volume
                    && d.execution_timestamp >= since
            })
            .map(|d| d.execution_timestamp)
            .max();
        match opened.max(dealt) {
            Some(at) => Found::Executed { at },
            None => Found::Missing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doubt() -> InDoubt {
        InDoubt {
            client_order_id: "wyck-1-2-3".to_owned(),
            symbol: 7,
            buy: true,
            volume: 100_000,
            sent_at: 1_000_000,
        }
    }

    fn order(id: i64, client: Option<&str>) -> Order {
        let mut order: Order = serde_json::from_value(serde_json::json!({
            "orderId": id,
            "tradeData": {"symbolId": 7, "volume": 100_000, "tradeSide": 1},
            "orderType": 2, "orderStatus": 1
        }))
        .unwrap();
        order.client_order_id = client.map(str::to_owned);
        order
    }

    fn position(side: i64, volume: i64, opened: i64) -> Position {
        serde_json::from_value(serde_json::json!({
            "positionId": 9,
            "tradeData": {"symbolId": 7, "volume": volume, "tradeSide": side, "openTimestamp": opened},
            "positionStatus": 1
        }))
        .unwrap()
    }

    fn deal(side: i64, volume: i64, at: i64) -> Deal {
        serde_json::from_value(serde_json::json!({
            "dealId": 1, "orderId": 2, "positionId": 3, "volume": volume,
            "filledVolume": volume, "symbolId": 7, "createTimestamp": at,
            "executionTimestamp": at, "tradeSide": side, "dealStatus": 2
        }))
        .unwrap()
    }

    #[test]
    fn a_working_order_is_found_by_its_id() {
        let orders = [order(5, Some("other")), order(6, Some("wyck-1-2-3"))];
        assert_eq!(
            doubt().find(&orders, &[], &[]),
            Found::Working { order_id: 6 }
        );
    }

    #[test]
    fn a_trade_of_the_same_size_after_the_send_counts_as_executed() {
        let positions = [position(1, 100_000, 1_000_500)];
        assert_eq!(
            doubt().find(&[], &positions, &[]),
            Found::Executed { at: 1_000_500 }
        );
        let deals = [deal(1, 100_000, 1_002_000)];
        assert_eq!(
            doubt().find(&[], &[], &deals),
            Found::Executed { at: 1_002_000 }
        );
    }

    #[test]
    fn a_trade_a_little_before_the_send_still_counts_because_the_clocks_differ() {
        let positions = [position(1, 100_000, 1_000_000 - 60_000)];
        assert!(matches!(
            doubt().find(&[], &positions, &[]),
            Found::Executed { .. }
        ));
    }

    #[test]
    fn nothing_that_matches_is_missing() {
        let positions = [
            position(2, 100_000, 1_000_500),
            position(1, 50_000, 1_000_500),
            position(1, 100_000, 1_000_000 - 3_600_000),
        ];
        let deals = [deal(2, 100_000, 1_001_000), deal(1, 100_000, 10)];
        assert_eq!(doubt().find(&[], &positions, &deals), Found::Missing);
        assert_eq!(doubt().find(&[], &[], &[]), Found::Missing);
    }
}

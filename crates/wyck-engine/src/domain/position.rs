//! Positions, pending orders and quotes.

use ctrader_mcp::common::TradeSide;
use serde::{Deserialize, Serialize};

use super::UnixMillis;
use super::volume::Volume;
use crate::ids::{OrderId, PositionId};

/// Trade direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    /// Long.
    Buy,
    /// Short.
    Sell,
}

impl Side {
    /// The opposite direction (the side that closes a position of this side).
    #[must_use]
    pub fn opposite(self) -> Self {
        match self {
            Self::Buy => Self::Sell,
            Self::Sell => Self::Buy,
        }
    }

    /// The `ctrader-mcp` equivalent.
    #[must_use]
    pub fn to_trade_side(self) -> TradeSide {
        match self {
            Self::Buy => TradeSide::Buy,
            Self::Sell => TradeSide::Sell,
        }
    }

    /// Parses a broker string in any case (`"BUY"`, `"Sell"`, ...).
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        TradeSide::parse_any_case(raw).map(Self::from)
    }
}

impl From<TradeSide> for Side {
    fn from(side: TradeSide) -> Self {
        match side {
            TradeSide::Buy => Self::Buy,
            TradeSide::Sell => Self::Sell,
        }
    }
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Buy => "Buy",
            Self::Sell => "Sell",
        })
    }
}

/// An open position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// The broker's position id.
    pub id: PositionId,
    /// The instrument's ticker.
    pub symbol: String,
    /// Long or short.
    pub side: Side,
    /// Open volume.
    pub volume: Volume,
    /// Average entry price.
    pub entry_price: Option<f64>,
    /// Stop loss as an absolute price, if set.
    pub stop_loss: Option<f64>,
    /// Take profit as an absolute price, if set.
    pub take_profit: Option<f64>,
    /// Accumulated swap, in account currency.
    pub swap: Option<f64>,
    /// Accumulated commission, in account currency.
    pub commission: Option<f64>,
    /// Unrealized profit and loss, in account currency, when the broker reports it.
    pub unrealized_pnl: Option<f64>,
    /// The `label` or `comment` the order was placed with, when the broker echoes it. Used
    /// to recognize the engine's own orders after an uncertain result.
    pub label: Option<String>,
}

/// The kind of a pending order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OrderKind {
    /// Buy below or sell above the market.
    Limit,
    /// Buy above or sell below the market.
    Stop,
    /// A stop order that becomes a limit order when triggered.
    StopLimit,
    /// Anything the engine does not model.
    Other,
}

/// A working (unfilled) order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingOrder {
    /// The broker's order id.
    pub id: OrderId,
    /// The instrument's ticker.
    pub symbol: String,
    /// Buy or sell.
    pub side: Side,
    /// Limit, stop, ...
    pub kind: OrderKind,
    /// Order volume.
    pub volume: Volume,
    /// Trigger or limit price, when the broker reports one.
    pub price: Option<f64>,
    /// Stop loss as an absolute price, if set.
    pub stop_loss: Option<f64>,
    /// Take profit as an absolute price, if set.
    pub take_profit: Option<f64>,
}

/// A top-of-book quote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    /// The instrument's ticker.
    pub symbol: String,
    /// Best bid.
    pub bid: f64,
    /// Best ask.
    pub ask: f64,
    /// When the broker stamped it, in Unix milliseconds, if known.
    pub timestamp: Option<UnixMillis>,
}

impl Quote {
    /// The price a market order on `side` would trade at: the ask for a buy, the bid for a
    /// sell.
    #[must_use]
    pub fn price_for(&self, side: Side) -> f64 {
        match side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        }
    }

    /// The midpoint.
    #[must_use]
    pub fn mid(&self) -> f64 {
        (self.bid + self.ask) / 2.0
    }

    /// Ask minus bid.
    #[must_use]
    pub fn spread(&self) -> f64 {
        self.ask - self.bid
    }

    /// Whether the quote is usable: both sides finite and positive, and not crossed.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.bid.is_finite() && self.ask.is_finite() && self.bid > 0.0 && self.ask >= self.bid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_round_trips_with_trade_side() {
        for side in [Side::Buy, Side::Sell] {
            assert_eq!(Side::from(side.to_trade_side()), side);
            assert_eq!(side.opposite().opposite(), side);
        }
        assert_eq!(Side::parse("BUY"), Some(Side::Buy));
        assert_eq!(Side::parse("sell"), Some(Side::Sell));
        assert_eq!(Side::parse("hold"), None);
    }

    #[test]
    fn quote_prices_follow_the_side() {
        let q = Quote {
            symbol: "EURUSD".into(),
            bid: 1.0849,
            ask: 1.0851,
            timestamp: None,
        };
        assert_eq!(q.price_for(Side::Buy), 1.0851);
        assert_eq!(q.price_for(Side::Sell), 1.0849);
        assert!((q.mid() - 1.0850).abs() < 1e-12);
        assert!((q.spread() - 0.0002).abs() < 1e-12);
        assert!(q.is_valid());
    }

    #[test]
    fn invalid_quotes_are_detected() {
        let mut q = Quote {
            symbol: "X".into(),
            bid: 1.0,
            ask: 1.1,
            timestamp: None,
        };
        assert!(q.is_valid());
        q.ask = 0.9;
        assert!(!q.is_valid(), "crossed");
        q.ask = f64::NAN;
        assert!(!q.is_valid());
        q.ask = 1.1;
        q.bid = 0.0;
        assert!(!q.is_valid());
    }
}

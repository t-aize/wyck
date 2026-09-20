//! Price history: one bar of it.

use serde::{Deserialize, Serialize};

use super::UnixMillis;

/// One bar of price history. Prices are display values, as everywhere in the domain.
///
/// `time` is when the bar opens. Both servers report bars that way, and the chart relies on it
/// to place the bar and to know when a new one starts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Candle {
    /// When the bar opens, in Unix milliseconds.
    pub time: UnixMillis,
    /// First price of the bar.
    pub open: f64,
    /// Highest price of the bar.
    pub high: f64,
    /// Lowest price of the bar.
    pub low: f64,
    /// Last price of the bar.
    pub close: f64,
    /// Traded volume, in whatever unit the server counts (Remote and Local differ).
    pub volume: f64,
}

impl Candle {
    /// A bar that has seen a single price.
    #[must_use]
    pub fn at(time: UnixMillis, price: f64) -> Self {
        Self {
            time,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: 0.0,
        }
    }

    /// Folds a later price into the bar: it becomes the close and may stretch the range.
    pub fn apply_price(&mut self, price: f64) {
        self.high = self.high.max(price);
        self.low = self.low.min(price);
        self.close = price;
    }

    /// Whether the close is at or above the open.
    #[must_use]
    pub fn is_up(&self) -> bool {
        self.close >= self.open
    }

    /// Whether the numbers can be drawn: all finite, and the range holds the body.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        let all = [self.open, self.high, self.low, self.close];
        all.iter().all(|v| v.is_finite())
            && self.low <= self.high
            && self.low <= self.open.min(self.close)
            && self.high >= self.open.max(self.close)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_price_stretches_the_range_and_becomes_the_close() {
        let mut bar = Candle::at(0, 1.0);
        bar.apply_price(1.2);
        bar.apply_price(0.9);
        bar.apply_price(1.1);
        assert_eq!(
            (bar.open, bar.high, bar.low, bar.close),
            (1.0, 1.2, 0.9, 1.1)
        );
        assert!(bar.is_up());
    }

    #[test]
    fn a_broken_bar_is_not_sane() {
        let mut bar = Candle::at(0, 1.0);
        assert!(bar.is_sane());
        bar.high = 0.5;
        assert!(!bar.is_sane());
        bar.high = f64::NAN;
        assert!(!bar.is_sane());
    }
}

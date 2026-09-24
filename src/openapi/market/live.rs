//! Live bars with their real close price.
//!
//! The live bars a [`SpotEvent`] carries (after
//! [`crate::openapi::market::MarketClient::subscribe_live_bars`]) come with a wrong close: the
//! server always sends a close equal to the bar's low. This was seen on a live demo account, where
//! the close of the live bar stayed at its low while the bid moved. The open, high, low and tick
//! count are right.
//!
//! [`LiveBarTracker`] repairs that: the close of a live bar is the bid of the event that carries
//! it, and when the event carries no bid (only the ask moved), the close already known for that
//! same bar. A bar seen for the first time without a bid has just opened, so its close is its
//! open. The high and low are then stretched to hold the close, so the bar stays sane.
//!
//! Like [`crate::openapi::market::SpotTracker`], it does no I/O: feed it the price events and read
//! the corrected bars back.

use std::collections::HashMap;

use super::bars::{Bar, Period};
use super::quotes::SpotEvent;

/// Corrects the live bars of price events. Keeps the latest corrected bar of every symbol and
/// period seen.
#[derive(Debug, Clone, Default)]
pub struct LiveBarTracker {
    latest: HashMap<(i64, Period), Bar>,
}

impl LiveBarTracker {
    /// An empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The live bars of `event`, with their periods and the real close, oldest period first as
    /// the event lists them. Also remembers them for the next event of the same symbol.
    pub fn apply(&mut self, event: &SpotEvent) -> Vec<(Period, Bar)> {
        let mut out = Vec::new();
        for (period, bar) in event.live_bars() {
            let key = (event.symbol_id, period);
            let fixed = with_true_close(bar, self.latest.get(&key), event.bid);
            self.latest.insert(key, fixed);
            out.push((period, fixed));
        }
        out
    }

    /// The latest corrected bar of a symbol and period.
    #[must_use]
    pub fn get(&self, symbol_id: i64, period: Period) -> Option<Bar> {
        self.latest.get(&(symbol_id, period)).copied()
    }

    /// Forgets a symbol and period, after unsubscribing from its live bars.
    pub fn forget(&mut self, symbol_id: i64, period: Period) {
        self.latest.remove(&(symbol_id, period));
    }

    /// Forgets everything, after a reconnect.
    pub fn clear(&mut self) {
        self.latest.clear();
    }
}

/// A live bar with the close it should have: `bid` when the event carried one, else the close of
/// `known` when it is the same bar, else the open. High and low are stretched to hold the close.
#[must_use]
pub fn with_true_close(mut live: Bar, known: Option<&Bar>, bid: Option<i64>) -> Bar {
    live.close = bid
        .or_else(|| known.filter(|b| b.time_ms == live.time_ms).map(|b| b.close))
        .unwrap_or(live.open);
    live.high = live.high.max(live.close);
    live.low = live.low.min(live.close);
    live
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bar(time_ms: i64, o: i64, h: i64, l: i64, c: i64, volume: i64) -> Bar {
        Bar {
            time_ms,
            open: o,
            high: h,
            low: l,
            close: c,
            volume,
        }
    }

    #[test]
    fn a_live_bar_gets_its_close_from_the_bid_not_from_the_low() {
        // As the server sent it: close equal to the low.
        let live = bar(0, 100, 130, 90, 90, 5);
        assert_eq!(with_true_close(live, None, Some(120)).close, 120);
        let held = bar(0, 100, 130, 90, 115, 4);
        assert_eq!(with_true_close(live, Some(&held), None).close, 115);
        let other = bar(-60, 1, 1, 1, 1, 1);
        assert_eq!(with_true_close(live, Some(&other), None).close, 100);
        let stretched = with_true_close(live, None, Some(140));
        assert_eq!((stretched.high, stretched.low), (140, 90));
        assert!(stretched.is_sane());
    }

    fn event(bid: Option<i64>, low: i64, minutes: i64) -> SpotEvent {
        serde_json::from_value(json!({
            "symbolId": 7,
            "bid": bid,
            "trendbar": [{
                "volume": 3, "period": 1, "low": low, "deltaOpen": 5, "deltaClose": 0,
                "deltaHigh": 20, "utcTimestampInMinutes": minutes
            }]
        }))
        .unwrap()
    }

    #[test]
    fn an_event_without_a_bid_keeps_the_close_already_known() {
        let mut tracker = LiveBarTracker::new();
        let first = tracker.apply(&event(Some(112), 100, 10));
        assert_eq!(first[0].1.close, 112);
        // Only the ask moved: the bar keeps the close it had.
        let second = tracker.apply(&event(None, 100, 10));
        assert_eq!(second[0].1.close, 112);
        assert_eq!(tracker.get(7, Period::M1).unwrap().close, 112);
        // A new bar without a bid has just opened.
        let third = tracker.apply(&event(None, 100, 11));
        assert_eq!(third[0].1.close, 105);
        tracker.forget(7, Period::M1);
        assert!(tracker.get(7, Period::M1).is_none());
    }

    #[test]
    fn symbols_do_not_share_their_bars() {
        let mut tracker = LiveBarTracker::new();
        tracker.apply(&event(Some(112), 100, 10));
        let mut other = event(None, 100, 10);
        other.symbol_id = 8;
        assert_eq!(tracker.apply(&other)[0].1.close, 105);
        tracker.clear();
        assert!(tracker.get(7, Period::M1).is_none());
    }
}

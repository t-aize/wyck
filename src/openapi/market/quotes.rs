//! Live prices: the two-sided [`Spot`], the wire [`SpotEvent`] that only carries the side that
//! changed, and [`SpotTracker`], which carries the other side forward into a full quote.
//!
//! None of this does I/O. Feed it the events of [`crate::openapi::Client::events`] and read it back.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::bars::{Bar, Period, WireTrendbar, decode_bar};
use crate::openapi::event::Event;
use crate::openapi::transport::wire::flex;

/// A live price change: what one `ProtoOASpotEvent` says about a symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    /// The symbol.
    pub symbol_id: i64,
    /// The bid, when this event changed it.
    pub bid: Option<i64>,
    /// The ask, when this event changed it.
    pub ask: Option<i64>,
    /// The server's time in Unix milliseconds, when it was asked to give one.
    pub time_ms: Option<i64>,
}

impl From<&SpotEvent> for Spot {
    fn from(event: &SpotEvent) -> Self {
        Self {
            symbol_id: event.symbol_id,
            bid: event.bid,
            ask: event.ask,
            time_ms: event.timestamp,
        }
    }
}

/// `ProtoOASpotEvent`: a new price, or the first one after a subscription.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotEvent {
    /// The trading account id.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    /// The symbol.
    #[serde(deserialize_with = "flex::int")]
    pub symbol_id: i64,
    /// The new bid, when it changed.
    #[serde(default, deserialize_with = "flex::opt")]
    pub bid: Option<i64>,
    /// The new ask, when it changed.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ask: Option<i64>,
    /// Updated live bars, when a live bar subscription exists.
    #[serde(default)]
    pub trendbar: Vec<WireTrendbar>,
    /// The close price of the last session.
    #[serde(default, deserialize_with = "flex::opt")]
    pub session_close: Option<i64>,
    /// When the server made the event, in Unix milliseconds, if it was asked to say.
    #[serde(default, deserialize_with = "flex::opt")]
    pub timestamp: Option<i64>,
}

impl SpotEvent {
    /// The live bars the event carries, decoded, with their periods. A bar without a known period
    /// or with damaged numbers is left out. Live bars only arrive after
    /// [`crate::openapi::market::MarketClient::subscribe_live_bars`].
    ///
    /// These are the bars exactly as sent, and the server's close of a live bar is wrong (it
    /// equals the low). Use [`crate::openapi::market::LiveBarTracker`] to get the real close.
    #[must_use]
    pub fn live_bars(&self) -> Vec<(Period, Bar)> {
        self.trendbar.iter().filter_map(live_bar).collect()
    }
}

fn live_bar(wire: &WireTrendbar) -> Option<(Period, Bar)> {
    let period = Period::from_number(wire.period.unwrap_or(1))?;
    Some((period, decode_bar(wire)?))
}

/// `ProtoOASubscribeSpotsReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeSpotsReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The symbols to follow.
    pub symbol_id: Vec<i64>,
    /// Ask for the server time of each spot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe_to_spot_timestamp: Option<bool>,
}

/// Keeps the latest bid and ask of every symbol seen.
///
/// A [`SpotEvent`] carries the side that changed, so a program that wants a full quote has to carry
/// the other side forward. [`SpotTracker::apply`] does that and returns the complete [`Spot`].
#[derive(Debug, Clone, Default)]
pub struct SpotTracker {
    latest: HashMap<i64, Spot>,
}

impl SpotTracker {
    /// An empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Folds one event in and returns the symbol's quote after it: the sides that changed, the
    /// others as last seen. Its time is the event's when it has one, else the previous one.
    pub fn apply(&mut self, event: &SpotEvent) -> Spot {
        let entry = self.latest.entry(event.symbol_id).or_insert(Spot {
            symbol_id: event.symbol_id,
            bid: None,
            ask: None,
            time_ms: None,
        });
        entry.bid = event.bid.or(entry.bid);
        entry.ask = event.ask.or(entry.ask);
        entry.time_ms = event.timestamp.or(entry.time_ms);
        *entry
    }

    /// Folds an [`Event`] in when it is a price event; returns the quote, or `None` for any other
    /// event.
    pub fn apply_event(&mut self, event: &Event) -> Option<Spot> {
        match event {
            Event::Spot(spot) => Some(self.apply(spot)),
            _ => None,
        }
    }

    /// The latest quote of a symbol.
    #[must_use]
    pub fn get(&self, symbol_id: i64) -> Option<Spot> {
        self.latest.get(&symbol_id).copied()
    }

    /// How many symbols have a quote.
    #[must_use]
    pub fn len(&self) -> usize {
        self.latest.len()
    }

    /// Whether nothing has been seen.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.latest.is_empty()
    }

    /// Forgets a symbol, after unsubscribing from it.
    pub fn forget(&mut self, symbol_id: i64) {
        self.latest.remove(&symbol_id);
    }

    /// Forgets everything, after a reconnect (the first event after a new subscription carries the
    /// latest prices again).
    pub fn clear(&mut self) {
        self.latest.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spot(symbol_id: i64, bid: Option<i64>, ask: Option<i64>, time: Option<i64>) -> SpotEvent {
        SpotEvent {
            ctid_trader_account_id: Some(1),
            symbol_id,
            bid,
            ask,
            trendbar: vec![],
            session_close: None,
            timestamp: time,
        }
    }

    #[test]
    fn a_spot_event_may_carry_only_one_side() {
        let e: SpotEvent =
            serde_json::from_value(json!({"symbolId": 1, "bid": 108499, "timestamp": 99})).unwrap();
        assert_eq!((e.bid, e.ask, e.timestamp), (Some(108_499), None, Some(99)));
        assert!(e.trendbar.is_empty());
    }

    #[test]
    fn the_other_side_is_carried_forward() {
        let mut tracker = SpotTracker::new();
        let first = tracker.apply(&spot(1, Some(100), None, Some(10)));
        assert_eq!((first.bid, first.ask), (Some(100), None));
        let second = tracker.apply(&spot(1, None, Some(102), None));
        assert_eq!(
            (second.bid, second.ask, second.time_ms),
            (Some(100), Some(102), Some(10))
        );
        let third = tracker.apply(&spot(1, Some(101), None, Some(30)));
        assert_eq!(
            (third.bid, third.ask, third.time_ms),
            (Some(101), Some(102), Some(30))
        );
        assert_eq!(tracker.get(1), Some(third));
    }

    #[test]
    fn symbols_are_tracked_independently() {
        let mut tracker = SpotTracker::new();
        tracker.apply(&spot(1, Some(1), Some(2), None));
        tracker.apply(&spot(2, Some(10), Some(20), None));
        assert_eq!(tracker.len(), 2);
        assert_eq!(tracker.get(1).unwrap().bid, Some(1));
        tracker.forget(1);
        assert!(tracker.get(1).is_none());
        tracker.clear();
        assert!(tracker.is_empty());
    }

    #[test]
    fn only_price_events_update_the_tracker() {
        let mut tracker = SpotTracker::new();
        assert!(
            tracker
                .apply_event(&Event::Spot(spot(1, Some(5), Some(6), None)))
                .is_some()
        );
        assert!(
            tracker
                .apply_event(&Event::Disconnected(
                    crate::openapi::event::DisconnectReason::ClosedByServer
                ))
                .is_none()
        );
        assert_eq!(tracker.len(), 1);
    }

    #[test]
    fn live_bars_are_decoded_with_their_period() {
        let event: SpotEvent = serde_json::from_value(json!({
            "symbolId": 1,
            "trendbar": [
                {"volume": 3, "period": 1, "low": 100, "deltaOpen": 1, "deltaClose": 2, "deltaHigh": 3, "utcTimestampInMinutes": 500},
                {"volume": 9, "period": 9, "low": 90, "deltaOpen": 0, "deltaClose": 0, "deltaHigh": 5, "utcTimestampInMinutes": 480},
                {"volume": 1, "period": 77, "low": 1, "utcTimestampInMinutes": 1},
                {"volume": 1, "period": 1}
            ]
        }))
        .unwrap();
        let bars = event.live_bars();
        assert_eq!(
            bars.len(),
            2,
            "an unknown period and a bar without a low are dropped"
        );
        assert_eq!(bars[0].0, Period::M1);
        assert_eq!(
            (bars[0].1.open, bars[0].1.high, bars[0].1.close),
            (101, 103, 102)
        );
        assert_eq!(bars[1].0, Period::H1);
    }

    #[test]
    fn a_spot_event_gives_a_spot() {
        let event = spot(4, Some(10), None, Some(99));
        assert_eq!(
            Spot::from(&event),
            Spot {
                symbol_id: 4,
                bid: Some(10),
                ask: None,
                time_ms: Some(99)
            }
        );
    }
}

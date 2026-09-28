//! Bars (trendbars): the period enum, the readable [`Bar`], and decoding the server's low-plus-
//! offsets wire form.
//!
//! A bar's time is in Unix minutes on the wire and its prices are a low plus three offsets
//! (`open`, `close`, `high`), each a difference from the low; [`decode_bar`] turns that into plain
//! numbers and Unix milliseconds, and refuses a bar whose numbers do not hold together
//! ([`Bar::is_sane`]) so a damaged bar never reaches a chart.

use serde::{Deserialize, Serialize};

use crate::transport::wire::flex;

/// The period of a bar. The numbers are those of the official `ProtoOATrendbarPeriod` enum.
///
/// The Open API offers more periods than the MCP servers do (2, 3, 4 and 10 minutes, and 12 hours).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Period {
    /// One minute.
    M1 = 1,
    /// Two minutes.
    M2 = 2,
    /// Three minutes.
    M3 = 3,
    /// Four minutes.
    M4 = 4,
    /// Five minutes.
    M5 = 5,
    /// Ten minutes.
    M10 = 6,
    /// Fifteen minutes.
    M15 = 7,
    /// Thirty minutes.
    M30 = 8,
    /// One hour.
    H1 = 9,
    /// Four hours.
    H4 = 10,
    /// Twelve hours.
    H12 = 11,
    /// One day.
    D1 = 12,
    /// One week.
    W1 = 13,
    /// One month.
    MN1 = 14,
}

impl Period {
    /// Every period, shortest first.
    pub const ALL: [Self; 14] = [
        Self::M1,
        Self::M2,
        Self::M3,
        Self::M4,
        Self::M5,
        Self::M10,
        Self::M15,
        Self::M30,
        Self::H1,
        Self::H4,
        Self::H12,
        Self::D1,
        Self::W1,
        Self::MN1,
    ];

    /// The number the server uses.
    #[must_use]
    pub fn number(self) -> i32 {
        self as i32
    }

    /// The period with the server number `number`, if there is one.
    #[must_use]
    pub fn from_number(number: i64) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| i64::from(p.number()) == number)
    }

    /// The length in minutes. A month is taken as 30 days, which is only good for sizing.
    #[must_use]
    pub fn minutes(self) -> i64 {
        match self {
            Self::M1 => 1,
            Self::M2 => 2,
            Self::M3 => 3,
            Self::M4 => 4,
            Self::M5 => 5,
            Self::M10 => 10,
            Self::M15 => 15,
            Self::M30 => 30,
            Self::H1 => 60,
            Self::H4 => 240,
            Self::H12 => 720,
            Self::D1 => 1_440,
            Self::W1 => 10_080,
            Self::MN1 => 43_200,
        }
    }

    /// The length in milliseconds (a month as 30 days).
    #[must_use]
    pub fn millis(self) -> i64 {
        self.minutes() * 60_000
    }

    /// The short name, for example `M15` or `H4`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::M1 => "M1",
            Self::M2 => "M2",
            Self::M3 => "M3",
            Self::M4 => "M4",
            Self::M5 => "M5",
            Self::M10 => "M10",
            Self::M15 => "M15",
            Self::M30 => "M30",
            Self::H1 => "H1",
            Self::H4 => "H4",
            Self::H12 => "H12",
            Self::D1 => "D1",
            Self::W1 => "W1",
            Self::MN1 => "MN1",
        }
    }
}

/// A bar with real values. Prices are the server's raw integers, see [`crate::market::to_price`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bar {
    /// When the bar opens, in Unix milliseconds.
    pub time_ms: i64,
    /// First price.
    pub open: i64,
    /// Highest price.
    pub high: i64,
    /// Lowest price.
    pub low: i64,
    /// Last price.
    pub close: i64,
    /// Volume in ticks: how many price changes the bar saw. It is not a traded amount.
    pub volume: i64,
}

impl Bar {
    /// Whether the numbers hold together: the range contains the body.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.low <= self.open.min(self.close) && self.high >= self.open.max(self.close)
    }
}

/// A bar as the server sends it (`ProtoOATrendbar`): the low is the base, the rest are offsets
/// from it. See [`Bar`] for the readable form.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireTrendbar {
    /// Volume in ticks: how many ticks the bar saw.
    #[serde(deserialize_with = "flex::int")]
    pub volume: i64,
    /// The bar period, as its number.
    #[serde(default, deserialize_with = "flex::opt")]
    pub period: Option<i64>,
    /// The low price.
    #[serde(default, deserialize_with = "flex::opt")]
    pub low: Option<i64>,
    /// `open - low`.
    #[serde(default, deserialize_with = "flex::opt")]
    pub delta_open: Option<i64>,
    /// `close - low`.
    #[serde(default, deserialize_with = "flex::opt")]
    pub delta_close: Option<i64>,
    /// `high - low`.
    #[serde(default, deserialize_with = "flex::opt")]
    pub delta_high: Option<i64>,
    /// The open time of the bar, in Unix minutes.
    #[serde(default, deserialize_with = "flex::opt")]
    pub utc_timestamp_in_minutes: Option<i64>,
}

/// Reads a bar from its wire form: `open = low + deltaOpen`, `close = low + deltaClose`,
/// `high = low + deltaHigh`, and the time is in Unix minutes.
///
/// Returns `None` for a bar that lacks its low price or its time, or whose numbers do not hold
/// together, so a damaged bar never reaches a chart.
#[must_use]
pub fn decode_bar(wire: &WireTrendbar) -> Option<Bar> {
    let low = wire.low?;
    let minutes = wire.utc_timestamp_in_minutes?;
    let bar = Bar {
        time_ms: minutes.checked_mul(60_000)?,
        open: low.checked_add(wire.delta_open.unwrap_or(0))?,
        high: low.checked_add(wire.delta_high.unwrap_or(0))?,
        low,
        close: low.checked_add(wire.delta_close.unwrap_or(0))?,
        volume: wire.volume,
    };
    bar.is_sane().then_some(bar)
}

/// The bars of an answer: decoded, damaged ones dropped, sorted oldest first, one per open time.
#[must_use]
pub fn decode_bars(wire: &[WireTrendbar]) -> Vec<Bar> {
    let mut bars: Vec<Bar> = wire.iter().filter_map(decode_bar).collect();
    bars.sort_by_key(|b| b.time_ms);
    bars.dedup_by_key(|b| b.time_ms);
    bars
}

/// `ProtoOASubscribeLiveTrendbarReq` and its unsubscribe twin.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTrendbarReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The bar period, as its number.
    pub period: i32,
    /// The symbol.
    pub symbol_id: i64,
}

/// `ProtoOAGetTrendbarsReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTrendbarsReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// Start of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<i64>,
    /// End of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<i64>,
    /// The bar period, as its number.
    pub period: i32,
    /// The symbol.
    pub symbol_id: i64,
    /// The most bars to return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

/// `ProtoOAGetTrendbarsRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTrendbarsRes {
    /// The bars.
    #[serde(default)]
    pub trendbar: Vec<WireTrendbar>,
    /// Whether more bars exist in the range than were returned.
    #[serde(default)]
    pub has_more: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wire_bar(low: i64, open: i64, close: i64, high: i64, minutes: i64) -> WireTrendbar {
        WireTrendbar {
            volume: 42,
            period: Some(1),
            low: Some(low),
            delta_open: Some(open - low),
            delta_close: Some(close - low),
            delta_high: Some(high - low),
            utc_timestamp_in_minutes: Some(minutes),
        }
    }

    #[test]
    fn periods_round_trip_through_their_server_number() {
        for period in Period::ALL {
            assert_eq!(
                Period::from_number(i64::from(period.number())),
                Some(period)
            );
        }
        assert_eq!(Period::from_number(0), None);
        assert_eq!(Period::from_number(15), None);
        assert_eq!(Period::M5.number(), 5);
        assert_eq!(Period::M10.number(), 6, "the numbers skip: M10 is 6");
        assert_eq!(Period::MN1.number(), 14);
    }

    #[test]
    fn period_lengths_grow_and_labels_are_unique() {
        assert!(
            Period::ALL
                .windows(2)
                .all(|w| w[0].minutes() < w[1].minutes())
        );
        assert_eq!(Period::H4.millis(), 4 * 60 * 60_000);
        let mut labels: Vec<_> = Period::ALL.iter().map(|p| p.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), Period::ALL.len());
    }

    #[test]
    fn a_bar_is_rebuilt_from_its_low_and_offsets() {
        let bar = decode_bar(&wire_bar(108_400, 108_450, 108_480, 108_500, 29_000_000)).unwrap();
        assert_eq!(
            (bar.open, bar.high, bar.low, bar.close),
            (108_450, 108_500, 108_400, 108_480)
        );
        assert_eq!(bar.time_ms, 29_000_000 * 60_000);
        assert_eq!(bar.volume, 42);
    }

    #[test]
    fn a_damaged_bar_is_dropped_not_drawn() {
        let mut no_low = wire_bar(1, 1, 1, 1, 1);
        no_low.low = None;
        assert!(decode_bar(&no_low).is_none());
        let mut no_time = wire_bar(1, 1, 1, 1, 1);
        no_time.utc_timestamp_in_minutes = None;
        assert!(decode_bar(&no_time).is_none());
        // A high below the body cannot be a bar.
        let broken = wire_bar(100, 150, 160, 120, 5);
        assert!(decode_bar(&broken).is_none());
        // An offset that would overflow is refused, not wrapped.
        let mut huge = wire_bar(i64::MAX - 1, i64::MAX - 1, i64::MAX - 1, i64::MAX - 1, 1);
        huge.delta_high = Some(10);
        assert!(decode_bar(&huge).is_none());
    }

    #[test]
    fn bars_come_out_sorted_and_unique() {
        let wire = [
            wire_bar(10, 10, 10, 10, 5),
            wire_bar(10, 10, 10, 10, 3),
            wire_bar(10, 10, 10, 10, 5),
            wire_bar(50, 10, 10, 10, 4),
        ];
        let bars = decode_bars(&wire);
        let times: Vec<i64> = bars.iter().map(|b| b.time_ms / 60_000).collect();
        assert_eq!(
            times,
            vec![3, 5],
            "the broken bar is gone, the duplicate merged"
        );
    }

    #[test]
    fn requests_use_the_official_field_names_and_skip_missing_options() {
        let req = GetTrendbarsReq {
            ctid_trader_account_id: 7,
            from_timestamp: Some(1),
            to_timestamp: None,
            period: 1,
            symbol_id: 3,
            count: None,
        };
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
            json!({"ctidTraderAccountId": 7, "fromTimestamp": 1, "period": 1, "symbolId": 3})
        );
    }
}

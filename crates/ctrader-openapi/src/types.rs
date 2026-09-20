//! Readable data: periods, bars, ticks, and how to decode what the server sends.
//!
//! The server speaks in raw integers. This module holds the small types the rest of the crate
//! (and its callers) work with, and the pure functions that turn the wire form into them:
//!
//! - **Prices** are integers scaled by [`PRICE_SCALE`] (100 000): the price `1.08501` travels as
//!   `108501`. [`to_price`] and [`from_price`] convert. Keep the integer for exact work and
//!   comparisons; the `f64` is for display.
//! - **Bars** come as a low price plus offsets ([`decode_bar`]); their time is in Unix minutes.
//! - **Ticks** come newest first with their times and prices as differences ([`decode_ticks`]).

use crate::model::{WireTick, WireTrendbar};

/// The factor between the server's integer prices and real prices.
pub const PRICE_SCALE: i64 = 100_000;

/// The raw price `raw` as a real price.
#[must_use]
pub fn to_price(raw: i64) -> f64 {
    raw as f64 / PRICE_SCALE as f64
}

/// A real price as the server's integer, rounded to the nearest unit.
#[must_use]
pub fn from_price(price: f64) -> i64 {
    (price * PRICE_SCALE as f64).round() as i64
}

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

/// Which side of the market a tick history is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuoteType {
    /// The bid: what a seller receives.
    Bid = 1,
    /// The ask: what a buyer pays.
    Ask = 2,
}

impl QuoteType {
    /// The number the server uses.
    #[must_use]
    pub fn number(self) -> i32 {
        self as i32
    }
}

/// A bar with real values. Prices are the server's raw integers, see [`to_price`].
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

/// One tick with an absolute time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    /// When the tick happened, in Unix milliseconds.
    pub time_ms: i64,
    /// The raw price, see [`to_price`].
    pub price: i64,
}

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

/// Turns the ticks of a `ProtoOAGetTickDataRes` into ticks with absolute times, oldest first.
///
/// The server sends them **newest first**. The first tick is absolute: its time is in Unix
/// milliseconds and its price is the price. Every next tick is a **difference** from the one before it
/// in the list, in both fields (going back in time the time difference is negative), so the real
/// time and the real price are running sums. The `.proto` comment only says so for the time; that
/// the price is a difference too was seen on a live demo account (a tick history whose oldest
/// prices read `1` and `-1` beside a newest price of `114880`). Reading the times as absolute yields
/// a few seconds of data where an hour was asked for, and the prices as absolute yields nonsense.
///
/// The result is sorted and ticks that share a time and a price are kept (two identical ticks are
/// real).
#[must_use]
pub fn decode_ticks(wire: &[WireTick]) -> Vec<Tick> {
    let (mut time, mut price) = (0i64, 0i64);
    let mut ticks: Vec<Tick> = wire
        .iter()
        .map(|tick| {
            // The first tick is absolute and the running sums start at zero, so the same
            // addition serves every tick.
            time = time.saturating_add(tick.timestamp);
            price = price.saturating_add(tick.tick);
            Tick {
                time_ms: time,
                price,
            }
        })
        .collect();
    ticks.sort_by_key(|t| t.time_ms);
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn prices_convert_both_ways_without_drift() {
        assert_eq!(to_price(108_501), 1.08501);
        assert_eq!(from_price(1.08501), 108_501);
        assert_eq!(from_price(to_price(2_965_960)), 2_965_960);
        assert_eq!(from_price(-0.5), -50_000);
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
    fn ticks_are_decoded_from_newest_first_deltas_in_time_and_price() {
        let wire = [
            WireTick {
                timestamp: 1_000_000,
                tick: 108_501,
            },
            WireTick {
                timestamp: -500,
                tick: -2,
            },
            WireTick {
                timestamp: -250,
                tick: 1,
            },
        ];
        let ticks = decode_ticks(&wire);
        assert_eq!(
            ticks,
            vec![
                Tick {
                    time_ms: 999_250,
                    price: 108_500
                },
                Tick {
                    time_ms: 999_500,
                    price: 108_499
                },
                Tick {
                    time_ms: 1_000_000,
                    price: 108_501
                },
            ]
        );
    }

    #[test]
    fn a_live_shaped_history_gives_sensible_prices_at_every_end() {
        // Shaped like the first live answer: the newest price absolute, the rest small steps.
        let wire = [
            WireTick {
                timestamp: 1_789_763_387_232,
                tick: 114_880,
            },
            WireTick {
                timestamp: -651,
                tick: -1,
            },
            WireTick {
                timestamp: -1_020,
                tick: 2,
            },
            WireTick {
                timestamp: -300,
                tick: -1,
            },
        ];
        let ticks = decode_ticks(&wire);
        assert_eq!(ticks.last().unwrap().price, 114_880);
        for tick in &ticks {
            assert!((114_870..=114_890).contains(&tick.price), "{tick:?}");
        }
        assert!(ticks.windows(2).all(|w| w[0].time_ms <= w[1].time_ms));
    }

    #[test]
    fn the_one_tick_case_and_the_empty_case() {
        assert!(decode_ticks(&[]).is_empty());
        let one = decode_ticks(&[WireTick {
            timestamp: 77,
            tick: 5,
        }]);
        assert_eq!(
            one,
            vec![Tick {
                time_ms: 77,
                price: 5
            }]
        );
    }

    #[test]
    fn two_ticks_at_the_same_millisecond_are_both_kept() {
        let wire = [
            WireTick {
                timestamp: 1_000,
                tick: 10,
            },
            WireTick {
                timestamp: 0,
                tick: 11,
            },
        ];
        assert_eq!(decode_ticks(&wire).len(), 2);
    }
}

/// A price update with both sides, built from bid and ask ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quote {
    /// When it happened, in Unix milliseconds.
    pub time_ms: i64,
    /// The bid at that time, once one has been seen.
    pub bid: Option<i64>,
    /// The ask at that time, once one has been seen.
    pub ask: Option<i64>,
}

/// Joins a list of bid ticks and a list of ask ticks (each oldest first) into quotes, one per
/// distinct time. A side that did not tick at that time keeps its last known value; before a side
/// has ticked at all it is `None`.
#[must_use]
pub fn merge_sides(bids: &[Tick], asks: &[Tick]) -> Vec<Quote> {
    let (mut b, mut a) = (0, 0);
    let (mut last_bid, mut last_ask) = (None, None);
    let mut quotes: Vec<Quote> = Vec::with_capacity(bids.len() + asks.len());
    while b < bids.len() || a < asks.len() {
        let time = match (bids.get(b), asks.get(a)) {
            (Some(x), Some(y)) => x.time_ms.min(y.time_ms),
            (Some(x), None) => x.time_ms,
            (None, Some(y)) => y.time_ms,
            (None, None) => break,
        };
        // All ticks of that millisecond count; the last of each side wins.
        while bids.get(b).is_some_and(|t| t.time_ms == time) {
            last_bid = Some(bids[b].price);
            b += 1;
        }
        while asks.get(a).is_some_and(|t| t.time_ms == time) {
            last_ask = Some(asks[a].price);
            a += 1;
        }
        quotes.push(Quote {
            time_ms: time,
            bid: last_bid,
            ask: last_ask,
        });
    }
    quotes
}

impl From<&crate::model::SpotEvent> for Spot {
    fn from(event: &crate::model::SpotEvent) -> Self {
        Self {
            symbol_id: event.symbol_id,
            bid: event.bid,
            ask: event.ask,
            time_ms: event.timestamp,
        }
    }
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    fn tick(time_ms: i64, price: i64) -> Tick {
        Tick { time_ms, price }
    }

    #[test]
    fn the_sides_are_joined_by_time_with_the_last_value_carried() {
        let bids = [tick(10, 100), tick(30, 101)];
        let asks = [tick(10, 102), tick(20, 103)];
        let quotes = merge_sides(&bids, &asks);
        assert_eq!(
            quotes,
            vec![
                Quote {
                    time_ms: 10,
                    bid: Some(100),
                    ask: Some(102)
                },
                Quote {
                    time_ms: 20,
                    bid: Some(100),
                    ask: Some(103)
                },
                Quote {
                    time_ms: 30,
                    bid: Some(101),
                    ask: Some(103)
                },
            ]
        );
    }

    #[test]
    fn a_side_that_starts_later_is_none_until_it_ticks() {
        let quotes = merge_sides(&[tick(5, 1)], &[tick(9, 2)]);
        assert_eq!(
            quotes[0],
            Quote {
                time_ms: 5,
                bid: Some(1),
                ask: None
            }
        );
        assert_eq!(
            quotes[1],
            Quote {
                time_ms: 9,
                bid: Some(1),
                ask: Some(2)
            }
        );
    }

    #[test]
    fn several_ticks_in_one_millisecond_give_one_quote_with_the_last_price() {
        let quotes = merge_sides(&[tick(5, 1), tick(5, 2)], &[]);
        assert_eq!(
            quotes,
            vec![Quote {
                time_ms: 5,
                bid: Some(2),
                ask: None
            }]
        );
    }

    #[test]
    fn nothing_in_nothing_out() {
        assert!(merge_sides(&[], &[]).is_empty());
    }

    #[test]
    fn a_spot_event_gives_a_spot() {
        let event = crate::model::SpotEvent {
            ctid_trader_account_id: Some(1),
            symbol_id: 4,
            bid: Some(10),
            ask: None,
            trendbar: vec![],
            session_close: None,
            timestamp: Some(99),
        };
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

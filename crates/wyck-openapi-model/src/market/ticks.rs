//! Ticks: the bid/ask side enum, the readable [`Tick`], decoding the server's delta-encoded wire
//! form, and joining both sides into two-sided [`Quote`]s.
//!
//! Ticks come newest first with their times **and prices** as differences from the tick before
//! ([`decode_ticks`], confirmed on a live demo account). There is no volume per tick: a bar's
//! volume counts ticks, and only the order book has sizes.

use serde::{Deserialize, Serialize};

use crate::transport::wire::flex;

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

/// One tick with an absolute time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    /// When the tick happened, in Unix milliseconds.
    pub time_ms: i64,
    /// The raw price, see [`crate::market::to_price`].
    pub price: i64,
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

/// A tick as the server sends it (`ProtoOATickData`). Read the list with [`decode_ticks`]: the
/// times are not absolute.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireTick {
    /// For the first tick a Unix time in milliseconds, for the others a difference in
    /// milliseconds from the tick before.
    #[serde(deserialize_with = "flex::int")]
    pub timestamp: i64,
    /// The tick price.
    #[serde(deserialize_with = "flex::int")]
    pub tick: i64,
}

/// `ProtoOAGetTickDataReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTickDataReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The symbol.
    pub symbol_id: i64,
    /// Bid (1) or ask (2) ticks.
    pub r#type: i32,
    /// Start of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<i64>,
    /// End of the range, in Unix milliseconds. The range may span at most one week.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<i64>,
}

/// `ProtoOAGetTickDataRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTickDataRes {
    /// The ticks, newest first.
    #[serde(default)]
    pub tick_data: Vec<WireTick>,
    /// Whether more ticks exist in the range than were returned.
    #[serde(default)]
    pub has_more: bool,
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
/// ```
/// use wyck_openapi_model::market::{WireTick, decode_ticks};
///
/// // Newest first: the first tick is absolute, the others are steps back from it.
/// let wire = [
///     WireTick { timestamp: 1_000_000, tick: 108_501 },
///     WireTick { timestamp: -500, tick: -2 },
/// ];
/// let ticks = decode_ticks(&wire);
/// assert_eq!((ticks[0].time_ms, ticks[0].price), (999_500, 108_499)); // the older tick, first
/// assert_eq!((ticks[1].time_ms, ticks[1].price), (1_000_000, 108_501));
/// ```
///
/// The result is in time order. Ticks that share a millisecond keep the order they happened in (it
/// decides the last price), and identical ticks are kept: two ticks at the same time and price are
/// real.
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
    // The list is newest first, so reversing it gives chronological order; the stable sort only
    // has to repair a server that breaks its own ordering, and keeps ticks that share a
    // millisecond in the order they happened (their order decides the last price).
    ticks.reverse();
    ticks.sort_by_key(|t| t.time_ms);
    ticks
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn ticks_in_one_millisecond_keep_the_order_they_happened_in() {
        // Newest first: the last tick of the millisecond comes first on the wire.
        let wire = [
            WireTick {
                timestamp: 5_000,
                tick: 110_002,
            },
            WireTick {
                timestamp: 0,
                tick: -1,
            },
            WireTick {
                timestamp: 0,
                tick: -1,
            },
        ];
        let prices: Vec<i64> = decode_ticks(&wire).iter().map(|t| t.price).collect();
        assert_eq!(
            prices,
            vec![110_000, 110_001, 110_002],
            "oldest first, the last price last"
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

    #[test]
    fn integers_are_read_from_numbers_and_from_text() {
        let a: WireTick = serde_json::from_value(json!({"timestamp": 5, "tick": 108500})).unwrap();
        let b: WireTick =
            serde_json::from_value(json!({"timestamp": "5", "tick": "108500"})).unwrap();
        assert_eq!(a, b);
        let c: WireTick = serde_json::from_value(json!({"timestamp": -3.0, "tick": 1})).unwrap();
        assert_eq!(c.timestamp, -3);
        assert!(serde_json::from_value::<WireTick>(json!({"timestamp": "x", "tick": 1})).is_err());
        assert!(serde_json::from_value::<WireTick>(json!({"timestamp": 1.5, "tick": 1})).is_err());
    }

    #[test]
    fn requests_use_the_official_field_names_and_skip_missing_options() {
        let ticks = GetTickDataReq {
            ctid_trader_account_id: 7,
            symbol_id: 3,
            r#type: 1,
            from_timestamp: Some(10),
            to_timestamp: Some(20),
        };
        assert_eq!(
            serde_json::to_value(&ticks).unwrap(),
            json!({"ctidTraderAccountId": 7, "symbolId": 3, "type": 1, "fromTimestamp": 10, "toTimestamp": 20})
        );
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
}

//! Properties that must hold for any input, checked on many random ones.
//!
//! The fixed tests next to the code pin down examples. These pin down the rules: encoding then
//! decoding gives the original back, windows cover a range without holes, nothing panics on
//! garbage. A failing case is shrunk to the smallest input that still fails.

use std::time::Duration;

use ctrader_openapi::event::event_from;
use ctrader_openapi::market::{
    DepthBook, DepthEvent, DepthQuote, MAX_TICK_RANGE_MS, Period, SpotEvent, SpotTracker, Tick,
    WireTick, WireTrendbar, continuation, decode_bars, decode_ticks, format_price, from_price,
    merge_sides, tick_windows, to_price,
};
use ctrader_openapi::session::Backoff;
use ctrader_openapi::transport::wire::{Envelope, flex, payload};
use proptest::prelude::*;
use serde_json::{Value, json};

/// Encodes ticks (oldest first) the way the server sends them: newest first, the first tick
/// absolute, every other one as a difference from the tick before it in the list.
fn encode(ticks: &[Tick]) -> Vec<WireTick> {
    let mut wire = Vec::with_capacity(ticks.len());
    let mut previous: Option<Tick> = None;
    for tick in ticks.iter().rev() {
        wire.push(match previous {
            None => WireTick {
                timestamp: tick.time_ms,
                tick: tick.price,
            },
            Some(before) => WireTick {
                timestamp: tick.time_ms - before.time_ms,
                tick: tick.price - before.price,
            },
        });
        previous = Some(*tick);
    }
    wire
}

/// Ticks with non-decreasing times and modest prices, oldest first.
fn ticks() -> impl Strategy<Value = Vec<Tick>> {
    prop::collection::vec((0i64..5_000, -300i64..300), 0..200).prop_map(|steps| {
        let (mut time, mut price) = (1_700_000_000_000i64, 110_000i64);
        steps
            .into_iter()
            .map(|(dt, dp)| {
                time += dt;
                price += dp;
                Tick {
                    time_ms: time,
                    price,
                }
            })
            .collect()
    })
}

proptest! {
    // ---- ticks ----

    #[test]
    fn decoding_what_was_encoded_gives_the_ticks_back(original in ticks()) {
        prop_assert_eq!(decode_ticks(&encode(&original)), original);
    }

    #[test]
    fn decoded_ticks_are_always_in_time_order(wire in prop::collection::vec((-10_000i64..10_000, -500i64..500), 0..100)) {
        let wire: Vec<WireTick> = wire.into_iter().map(|(t, p)| WireTick { timestamp: t, tick: p }).collect();
        let decoded = decode_ticks(&wire);
        prop_assert_eq!(decoded.len(), wire.len());
        prop_assert!(decoded.windows(2).all(|w| w[0].time_ms <= w[1].time_ms));
    }

    #[test]
    fn merging_the_sides_keeps_each_last_known_value(bids in ticks(), asks in ticks()) {
        let quotes = merge_sides(&bids, &asks);
        prop_assert!(quotes.windows(2).all(|w| w[0].time_ms < w[1].time_ms), "one quote per time");
        prop_assert!(quotes.len() <= bids.len() + asks.len());
        for quote in &quotes {
            let last_bid = bids.iter().rev().find(|t| t.time_ms <= quote.time_ms).map(|t| t.price);
            let last_ask = asks.iter().rev().find(|t| t.time_ms <= quote.time_ms).map(|t| t.price);
            prop_assert_eq!(quote.bid, last_bid);
            prop_assert_eq!(quote.ask, last_ask);
        }
        // Every time either side ticked at appears.
        for tick in bids.iter().chain(asks.iter()) {
            prop_assert!(quotes.iter().any(|q| q.time_ms == tick.time_ms));
        }
    }

    // ---- windows and paging ----

    #[test]
    fn tick_windows_cover_the_range_exactly(from in -1_000_000_000_000i64..1_000_000_000_000, span in 0i64..(40 * 86_400_000)) {
        let to = from + span;
        let windows = tick_windows(from, to);
        prop_assert!(!windows.is_empty());
        prop_assert_eq!(windows[0].0, from);
        prop_assert_eq!(windows.last().unwrap().1, to);
        for (start, end) in &windows {
            prop_assert!(start <= end);
            prop_assert!(end - start < MAX_TICK_RANGE_MS);
        }
        for pair in windows.windows(2) {
            prop_assert_eq!(pair[1].0, pair[0].1 + 1);
        }
    }

    #[test]
    fn a_backwards_range_has_no_windows(from in 0i64..1_000_000, gap in 1i64..1_000_000) {
        prop_assert!(tick_windows(from, from - gap).is_empty());
    }

    #[test]
    fn the_rest_of_a_truncated_page_is_always_smaller_and_inside(
        low in 0i64..1_000_000_000,
        width in 1i64..1_000_000_000,
        a in 0i64..1_000_000_000,
        b in 0i64..1_000_000_000,
    ) {
        let high = low + width;
        let (first, last) = (low + a.min(b) % (width + 1), low + a.max(b) % (width + 1));
        let (first, last) = (first.min(last), first.max(last));
        if let Some((next_low, next_high)) = continuation(low, high, first, last, Period::M1) {
            prop_assert!(next_low >= low && next_high <= high);
            prop_assert!(next_high - next_low < high - low, "progress is guaranteed");
            prop_assert!(next_low <= next_high + 1);
        }
    }

    // ---- bars ----

    #[test]
    fn decoded_bars_are_sane_sorted_and_unique(raw in prop::collection::vec(
        (prop::option::of(-1_000i64..200_000), prop::option::of(0i64..500), prop::option::of(0i64..500), prop::option::of(0i64..500), prop::option::of(0i64..40_000_000), 0i64..1000),
        0..60,
    )) {
        let wire: Vec<WireTrendbar> = raw
            .into_iter()
            .map(|(low, open, close, high, minutes, volume)| WireTrendbar {
                volume,
                period: Some(1),
                low,
                delta_open: open,
                delta_close: close,
                delta_high: high,
                utc_timestamp_in_minutes: minutes,
            })
            .collect();
        let bars = decode_bars(&wire);
        prop_assert!(bars.iter().all(|b| b.is_sane()));
        prop_assert!(bars.windows(2).all(|w| w[0].time_ms < w[1].time_ms));
        prop_assert!(bars.len() <= wire.len());
    }

    // ---- prices ----

    #[test]
    fn a_price_survives_the_round_trip_through_the_integer(raw in -50_000_000_000i64..50_000_000_000) {
        prop_assert_eq!(from_price(to_price(raw)), raw);
    }

    #[test]
    fn a_formatted_price_reads_back_within_half_a_unit_of_its_last_decimal(
        raw in -5_000_000_000i64..5_000_000_000,
        digits in 0u32..=5,
    ) {
        let text = format_price(raw, digits);
        let parsed: f64 = text.parse().unwrap();
        let tolerance = 0.5 * 10f64.powi(-(digits as i32)) + 1e-9;
        prop_assert!((parsed - to_price(raw)).abs() <= tolerance, "{text} for {raw} at {digits}");
        let decimals = text.split('.').nth(1).map_or(0, str::len);
        prop_assert_eq!(decimals, digits as usize);
    }

    // ---- integers from the server ----

    #[test]
    fn any_integer_is_read_from_a_number_and_from_text(value in any::<i64>()) {
        #[derive(serde::Deserialize)]
        struct One {
            #[serde(deserialize_with = "flex::int")]
            n: i64,
        }
        let as_number: One = serde_json::from_value(json!({"n": value})).unwrap();
        let as_text: One = serde_json::from_value(json!({"n": value.to_string()})).unwrap();
        prop_assert_eq!(as_number.n, value);
        prop_assert_eq!(as_text.n, value);
    }

    // ---- the envelope and unknown input ----

    #[test]
    fn an_envelope_survives_being_written_and_read(
        kind in any::<u32>(),
        id in prop::option::of("[a-z0-9]{1,24}"),
        text in ".{0,40}",
        number in any::<i64>(),
    ) {
        let envelope = Envelope {
            client_msg_id: id,
            payload_type: kind,
            payload: json!({"text": text, "number": number, "list": [1, 2, 3]}),
        };
        let again = Envelope::from_text(&envelope.to_text().unwrap()).unwrap();
        prop_assert_eq!(again, envelope);
    }

    #[test]
    fn garbage_is_never_a_panic_only_an_error_or_an_event(text in ".{0,200}") {
        if let Ok(envelope) = Envelope::from_text(&text) {
            let _ = event_from(&envelope);
        }
    }

    #[test]
    fn a_known_event_with_any_payload_is_decoded_or_reported_never_a_panic(
        which in prop::sample::select(vec![
            payload::SPOT_EVENT, payload::DEPTH_EVENT, payload::TRADER_UPDATE_EVENT,
            payload::ACCOUNTS_TOKEN_INVALIDATED_EVENT, payload::ACCOUNT_DISCONNECT_EVENT,
            payload::CLIENT_DISCONNECT_EVENT, payload::ERROR_RES, payload::HEARTBEAT_EVENT,
            payload::EXECUTION_EVENT, payload::ORDER_ERROR_EVENT,
            payload::TRAILING_SL_CHANGED_EVENT, payload::MARGIN_CHANGED_EVENT,
            payload::MARGIN_CALL_TRIGGER_EVENT, payload::MARGIN_CALL_UPDATE_EVENT,
            payload::SYMBOL_CHANGED_EVENT, 424242,
        ]),
        body in json_value(),
    ) {
        let envelope = Envelope { client_msg_id: None, payload_type: which, payload: body };
        let _ = event_from(&envelope);
    }

    // ---- the market helpers ----

    #[test]
    fn the_tracker_always_holds_the_last_side_seen(events in prop::collection::vec(
        (0i64..3, prop::option::of(1i64..1000), prop::option::of(1i64..1000)),
        0..80,
    )) {
        let mut tracker = SpotTracker::new();
        let mut expected: std::collections::HashMap<i64, (Option<i64>, Option<i64>)> = Default::default();
        for (symbol, bid, ask) in events {
            let quote = tracker.apply(&SpotEvent {
                ctid_trader_account_id: None, symbol_id: symbol, bid, ask,
                trendbar: vec![], session_close: None, timestamp: None,
            });
            let entry = expected.entry(symbol).or_default();
            entry.0 = bid.or(entry.0);
            entry.1 = ask.or(entry.1);
            prop_assert_eq!((quote.bid, quote.ask), *entry);
        }
        for (symbol, (bid, ask)) in expected {
            let held = tracker.get(symbol).unwrap();
            prop_assert_eq!((held.bid, held.ask), (bid, ask));
        }
    }

    #[test]
    fn the_book_is_always_sorted_and_an_entry_lives_on_one_side_only(steps in prop::collection::vec(
        (prop::collection::vec((1i64..12, 1i64..500, any::<bool>(), 90i64..110), 0..6), prop::collection::vec(1i64..12, 0..4)),
        0..30,
    )) {
        let mut book = DepthBook::new();
        for (added, deleted) in steps {
            let new_quotes = added
                .into_iter()
                .map(|(id, size, is_bid, price)| DepthQuote {
                    id: Some(id),
                    size: Some(size),
                    bid: is_bid.then_some(price),
                    ask: (!is_bid).then_some(price),
                })
                .collect();
            book.apply(&DepthEvent { symbol_id: 1, new_quotes, deleted_quotes: deleted });

            let bids = book.bids();
            let asks = book.asks();
            prop_assert!(bids.windows(2).all(|w| w[0].price >= w[1].price));
            prop_assert!(asks.windows(2).all(|w| w[0].price <= w[1].price));
            prop_assert_eq!(bids.iter().map(|l| l.size).sum::<i64>(), book.bid_size());
            prop_assert_eq!(asks.iter().map(|l| l.size).sum::<i64>(), book.ask_size());
            prop_assert_eq!(book.best_bid(), bids.first().copied());
            prop_assert_eq!(book.best_ask(), asks.first().copied());
        }
    }

    // ---- reconnection delays ----

    #[test]
    fn waits_never_shrink_and_never_pass_the_cap(
        initial_ms in 1u64..5_000,
        max_ms in 1u64..600_000,
        factor in 0u32..6,
        noise in any::<u32>(),
    ) {
        let backoff = Backoff {
            initial: Duration::from_millis(initial_ms),
            max: Duration::from_millis(max_ms.max(initial_ms)),
            factor,
        };
        let mut last = Duration::ZERO;
        for attempt in 1..40 {
            let wait = backoff.delay(attempt);
            prop_assert!(wait >= last, "attempt {attempt}");
            prop_assert!(wait <= backoff.max);
            let jittered = backoff.jittered(attempt, noise);
            prop_assert!(jittered >= wait && jittered <= wait + wait / 5 + Duration::from_millis(1));
            last = wait;
        }
    }
}

/// Arbitrary JSON, a few levels deep.
fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        any::<f64>()
            .prop_filter("finite", |f| f.is_finite())
            .prop_map(Value::from),
        "[a-zA-Z0-9 ]{0,12}".prop_map(Value::from),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::vec(("[a-zA-Z]{1,8}", inner), 0..4)
                .prop_map(|pairs| Value::Object(pairs.into_iter().collect())),
        ]
    })
}

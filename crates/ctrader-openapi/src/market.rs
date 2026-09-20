//! Working with market data as it arrives: symbol lookup, the latest price of each symbol, the
//! order book, and price formatting.
//!
//! The server's messages are built for the wire, not for use. A price event may carry only the side
//! that changed; the order book comes as a stream of additions and removals; symbols are numbers.
//! The small types here keep the state needed to turn that into what a program wants: a full quote,
//! the best bid and ask, a symbol by name.
//!
//! None of them does I/O. Feed them the events of [`crate::Client::events`] and read them back.

use std::collections::{BTreeMap, HashMap};

use crate::event::Event;
use crate::model::{DepthEvent, LightSymbol, SpotEvent, WireTrendbar};
use crate::types::{Bar, PRICE_SCALE, Period, Spot, decode_bar};

// ---- symbols ----

/// A symbol list indexed by id and by name.
///
/// Names are matched without regard to case (`eurusd` finds `EURUSD`). A name that appears twice
/// keeps the first symbol.
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    symbols: Vec<LightSymbol>,
    by_id: HashMap<i64, usize>,
    by_name: HashMap<String, usize>,
}

impl SymbolTable {
    /// Indexes `symbols`.
    #[must_use]
    pub fn new(symbols: impl IntoIterator<Item = LightSymbol>) -> Self {
        let mut table = Self::default();
        for symbol in symbols {
            let index = table.symbols.len();
            table.by_id.entry(symbol.symbol_id).or_insert(index);
            if let Some(name) = &symbol.symbol_name {
                table
                    .by_name
                    .entry(name.to_ascii_uppercase())
                    .or_insert(index);
            }
            table.symbols.push(symbol);
        }
        table
    }

    /// How many symbols there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// The symbol with this id.
    #[must_use]
    pub fn get(&self, symbol_id: i64) -> Option<&LightSymbol> {
        self.by_id.get(&symbol_id).map(|&i| &self.symbols[i])
    }

    /// The symbol with this name, in any case.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&LightSymbol> {
        self.by_name
            .get(&name.to_ascii_uppercase())
            .map(|&i| &self.symbols[i])
    }

    /// The id of the symbol with this name.
    #[must_use]
    pub fn id_of(&self, name: &str) -> Option<i64> {
        self.find(name).map(|s| s.symbol_id)
    }

    /// The name of the symbol with this id.
    #[must_use]
    pub fn name_of(&self, symbol_id: i64) -> Option<&str> {
        self.get(symbol_id).and_then(|s| s.symbol_name.as_deref())
    }

    /// The symbols whose name starts with `prefix` (any case), in list order.
    #[must_use]
    pub fn starting_with(&self, prefix: &str) -> Vec<&LightSymbol> {
        let prefix = prefix.to_ascii_uppercase();
        self.symbols
            .iter()
            .filter(|s| {
                s.symbol_name
                    .as_deref()
                    .is_some_and(|n| n.to_ascii_uppercase().starts_with(&prefix))
            })
            .collect()
    }

    /// Every symbol, in list order.
    pub fn iter(&self) -> impl Iterator<Item = &LightSymbol> {
        self.symbols.iter()
    }
}

// ---- prices ----

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

impl SpotEvent {
    /// The live bars the event carries, decoded, with their periods. A bar without a known period
    /// or with damaged numbers is left out. Live bars only arrive after
    /// [`crate::Client::subscribe_live_bars`].
    #[must_use]
    pub fn live_bars(&self) -> Vec<(Period, Bar)> {
        self.trendbar.iter().filter_map(live_bar).collect()
    }
}

fn live_bar(wire: &WireTrendbar) -> Option<(Period, Bar)> {
    let period = Period::from_number(wire.period.unwrap_or(1))?;
    Some((period, decode_bar(wire)?))
}

/// A raw price as text with `digits` decimals, as the symbol quotes it (`114880` with 5 digits is
/// `1.14880`, with 3 digits `1.149`). The price is rounded to the decimals asked for.
///
/// `digits` above 5 is treated as 5: the server's integers only carry five decimals.
#[must_use]
pub fn format_price(raw: i64, digits: u32) -> String {
    let digits = digits.min(5);
    let drop = 10i128.pow(5 - digits);
    let scaled = i128::from(raw);
    // Round half away from zero at the last decimal kept.
    let rounded = if scaled >= 0 {
        (scaled + drop / 2) / drop
    } else {
        (scaled - drop / 2) / drop
    };
    let unit = 10i128.pow(digits);
    let sign = if rounded < 0 { "-" } else { "" };
    let magnitude = rounded.abs();
    if digits == 0 {
        return format!("{sign}{magnitude}");
    }
    format!(
        "{sign}{}.{:0width$}",
        magnitude / unit,
        magnitude % unit,
        width = digits as usize
    )
}

// ---- the order book ----

/// One price level of the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepthLevel {
    /// The raw price (see [`crate::types::to_price`]).
    pub price: i64,
    /// The size in hundredths of a unit (see [`crate::account::volume_units`]).
    pub size: i64,
}

/// The order book of one symbol, kept up to date from [`DepthEvent`]s.
///
/// The server sends the entries that were added or changed and the ids of the ones removed; this
/// keeps the entries by id and reads them back sorted.
#[derive(Debug, Clone, Default)]
pub struct DepthBook {
    bids: BTreeMap<i64, DepthLevel>,
    asks: BTreeMap<i64, DepthLevel>,
}

impl DepthBook {
    /// An empty book.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies one event: entries are added or replaced by id, removed ids are dropped from both
    /// sides. An entry without a price or without an id is ignored.
    pub fn apply(&mut self, event: &DepthEvent) {
        for id in &event.deleted_quotes {
            self.bids.remove(id);
            self.asks.remove(id);
        }
        for quote in &event.new_quotes {
            let (Some(id), size) = (quote.id, quote.size.unwrap_or(0)) else {
                continue;
            };
            // An entry may move from one side to the other only by being removed first, but be
            // safe: whichever side it is on now is the only one that keeps it.
            self.bids.remove(&id);
            self.asks.remove(&id);
            if let Some(price) = quote.bid {
                self.bids.insert(id, DepthLevel { price, size });
            } else if let Some(price) = quote.ask {
                self.asks.insert(id, DepthLevel { price, size });
            }
        }
    }

    /// The bid side, best (highest price) first.
    #[must_use]
    pub fn bids(&self) -> Vec<DepthLevel> {
        let mut levels: Vec<DepthLevel> = self.bids.values().copied().collect();
        levels.sort_by_key(|l| std::cmp::Reverse(l.price));
        levels
    }

    /// The ask side, best (lowest price) first.
    #[must_use]
    pub fn asks(&self) -> Vec<DepthLevel> {
        let mut levels: Vec<DepthLevel> = self.asks.values().copied().collect();
        levels.sort_by_key(|l| l.price);
        levels
    }

    /// The highest bid.
    #[must_use]
    pub fn best_bid(&self) -> Option<DepthLevel> {
        self.bids.values().max_by_key(|l| l.price).copied()
    }

    /// The lowest ask.
    #[must_use]
    pub fn best_ask(&self) -> Option<DepthLevel> {
        self.asks.values().min_by_key(|l| l.price).copied()
    }

    /// The gap between the best ask and the best bid, when both exist. Negative in a crossed book.
    #[must_use]
    pub fn spread(&self) -> Option<i64> {
        Some(self.best_ask()?.price - self.best_bid()?.price)
    }

    /// The total size on the bid side.
    #[must_use]
    pub fn bid_size(&self) -> i64 {
        self.bids.values().map(|l| l.size).sum()
    }

    /// The total size on the ask side.
    #[must_use]
    pub fn ask_size(&self) -> i64 {
        self.asks.values().map(|l| l.size).sum()
    }

    /// Forgets everything, after a reconnect.
    pub fn clear(&mut self) {
        self.bids.clear();
        self.asks.clear();
    }

    /// Whether the book holds no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bids.is_empty() && self.asks.is_empty()
    }
}

/// The number of raw price units in a price of 1.0 (re-exported for convenience next to
/// [`format_price`]).
pub const UNITS_PER_PRICE: i64 = PRICE_SCALE;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DepthQuote;
    use serde_json::json;

    fn symbol(id: i64, name: &str) -> LightSymbol {
        LightSymbol {
            symbol_id: id,
            symbol_name: Some(name.to_owned()),
            enabled: Some(true),
            description: None,
            base_asset_id: None,
            quote_asset_id: None,
            symbol_category_id: None,
        }
    }

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

    fn depth(new: Vec<DepthQuote>, deleted: Vec<i64>) -> DepthEvent {
        DepthEvent {
            symbol_id: 1,
            new_quotes: new,
            deleted_quotes: deleted,
        }
    }

    fn quote(id: i64, size: i64, bid: Option<i64>, ask: Option<i64>) -> DepthQuote {
        DepthQuote {
            id: Some(id),
            size: Some(size),
            bid,
            ask,
        }
    }

    // ---- symbols ----

    #[test]
    fn symbols_are_found_by_id_and_by_name_in_any_case() {
        let table = SymbolTable::new([
            symbol(1, "EURUSD"),
            symbol(2, "XAUUSD"),
            symbol(3, "EURGBP"),
        ]);
        assert_eq!(table.len(), 3);
        assert_eq!(table.id_of("eurusd"), Some(1));
        assert_eq!(table.id_of("XauUsd"), Some(2));
        assert_eq!(table.name_of(3), Some("EURGBP"));
        assert_eq!(table.id_of("NOPE"), None);
        assert_eq!(table.name_of(99), None);
    }

    #[test]
    fn a_prefix_search_keeps_list_order() {
        let table = SymbolTable::new([
            symbol(1, "EURUSD"),
            symbol(2, "XAUUSD"),
            symbol(3, "EURGBP"),
        ]);
        let names: Vec<_> = table
            .starting_with("eur")
            .iter()
            .filter_map(|s| s.symbol_name.as_deref())
            .collect();
        assert_eq!(names, vec!["EURUSD", "EURGBP"]);
        assert!(table.starting_with("zzz").is_empty());
    }

    #[test]
    fn a_repeated_name_keeps_the_first_and_a_nameless_symbol_is_still_listed() {
        let mut nameless = symbol(9, "X");
        nameless.symbol_name = None;
        let table = SymbolTable::new([symbol(1, "EURUSD"), symbol(2, "eurusd"), nameless]);
        assert_eq!(table.id_of("EURUSD"), Some(1));
        assert_eq!(table.len(), 3);
        assert!(table.get(9).is_some());
        assert_eq!(table.name_of(9), None);
        assert!(SymbolTable::default().is_empty());
    }

    // ---- prices ----

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
                    crate::event::DisconnectReason::ClosedByServer
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
    fn prices_are_formatted_with_the_symbols_decimals() {
        assert_eq!(format_price(114_880, 5), "1.14880");
        assert_eq!(format_price(114_880, 4), "1.1488");
        assert_eq!(format_price(114_886, 4), "1.1489");
        assert_eq!(format_price(15_676_800, 3), "156.768");
        assert_eq!(format_price(265_000_000, 2), "2650.00");
        assert_eq!(format_price(265_000_000, 0), "2650");
        assert_eq!(format_price(0, 5), "0.00000");
        assert_eq!(format_price(5, 5), "0.00005");
        assert_eq!(format_price(-114_880, 5), "-1.14880");
        assert_eq!(
            format_price(-50, 2),
            "0.00",
            "a value that rounds to zero has no sign"
        );
        assert_eq!(
            format_price(114_880, 9),
            "1.14880",
            "more than five decimals is five"
        );
    }

    // ---- the order book ----

    #[test]
    fn a_book_is_built_from_additions_and_sorted_best_first() {
        let mut book = DepthBook::new();
        book.apply(&depth(
            vec![
                quote(1, 500, Some(100), None),
                quote(2, 300, Some(101), None),
                quote(3, 400, None, Some(103)),
                quote(4, 200, None, Some(102)),
            ],
            vec![],
        ));
        assert_eq!(book.best_bid().unwrap().price, 101);
        assert_eq!(book.best_ask().unwrap().price, 102);
        assert_eq!(book.spread(), Some(1));
        assert_eq!(
            book.bids().iter().map(|l| l.price).collect::<Vec<_>>(),
            vec![101, 100]
        );
        assert_eq!(
            book.asks().iter().map(|l| l.price).collect::<Vec<_>>(),
            vec![102, 103]
        );
        assert_eq!((book.bid_size(), book.ask_size()), (800, 600));
    }

    #[test]
    fn a_changed_entry_replaces_and_a_deleted_one_disappears() {
        let mut book = DepthBook::new();
        book.apply(&depth(
            vec![
                quote(1, 500, Some(100), None),
                quote(2, 300, None, Some(103)),
            ],
            vec![],
        ));
        book.apply(&depth(vec![quote(1, 900, Some(100), None)], vec![2]));
        assert_eq!(book.bid_size(), 900);
        assert!(book.asks().is_empty());
        assert_eq!(book.spread(), None);
        book.clear();
        assert!(book.is_empty());
    }

    #[test]
    fn an_entry_that_changes_side_is_only_kept_on_the_new_one() {
        let mut book = DepthBook::new();
        book.apply(&depth(vec![quote(7, 100, Some(100), None)], vec![]));
        book.apply(&depth(vec![quote(7, 100, None, Some(104))], vec![]));
        assert!(book.bids().is_empty());
        assert_eq!(book.asks().len(), 1);
    }

    #[test]
    fn entries_without_an_id_or_a_price_are_ignored() {
        let mut book = DepthBook::new();
        book.apply(&depth(
            vec![
                DepthQuote {
                    id: None,
                    size: Some(1),
                    bid: Some(1),
                    ask: None,
                },
                quote(5, 10, None, None),
            ],
            vec![],
        ));
        assert!(book.is_empty());
    }

    #[test]
    fn a_crossed_book_has_a_negative_spread() {
        let mut book = DepthBook::new();
        book.apply(&depth(
            vec![quote(1, 1, Some(105), None), quote(2, 1, None, Some(103))],
            vec![],
        ));
        assert_eq!(book.spread(), Some(-2));
    }
}

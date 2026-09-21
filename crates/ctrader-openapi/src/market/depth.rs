//! The order book: [`DepthBook`] keeps it up to date from the server's additions and removals and
//! reads it back sorted, best price first on each side.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::transport::wire::flex;

/// One order book entry (`ProtoOADepthQuote`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthQuote {
    /// The entry id.
    #[serde(default, deserialize_with = "flex::opt")]
    pub id: Option<i64>,
    /// The size, in hundredths of a unit.
    #[serde(default, deserialize_with = "flex::opt")]
    pub size: Option<i64>,
    /// The price, for a bid entry.
    #[serde(default, deserialize_with = "flex::opt")]
    pub bid: Option<i64>,
    /// The price, for an ask entry.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ask: Option<i64>,
}

/// `ProtoOADepthEvent`: changes of the order book.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthEvent {
    /// The symbol.
    #[serde(deserialize_with = "flex::int")]
    pub symbol_id: i64,
    /// Entries that were added or changed.
    #[serde(default)]
    pub new_quotes: Vec<DepthQuote>,
    /// Ids of the entries that were removed.
    #[serde(default, deserialize_with = "flex::list")]
    pub deleted_quotes: Vec<i64>,
}

/// One price level of the book: the total size of the entries at that price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepthLevel {
    /// The raw price (see [`crate::market::to_price`]).
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

    /// The bid side as price levels, best (highest price) first. Entries at the same price are added
    /// into one level, as in any order book.
    #[must_use]
    pub fn bids(&self) -> Vec<DepthLevel> {
        let mut levels = levels_of(&self.bids);
        levels.reverse();
        levels
    }

    /// The ask side as price levels, best (lowest price) first. Entries at the same price are added
    /// into one level.
    #[must_use]
    pub fn asks(&self) -> Vec<DepthLevel> {
        levels_of(&self.asks)
    }

    /// The highest bid level.
    #[must_use]
    pub fn best_bid(&self) -> Option<DepthLevel> {
        self.bids().first().copied()
    }

    /// The lowest ask level.
    #[must_use]
    pub fn best_ask(&self) -> Option<DepthLevel> {
        self.asks().first().copied()
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

/// Adds the entries at each price into one level, lowest price first.
fn levels_of(entries: &BTreeMap<i64, DepthLevel>) -> Vec<DepthLevel> {
    let mut by_price: BTreeMap<i64, i64> = BTreeMap::new();
    for level in entries.values() {
        *by_price.entry(level.price).or_default() += level.size;
    }
    by_price
        .into_iter()
        .map(|(price, size)| DepthLevel { price, size })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_depth_event_reads_its_lists() {
        let e: DepthEvent = serde_json::from_value(serde_json::json!({
            "symbolId": 1,
            "newQuotes": [{"id": 5, "size": 100, "bid": 108490}],
            "deletedQuotes": [1, "2"]
        }))
        .unwrap();
        assert_eq!(e.new_quotes[0].bid, Some(108_490));
        assert_eq!(e.deleted_quotes, vec![1, 2]);
    }

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

#[cfg(test)]
mod level_tests {
    use super::*;

    fn quote(id: i64, size: i64, bid: Option<i64>, ask: Option<i64>) -> DepthQuote {
        DepthQuote {
            id: Some(id),
            size: Some(size),
            bid,
            ask,
        }
    }

    #[test]
    fn entries_at_the_same_price_are_one_level() {
        let mut book = DepthBook::new();
        book.apply(&DepthEvent {
            symbol_id: 1,
            new_quotes: vec![
                quote(1, 100, Some(105), None),
                quote(2, 250, Some(105), None),
                quote(3, 50, Some(104), None),
                quote(4, 70, None, Some(106)),
                quote(5, 30, None, Some(106)),
            ],
            deleted_quotes: vec![],
        });
        assert_eq!(
            book.bids(),
            vec![
                DepthLevel {
                    price: 105,
                    size: 350
                },
                DepthLevel {
                    price: 104,
                    size: 50
                }
            ]
        );
        assert_eq!(
            book.asks(),
            vec![DepthLevel {
                price: 106,
                size: 100
            }]
        );
        assert_eq!(
            book.best_bid(),
            Some(DepthLevel {
                price: 105,
                size: 350
            })
        );
        assert_eq!(book.bid_size(), 400, "the totals count every entry");
    }
}

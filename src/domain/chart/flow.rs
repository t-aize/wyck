//! Order flow: how much traded at each price of each bar, split between sellers and buyers. It
//! is what the footprint chart type draws.
//!
//! # Where the numbers come from
//!
//! The Open API has no trade tape: no size and no side per trade. What it has is the bid and the
//! ask, tick by tick. So the flow is inferred from the quotes, as every spot forex and CFD
//! footprint does:
//!
//! - Every quote that moves the middle of the spread is one unit of volume (tick volume, the
//!   same measure the server's bars use).
//! - When the middle moved up the unit is a **buy** (buyers lifted the ask); when it moved down it
//!   is a **sell** (sellers hit the bid). A quote that leaves the middle where it was only widened
//!   or narrowed the spread: it shows no aggression and counts for nothing.
//! - The unit is put on the **bid** price, the price the candles are drawn from, so the cells of a
//!   bar never reach past its wicks.
//!
//! This is an estimate of the direction, not the traded size. The chart says so in its legend.
//!
//! Everything here is plain data, so the rules are unit tested without a window.

use crate::domain::market::Quote;

/// The most bars of flow kept. Older ones are dropped as new ones arrive.
pub const MAX_FLOW_BARS: usize = 3_000;

/// Which side of the market a unit of volume came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Sellers hit the bid.
    Sell,
    /// Buyers lifted the ask.
    Buy,
}

/// Turns a stream of quotes into units of volume with a side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Classifier {
    bid: Option<i64>,
    ask: Option<i64>,
}

impl Classifier {
    /// The next quote (a side that did not change may be `None`). Returns the bid price and the
    /// side of the volume it stands for, or `None` when it shows no aggression.
    pub fn push(&mut self, bid: Option<i64>, ask: Option<i64>) -> Option<(i64, Side)> {
        let (old_bid, old_ask) = (self.bid, self.ask);
        self.bid = bid.or(self.bid);
        self.ask = ask.or(self.ask);
        let now = self.bid?;
        // Both sides known before and now: the middle of the spread decides. Otherwise the bid
        // alone has to.
        let moved = match (old_bid, old_ask, self.ask) {
            (Some(ob), Some(oa), Some(a)) => (now + a) - (ob + oa),
            (Some(ob), _, _) => now - ob,
            _ => return None,
        };
        match moved.cmp(&0) {
            std::cmp::Ordering::Greater => Some((now, Side::Buy)),
            std::cmp::Ordering::Less => Some((now, Side::Sell)),
            std::cmp::Ordering::Equal => None,
        }
    }
}

/// What traded at one price of a bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    /// Raw price units.
    pub price: i64,
    pub sell: u32,
    pub buy: u32,
}

/// The flow of one bar, from the lowest price up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarFlow {
    /// When the bar opens, Unix milliseconds.
    pub time_ms: i64,
    pub levels: Vec<Level>,
}

impl BarFlow {
    fn new(time_ms: i64) -> Self {
        Self {
            time_ms,
            levels: Vec::new(),
        }
    }

    fn add(&mut self, price: i64, side: Side) {
        let at = match self.levels.binary_search_by_key(&price, |l| l.price) {
            Ok(at) => at,
            Err(at) => {
                self.levels.insert(
                    at,
                    Level {
                        price,
                        sell: 0,
                        buy: 0,
                    },
                );
                at
            }
        };
        let level = &mut self.levels[at];
        match side {
            Side::Sell => level.sell = level.sell.saturating_add(1),
            Side::Buy => level.buy = level.buy.saturating_add(1),
        }
    }

    /// The units sold and bought in the whole bar.
    pub fn totals(&self) -> (u64, u64) {
        self.levels.iter().fold((0, 0), |(sell, buy), l| {
            (sell + u64::from(l.sell), buy + u64::from(l.buy))
        })
    }
}

/// The open time of the bar a quote at `time_ms` belongs to, given the open times of the bars
/// (oldest first). A time past the newest bar belongs to it, or, once the bar is over, to the
/// bar that will open after it (`bar_ms` long), which the chart has not received yet. `None` for
/// a time before the first bar.
pub fn bar_open(times: &[i64], bar_ms: Option<i64>, time_ms: i64) -> Option<i64> {
    let after = times.partition_point(|t| *t <= time_ms);
    let open = *times.get(after.checked_sub(1)?)?;
    match bar_ms {
        Some(ms) if ms > 0 && after == times.len() && time_ms >= open + ms => {
            Some(open + (time_ms - open) / ms * ms)
        }
        _ => Some(open),
    }
}

/// One request for flow: the bars `start..end` of the chart, the quotes of `from..=to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub start: usize,
    pub end: usize,
    pub from: i64,
    pub to: i64,
    /// The open time of the oldest bar the request makes complete.
    pub covers: i64,
    /// Whether it reaches the present (the flow carries on from its end).
    pub newest: bool,
}

/// What the flow held has, for [`next_step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held {
    /// The oldest bar whose flow is complete, `None` when nothing is held.
    pub covered: Option<i64>,
    /// How many bars of flow are held.
    pub len: usize,
    /// The time of the last quote counted, when the quotes since it are missing.
    pub gap_after: Option<i64>,
}

/// What to ask for next, given the bars (oldest first) and how to read their open times, what is
/// held, `want` (the index of the oldest bar the screen needs), `per_chunk` (the most bars one
/// request covers) and the time now.
///
/// Nothing is held: the newest bars, whatever the screen needs, so live quotes have something to
/// join. A gap: the quotes since the last one counted. Otherwise the bars before the oldest held,
/// a chunk at a time, until the screen is covered. `None` when there is nothing to ask.
pub fn next_step<T>(
    bars: &[T],
    time: impl Fn(&T) -> i64,
    held: Held,
    want: usize,
    per_chunk: usize,
    now: i64,
) -> Option<Step> {
    let len = bars.len();
    if len == 0 {
        return None;
    }
    let per_chunk = per_chunk.max(1);
    let want = want.min(len - 1);
    match (held.covered, held.gap_after) {
        (Some(covered), Some(last)) => {
            let start = bars
                .partition_point(|b| time(b) <= last)
                .saturating_sub(1)
                .min(len - 1);
            Some(Step {
                start,
                end: len,
                from: last,
                to: now,
                covers: covered,
                newest: true,
            })
        }
        (None, _) => {
            let start = want.max(len.saturating_sub(per_chunk));
            Some(Step {
                start,
                end: len,
                from: time(&bars[start]),
                to: now,
                covers: time(&bars[start]),
                newest: true,
            })
        }
        (Some(covered), None) => {
            if held.len >= MAX_FLOW_BARS {
                return None;
            }
            let end = bars.partition_point(|b| time(b) < covered);
            if end == 0 || end <= want {
                return None;
            }
            let start = want.max(end.saturating_sub(per_chunk));
            Some(Step {
                start,
                end,
                from: time(&bars[start]),
                to: covered - 1,
                covers: time(&bars[start]),
                newest: false,
            })
        }
    }
}

/// The flow of the bars held, and how far back and forward it reaches.
#[derive(Debug, Clone, Default)]
pub struct Flow {
    bars: Vec<BarFlow>,
    /// The open time of the oldest bar whose flow is complete; `None` until something loaded.
    from_ms: Option<i64>,
    /// The time of the newest quote counted.
    last_ms: i64,
    classifier: Classifier,
}

impl Flow {
    pub fn is_loaded(&self) -> bool {
        self.from_ms.is_some()
    }

    pub fn covered_from(&self) -> Option<i64> {
        self.from_ms
    }

    pub fn last_ms(&self) -> i64 {
        self.last_ms
    }

    pub fn len(&self) -> usize {
        self.bars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    /// The flow of the bar that opens at `time_ms`.
    pub fn bar(&self, time_ms: i64) -> Option<&BarFlow> {
        let at = self
            .bars
            .binary_search_by_key(&time_ms, |b| b.time_ms)
            .ok()?;
        self.bars.get(at)
    }

    fn bar_mut(&mut self, time_ms: i64) -> &mut BarFlow {
        match self.bars.binary_search_by_key(&time_ms, |b| b.time_ms) {
            Ok(at) => &mut self.bars[at],
            Err(at) => {
                self.bars.insert(at, BarFlow::new(time_ms));
                &mut self.bars[at]
            }
        }
    }

    /// Says the flow is complete from the bar opening at `from_ms` on, even where no quote fell.
    pub fn cover_from(&mut self, from_ms: i64) {
        self.from_ms = Some(self.from_ms.map_or(from_ms, |held| held.min(from_ms)));
    }

    /// Counts a batch of quotes (oldest first). `times` are the open times of the bars (oldest
    /// first) and `bar_ms` their length when it is fixed.
    ///
    /// The batch is classified on its own, so its first quote has nothing to be compared with and
    /// counts for nothing. With `newest` set the batch reaches the present: the flow carries on
    /// from where it ended, for live quotes and for a refill after a lost connection.
    pub fn ingest(&mut self, times: &[i64], bar_ms: Option<i64>, quotes: &[Quote], newest: bool) {
        let mut classifier = if newest {
            self.classifier
        } else {
            Classifier::default()
        };
        for quote in quotes {
            if newest && quote.time_ms <= self.last_ms {
                continue;
            }
            if let Some((price, side)) = classifier.push(quote.bid, quote.ask)
                && let Some(open) = bar_open(times, bar_ms, quote.time_ms)
            {
                self.bar_mut(open).add(price, side);
            }
            if newest {
                self.last_ms = quote.time_ms;
            }
        }
        if newest {
            self.classifier = classifier;
        }
        self.trim();
    }

    /// Counts one live quote into the bar that opens at `open`. A quote older than the newest
    /// one counted is folded in at the newest time, so the order never breaks.
    pub fn push_live(&mut self, open: i64, time_ms: i64, bid: Option<i64>, ask: Option<i64>) {
        self.last_ms = self.last_ms.max(time_ms);
        if let Some((price, side)) = self.classifier.push(bid, ask) {
            self.bar_mut(open).add(price, side);
            self.trim();
        }
    }

    fn trim(&mut self) {
        if self.bars.len() > MAX_FLOW_BARS {
            let excess = self.bars.len() - MAX_FLOW_BARS;
            self.bars.drain(..excess);
            self.from_ms = self.bars.first().map(|b| b.time_ms);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/domain/chart/flow.rs"]
mod tests;

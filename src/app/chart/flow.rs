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

use wyck::openapi::market::Quote;

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
mod tests {
    use super::*;

    fn q(time_ms: i64, bid: i64, ask: i64) -> Quote {
        Quote {
            time_ms,
            bid: Some(bid),
            ask: Some(ask),
        }
    }

    #[test]
    fn a_rising_middle_is_a_buy_and_a_falling_one_a_sell() {
        let mut c = Classifier::default();
        assert_eq!(c.push(Some(100), Some(102)), None, "nothing to compare");
        assert_eq!(c.push(Some(101), Some(103)), Some((101, Side::Buy)));
        assert_eq!(c.push(Some(99), Some(101)), Some((99, Side::Sell)));
    }

    #[test]
    fn a_change_of_spread_alone_is_not_aggression() {
        let mut c = Classifier::default();
        c.push(Some(100), Some(102));
        assert_eq!(c.push(Some(99), Some(103)), None, "the middle stayed");
        assert_eq!(c.push(Some(100), Some(102)), None);
    }

    #[test]
    fn a_side_that_did_not_change_is_carried() {
        let mut c = Classifier::default();
        c.push(Some(100), Some(102));
        // Only the ask moved, up: the middle rose by one.
        assert_eq!(c.push(None, Some(104)), Some((100, Side::Buy)));
        // Only the bid moved, down.
        assert_eq!(c.push(Some(97), None), Some((97, Side::Sell)));
    }

    #[test]
    fn without_an_ask_the_bid_alone_decides() {
        let mut c = Classifier::default();
        c.push(Some(100), None);
        assert_eq!(c.push(Some(101), None), Some((101, Side::Buy)));
        assert_eq!(c.push(Some(101), None), None);
    }

    #[test]
    fn nothing_is_counted_before_a_bid_is_known() {
        let mut c = Classifier::default();
        assert_eq!(c.push(None, Some(102)), None);
        assert_eq!(c.push(None, Some(103)), None);
    }

    #[test]
    fn a_quote_belongs_to_the_bar_it_falls_in() {
        let times = [0, 60_000, 120_000];
        assert_eq!(bar_open(&times, Some(60_000), 0), Some(0));
        assert_eq!(bar_open(&times, Some(60_000), 59_999), Some(0));
        assert_eq!(bar_open(&times, Some(60_000), 60_000), Some(60_000));
        assert_eq!(bar_open(&times, Some(60_000), 179_999), Some(120_000));
        assert_eq!(bar_open(&times, Some(60_000), -1), None);
        assert_eq!(bar_open(&[], Some(60_000), 5), None);
    }

    #[test]
    fn a_quote_past_the_newest_bar_goes_to_the_bar_that_opens_next() {
        let times = [0, 60_000];
        assert_eq!(bar_open(&times, Some(60_000), 125_000), Some(120_000));
        assert_eq!(bar_open(&times, Some(60_000), 300_500), Some(300_000));
        assert_eq!(bar_open(&times, None, 300_500), Some(60_000));
    }

    #[test]
    fn a_history_is_counted_per_bar_and_per_price() {
        let times = [0, 1_000];
        let quotes = [
            q(100, 100, 102),
            q(200, 101, 103),  // buy at 101
            q(300, 101, 104),  // spread only in the middle: 101+104 = 205 > 204: buy
            q(1_100, 99, 101), // second bar: sell at 99
            q(1_200, 99, 101), // nothing
        ];
        let mut flow = Flow::default();
        flow.ingest(&times, Some(1_000), &quotes, true);
        let first = flow.bar(0).unwrap();
        assert_eq!(
            first.levels,
            vec![Level {
                price: 101,
                sell: 0,
                buy: 2
            }]
        );
        let second = flow.bar(1_000).unwrap();
        assert_eq!(
            second.levels,
            vec![Level {
                price: 99,
                sell: 1,
                buy: 0
            }]
        );
        assert_eq!(first.totals(), (0, 2));
        assert_eq!(flow.last_ms(), 1_200);
    }

    #[test]
    fn levels_stay_sorted_by_price() {
        let mut bar = BarFlow::new(0);
        for price in [5, 1, 3, 1, 5] {
            bar.add(price, Side::Buy);
        }
        let prices: Vec<i64> = bar.levels.iter().map(|l| l.price).collect();
        assert_eq!(prices, vec![1, 3, 5]);
        assert_eq!(bar.levels[0].buy, 2);
    }

    #[test]
    fn live_quotes_carry_on_from_the_history() {
        let times = [0];
        let mut flow = Flow::default();
        flow.ingest(
            &times,
            Some(60_000),
            &[q(10, 100, 102), q(20, 100, 102)],
            true,
        );
        // The last state was (100, 102): a bid of 101 with the ask carried is a buy.
        flow.push_live(0, 30, Some(101), None);
        assert_eq!(flow.bar(0).unwrap().totals(), (0, 1));
        assert_eq!(flow.last_ms(), 30);
    }

    #[test]
    fn a_refill_skips_what_was_already_counted() {
        let times = [0];
        let mut flow = Flow::default();
        flow.ingest(
            &times,
            Some(60_000),
            &[q(10, 100, 102), q(20, 101, 103)],
            true,
        );
        assert_eq!(flow.bar(0).unwrap().totals(), (0, 1));
        // The same quotes again, and one new: only the new one counts.
        flow.ingest(
            &times,
            Some(60_000),
            &[q(10, 100, 102), q(20, 101, 103), q(30, 102, 104)],
            true,
        );
        assert_eq!(flow.bar(0).unwrap().totals(), (0, 2));
    }

    #[test]
    fn an_older_batch_does_not_touch_the_newest_state() {
        let times = [0, 1_000];
        let mut flow = Flow::default();
        flow.ingest(
            &times,
            Some(1_000),
            &[q(1_010, 100, 102), q(1_020, 101, 103)],
            true,
        );
        let last = flow.last_ms();
        flow.ingest(&times, Some(1_000), &[q(10, 50, 52), q(20, 49, 51)], false);
        assert_eq!(flow.last_ms(), last);
        assert_eq!(flow.bar(0).unwrap().totals(), (1, 0));
        assert_eq!(flow.bar(1_000).unwrap().totals(), (0, 1));
    }

    fn ask(
        times: &[i64],
        covered: Option<i64>,
        len: usize,
        want: usize,
        per_chunk: usize,
        gap_after: Option<i64>,
        now: i64,
    ) -> Option<Step> {
        let held = Held {
            covered,
            len,
            gap_after,
        };
        next_step(times, |t| *t, held, want, per_chunk, now)
    }

    fn minutes(n: usize) -> Vec<i64> {
        (0..n as i64).map(|i| i * 60_000).collect()
    }

    #[test]
    fn nothing_held_asks_for_the_newest_bars_first() {
        let times = minutes(500);
        // The screen needs bar 100 but a request covers 40 bars: the newest ones come first.
        let step = ask(&times, None, 0, 100, 40, None, 99_000_000).unwrap();
        assert_eq!((step.start, step.end, step.newest), (460, 500, true));
        assert_eq!(step.from, times[460]);
        assert_eq!(step.to, 99_000_000);
        assert_eq!(step.covers, times[460]);
        // A screen that only needs the newest few takes no more than it needs.
        let step = ask(&times, None, 0, 495, 40, None, 99_000_000).unwrap();
        assert_eq!((step.start, step.end), (495, 500));
    }

    #[test]
    fn older_flow_is_asked_for_until_the_screen_is_covered() {
        let times = minutes(500);
        let mut covered = times[460];
        let mut held = 40;
        let mut asked = Vec::new();
        while let Some(step) = ask(&times, Some(covered), held, 100, 40, None, 0) {
            assert!(!step.newest);
            assert_eq!(step.to, covered - 1, "no gap, no overlap");
            assert!(step.start < step.end && step.end - step.start <= 40);
            asked.push((step.start, step.end));
            held += step.end - step.start;
            covered = step.covers;
        }
        assert_eq!(asked.first(), Some(&(420, 460)));
        assert_eq!(asked.last().map(|s| s.0), Some(100));
        assert_eq!(asked.len(), 9);
    }

    #[test]
    fn nothing_older_is_asked_for_at_the_first_bar_or_past_the_limit() {
        let times = minutes(100);
        assert_eq!(ask(&times, Some(times[0]), 100, 0, 40, None, 0), None);
        assert_eq!(ask(&times, Some(times[50]), 50, 60, 40, None, 0), None);
        assert_eq!(
            ask(&times, Some(times[50]), MAX_FLOW_BARS, 0, 40, None, 0),
            None,
            "the flow is as long as it may be"
        );
        assert_eq!(ask(&[], None, 0, 0, 40, None, 0), None);
    }

    #[test]
    fn a_gap_is_asked_for_from_the_last_quote_to_now() {
        let times = minutes(100);
        let last = times[97] + 1_000;
        let step = ask(&times, Some(times[60]), 40, 0, 40, Some(last), 7_000_000).unwrap();
        assert!(step.newest);
        assert_eq!((step.from, step.to), (last, 7_000_000));
        // It starts at the bar the last quote fell in, so the quotes after it find their bar.
        assert_eq!((step.start, step.end), (97, 100));
        assert_eq!(
            step.covers, times[60],
            "the flow held is not made shorter or longer"
        );
    }

    #[test]
    fn coverage_only_grows_backwards() {
        let mut flow = Flow::default();
        assert!(!flow.is_loaded());
        flow.cover_from(500);
        flow.cover_from(900);
        assert_eq!(flow.covered_from(), Some(500));
        flow.cover_from(100);
        assert_eq!(flow.covered_from(), Some(100));
    }

    #[test]
    fn the_oldest_bars_are_dropped_past_the_limit() {
        let mut flow = Flow::default();
        flow.cover_from(0);
        for i in 0..(MAX_FLOW_BARS as i64 + 10) {
            flow.bar_mut(i * 1_000).add(1, Side::Buy);
        }
        flow.trim();
        assert_eq!(flow.len(), MAX_FLOW_BARS);
        assert_eq!(flow.covered_from(), Some(10_000));
        assert!(flow.bar(0).is_none());
    }
}

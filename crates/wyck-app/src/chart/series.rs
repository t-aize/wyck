//! A run of bars for one symbol and one period, and the rules for keeping it right.
//!
//! [`Series`] is what the chart reads: bars sorted by open time, one per open time. It grows in
//! three ways, and each keeps that invariant:
//!
//! - [`Series::merge`] adds bars fetched from the server or read from the cache (older history,
//!   or a fresher tail). A bar already present is replaced, because the server's later answer for
//!   the same open time is the better one (the forming bar changes until it closes).
//! - [`Series::apply_price`] folds a live price into the last bar, and opens a new bar when the
//!   price belongs to the period after it.
//! - [`Series::trim_front`] drops the oldest bars once the series is longer than the memory
//!   budget.
//!
//! # Where a new bar starts
//!
//! A live price opens the next bar at `last.time + k * period`, counted from the last bar the
//! server gave. No time zone or session rule is assumed: brokers cut their days differently, and
//! counting from a real bar is always consistent with how that broker cut this series. After a
//! weekend gap the guess can be off by a session; the next fetch replaces it with the server's
//! own bar. A calendar month has no fixed length, so it is advanced with calendar arithmetic.

use time::{Date, Month, OffsetDateTime};
use wyck_engine::domain::{Candle, Period, UnixMillis};

const MINUTE_MS: i64 = 60_000;
const DAY_MS: i64 = 24 * 60 * MINUTE_MS;

/// The fixed length of a period in milliseconds, or `None` for a calendar month, whose length
/// depends on which month it is.
#[must_use]
pub fn period_millis(period: Period) -> Option<i64> {
    match period {
        Period::MN1 => None,
        other => Some(other.approx_minutes() * MINUTE_MS),
    }
}

/// The open time of the bar after the one opening at `time`.
///
/// For a month this is the first instant of the next calendar month at the same time of day
/// (UTC), which follows a broker that cuts months at a fixed hour.
#[must_use]
pub fn next_open(time: UnixMillis, period: Period) -> UnixMillis {
    if let Some(length) = period_millis(period) {
        return time + length;
    }
    let Ok(start) = OffsetDateTime::from_unix_timestamp_nanos(i128::from(time) * 1_000_000) else {
        return time + 30 * DAY_MS;
    };
    let (year, month) = match start.month() {
        Month::December => (start.year() + 1, Month::January),
        other => (start.year(), other.next()),
    };
    let Ok(date) = Date::from_calendar_date(year, month, 1) else {
        return time + 30 * DAY_MS;
    };
    let next = date.with_time(start.time()).assume_utc();
    (next.unix_timestamp_nanos() / 1_000_000) as i64
}

/// Bars of one symbol at one period: sorted by open time, one bar per open time.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Series {
    bars: Vec<Candle>,
}

impl Series {
    /// An empty series.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A series from bars in any order. Broken bars (see [`Candle::is_sane`]) are dropped and a
    /// repeated open time keeps its last bar.
    #[must_use]
    pub fn from_bars(bars: impl IntoIterator<Item = Candle>) -> Self {
        let mut series = Self::new();
        series.merge(bars);
        series
    }

    /// The bars, oldest first.
    #[must_use]
    pub fn bars(&self) -> &[Candle] {
        &self.bars
    }

    /// How many bars there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bars.len()
    }

    /// Whether there are no bars.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    /// The newest bar.
    #[must_use]
    pub fn last(&self) -> Option<&Candle> {
        self.bars.last()
    }

    /// The open time of the oldest bar.
    #[must_use]
    pub fn first_time(&self) -> Option<UnixMillis> {
        self.bars.first().map(|bar| bar.time)
    }

    /// The index of the first bar opening at or after `time`, or `len()` when every bar opens
    /// before it. This is the lower bound the chart uses to find what is on screen.
    #[must_use]
    pub fn lower_bound(&self, time: UnixMillis) -> usize {
        self.bars.partition_point(|bar| bar.time < time)
    }

    /// Adds bars. A bar with an open time already present replaces the old one; broken bars are
    /// skipped. Returns whether anything changed, so a caller can skip a repaint.
    pub fn merge(&mut self, incoming: impl IntoIterator<Item = Candle>) -> bool {
        let mut incoming: Vec<Candle> = incoming.into_iter().filter(Candle::is_sane).collect();
        if incoming.is_empty() {
            return false;
        }
        // A stable sort keeps the arrival order of equal times, so after reversing, the first
        // of each run is the latest arrival and `dedup` keeps it.
        incoming.sort_by_key(|bar| bar.time);
        incoming.reverse();
        incoming.dedup_by_key(|bar| bar.time);
        incoming.reverse();

        // The common case, a tail top-up, only appends.
        if self
            .bars
            .last()
            .is_none_or(|last| incoming[0].time > last.time)
        {
            self.bars.extend(incoming);
            return true;
        }

        let mut merged = Vec::with_capacity(self.bars.len() + incoming.len());
        let (mut old, mut new) = (
            self.bars.iter().copied().peekable(),
            incoming.into_iter().peekable(),
        );
        while let (Some(a), Some(b)) = (old.peek(), new.peek()) {
            if a.time < b.time {
                merged.extend(old.next());
            } else if a.time > b.time {
                merged.extend(new.next());
            } else {
                old.next();
                merged.extend(new.next());
            }
        }
        merged.extend(old);
        merged.extend(new);

        let changed = merged != self.bars;
        self.bars = merged;
        changed
    }

    /// Folds a live price seen at `at` into the series.
    ///
    /// The price joins the last bar when `at` is inside it, and opens the following bar
    /// (see the module docs for where that starts) when `at` is past its end. A price older
    /// than the last bar's open, or a series with no bar to anchor to, changes nothing. Returns
    /// whether the series changed.
    pub fn apply_price(&mut self, period: Period, price: f64, at: UnixMillis) -> bool {
        if !price.is_finite() {
            return false;
        }
        let Some(last) = self.bars.last().copied() else {
            return false;
        };
        if at < last.time {
            return false;
        }
        let mut open = last.time;
        let mut next = next_open(open, period);
        if at < next {
            let mut bar = last;
            bar.apply_price(price);
            let changed = bar != last;
            if let Some(slot) = self.bars.last_mut() {
                *slot = bar;
            }
            return changed;
        }
        // Skip whole periods until `at` falls inside one. A fixed period is jumped in one step,
        // a month is stepped, so a long gap costs no loop for the common periods.
        match period_millis(period) {
            Some(length) => open += (at - open) / length * length,
            None => {
                while next <= at {
                    open = next;
                    next = next_open(open, period);
                }
            }
        }
        self.bars.push(Candle::at(open, price));
        true
    }

    /// Drops the oldest bars so at most `keep` remain. Returns how many were dropped.
    pub fn trim_front(&mut self, keep: usize) -> usize {
        let drop = self.bars.len().saturating_sub(keep);
        self.bars.drain(..drop);
        drop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: i64 = MINUTE_MS;

    fn bar(minute: i64, price: f64) -> Candle {
        Candle::at(minute * M, price)
    }

    fn times(series: &Series) -> Vec<i64> {
        series.bars().iter().map(|b| b.time / M).collect()
    }

    #[test]
    fn bars_come_out_sorted_and_unique() {
        let series = Series::from_bars([bar(2, 1.0), bar(0, 1.0), bar(1, 1.0), bar(1, 2.0)]);
        assert_eq!(times(&series), vec![0, 1, 2]);
        assert_eq!(series.bars()[1].close, 2.0, "the later duplicate wins");
    }

    #[test]
    fn broken_bars_are_dropped() {
        let mut broken = bar(0, 1.0);
        broken.high = f64::NAN;
        assert!(Series::from_bars([broken]).is_empty());
    }

    #[test]
    fn older_history_merges_in_front_and_a_new_answer_replaces() {
        let mut series = Series::from_bars([bar(10, 1.0), bar(11, 1.0)]);
        assert!(series.merge([bar(8, 1.0), bar(9, 1.0), bar(11, 5.0)]));
        assert_eq!(times(&series), vec![8, 9, 10, 11]);
        assert_eq!(series.bars()[3].close, 5.0);
        assert!(
            !series.merge([bar(9, 1.0)]),
            "the same bar again is no change"
        );
    }

    #[test]
    fn a_tail_top_up_appends() {
        let mut series = Series::from_bars([bar(0, 1.0)]);
        assert!(series.merge([bar(1, 1.0), bar(2, 1.0)]));
        assert_eq!(times(&series), vec![0, 1, 2]);
    }

    #[test]
    fn lower_bound_finds_the_first_bar_at_or_after() {
        let series = Series::from_bars([bar(0, 1.0), bar(5, 1.0), bar(10, 1.0)]);
        assert_eq!(series.lower_bound(-1), 0);
        assert_eq!(series.lower_bound(5 * M), 1);
        assert_eq!(series.lower_bound(6 * M), 2);
        assert_eq!(series.lower_bound(11 * M), 3);
    }

    #[test]
    fn a_price_inside_the_last_bar_stretches_it() {
        let mut series = Series::from_bars([bar(0, 1.0)]);
        assert!(series.apply_price(Period::M1, 1.5, 30_000));
        let last = series.last().unwrap();
        assert_eq!((last.high, last.close), (1.5, 1.5));
        assert!(
            !series.apply_price(Period::M1, 1.5, 31_000),
            "same price, no change"
        );
    }

    #[test]
    fn a_price_past_the_end_opens_the_next_bar() {
        let mut series = Series::from_bars([bar(0, 1.0)]);
        assert!(series.apply_price(Period::M1, 2.0, M + 5));
        assert_eq!(times(&series), vec![0, 1]);
        assert_eq!(series.last().unwrap().open, 2.0);
    }

    #[test]
    fn a_price_after_a_gap_lands_on_the_period_grid() {
        let mut series = Series::from_bars([bar(0, 1.0)]);
        series.apply_price(Period::M5, 2.0, 27 * M);
        assert_eq!(series.last().unwrap().time, 25 * M);
    }

    #[test]
    fn a_price_older_than_the_last_bar_or_without_an_anchor_is_ignored() {
        let mut series = Series::from_bars([bar(5, 1.0)]);
        assert!(!series.apply_price(Period::M1, 2.0, 4 * M));
        assert!(!Series::new().apply_price(Period::M1, 2.0, 0));
        assert!(!series.apply_price(Period::M1, f64::NAN, 5 * M));
    }

    #[test]
    fn a_month_advances_by_the_calendar() {
        // 2026-01-01 00:00 UTC to 2026-02-01 00:00 UTC.
        let jan = 1_767_225_600_000;
        let feb = 1_769_904_000_000;
        assert_eq!(next_open(jan, Period::MN1), feb);
        // December rolls into January of the next year.
        let dec = 1_796_083_200_000; // 2026-12-01
        let next_jan = 1_798_761_600_000; // 2027-01-01
        assert_eq!(next_open(dec, Period::MN1), next_jan);
    }

    #[test]
    fn trimming_keeps_the_newest() {
        let mut series = Series::from_bars((0..10).map(|i| bar(i, 1.0)));
        assert_eq!(series.trim_front(4), 6);
        assert_eq!(times(&series), vec![6, 7, 8, 9]);
        assert_eq!(series.trim_front(100), 0);
    }
}

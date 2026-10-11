//! What the chart holds and how it grows: bars or ticks, kept sorted and bounded.
//!
//! Everything here is plain data in, plain data out, so the rules that keep a live chart correct
//! (which bar a tick belongs to, how a page of older history joins, what a reconnect refill
//! replaces) are unit tested without a window.

use crate::domain::market::{Bar, Tick};

use super::timeframe::Timeframe;

/// The most bars kept. Older ones are dropped as new ones arrive, so a chart left open for days
/// does not grow without end.
pub const MAX_BARS: usize = 120_000;
/// The most ticks kept.
pub const MAX_TICKS: usize = 250_000;

#[derive(Debug, Clone)]
pub enum Series {
    Bars(Vec<Bar>),
    Ticks(Vec<Tick>),
}

impl Series {
    pub fn len(&self) -> usize {
        match self {
            Self::Bars(bars) => bars.len(),
            Self::Ticks(ticks) => ticks.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The most a series may hold.
    pub fn capacity_limit(&self) -> usize {
        match self {
            Self::Bars(_) => MAX_BARS,
            Self::Ticks(_) => MAX_TICKS,
        }
    }

    /// The time of point `index`, in Unix milliseconds.
    pub fn time_at(&self, index: usize) -> Option<i64> {
        match self {
            Self::Bars(bars) => bars.get(index).map(|b| b.time_ms),
            Self::Ticks(ticks) => ticks.get(index).map(|t| t.time_ms),
        }
    }

    /// The time of the first point.
    pub fn first_time(&self) -> Option<i64> {
        self.time_at(0)
    }

    /// The time of the last point.
    pub fn last_time(&self) -> Option<i64> {
        self.len().checked_sub(1).and_then(|i| self.time_at(i))
    }

    /// The last price: a bar's close, or the tick's price.
    pub fn last_price(&self) -> Option<i64> {
        match self {
            Self::Bars(bars) => bars.last().map(|b| b.close),
            Self::Ticks(ticks) => ticks.last().map(|t| t.price),
        }
    }

    /// The value a point is plotted at in a line: a bar's close, or a tick's price.
    pub fn value_at(&self, index: usize) -> Option<i64> {
        match self {
            Self::Bars(bars) => bars.get(index).map(|b| b.close),
            Self::Ticks(ticks) => ticks.get(index).map(|t| t.price),
        }
    }

    /// How many points have a time at or before `time_ms`.
    fn count_up_to(&self, time_ms: i64) -> usize {
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.time_at(mid).is_some_and(|t| t <= time_ms) {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// The time between two points: the timeframe's own length for bars, the average for ticks.
    pub fn step_ms(&self, nominal: Option<i64>) -> f64 {
        if let Some(nominal) = nominal {
            return nominal.max(1) as f64;
        }
        match (self.first_time(), self.last_time(), self.len()) {
            (Some(a), Some(b), n) if n > 1 => ((b - a) as f64 / (n - 1) as f64).max(1.0),
            _ => 1_000.0,
        }
    }

    /// Where a time falls among the points, as a fractional index: between two points it is
    /// interpolated, before the first and after the last it continues at `step_ms` a point.
    pub fn index_of_time(&self, time_ms: i64, step_ms: f64) -> Option<f64> {
        let n = self.len();
        let (first, last) = (self.first_time()?, self.last_time()?);
        let up_to = self.count_up_to(time_ms);
        Some(if up_to == 0 {
            -((first - time_ms) as f64) / step_ms
        } else if up_to == n {
            (n - 1) as f64 + (time_ms - last) as f64 / step_ms
        } else {
            let (a, b) = (self.time_at(up_to - 1)?, self.time_at(up_to)?);
            (up_to - 1) as f64 + (time_ms - a) as f64 / ((b - a).max(1)) as f64
        })
    }

    /// The time of a fractional index; the inverse of [`Series::index_of_time`].
    pub fn time_of_index(&self, index: f64, step_ms: f64) -> Option<i64> {
        let n = self.len();
        let (first, last) = (self.first_time()?, self.last_time()?);
        if !index.is_finite() {
            return None;
        }
        Some(if index <= 0.0 {
            first + (index * step_ms).round() as i64
        } else if index >= (n - 1) as f64 {
            last + ((index - (n - 1) as f64) * step_ms).round() as i64
        } else {
            let i = index.floor() as usize;
            let (a, b) = (self.time_at(i)?, self.time_at(i + 1)?);
            a + ((index - i as f64) * (b - a) as f64).round() as i64
        })
    }

    /// The lowest and highest price in `[first, last)`. With `wicks` a bar counts by its low and
    /// high, otherwise by its close.
    pub fn price_range(&self, first: usize, last: usize, wicks: bool) -> Option<(i64, i64)> {
        let (mut lo, mut hi) = (i64::MAX, i64::MIN);
        match self {
            Self::Bars(bars) => {
                for bar in bars.get(first..last.min(bars.len()))? {
                    let (l, h) = if wicks {
                        (bar.low, bar.high)
                    } else {
                        (bar.close, bar.close)
                    };
                    lo = lo.min(l);
                    hi = hi.max(h);
                }
            }
            Self::Ticks(ticks) => {
                for tick in ticks.get(first..last.min(ticks.len()))? {
                    lo = lo.min(tick.price);
                    hi = hi.max(tick.price);
                }
            }
        }
        (lo <= hi).then_some((lo, hi))
    }
}

/// The start of the bucket of `bucket_ms` that holds `time_ms`.
pub fn bucket_start(time_ms: i64, bucket_ms: i64) -> i64 {
    time_ms - time_ms.rem_euclid(bucket_ms)
}

/// Groups ticks (oldest first) into bars of `bucket_ms`. A bucket without ticks has no bar.
pub fn aggregate_ticks(ticks: &[Tick], bucket_ms: i64) -> Vec<Bar> {
    let mut bars: Vec<Bar> = Vec::new();
    for tick in ticks {
        fold_tick(&mut bars, bucket_ms, *tick);
    }
    bars
}

/// Adds a tick to bars built from ticks: into the last bar when it falls in its bucket, or as a
/// new bar after it. A tick older than the last bar (a clock that stepped back) is folded into the
/// last bar rather than breaking the order. Returns whether a bar was added.
pub fn fold_tick(bars: &mut Vec<Bar>, bucket_ms: i64, tick: Tick) -> bool {
    let bucket = bucket_start(tick.time_ms, bucket_ms);
    match bars.last_mut() {
        Some(last) if bucket <= last.time_ms => {
            last.high = last.high.max(tick.price);
            last.low = last.low.min(tick.price);
            last.close = tick.price;
            last.volume += 1;
            false
        }
        _ => {
            bars.push(Bar {
                time_ms: bucket,
                open: tick.price,
                high: tick.price,
                low: tick.price,
                close: tick.price,
                volume: 1,
            });
            true
        }
    }
}

/// Moves the last bar of a server period with a tick, when the tick belongs to it. The server's
/// own live bar decides when a new bar starts, so a tick past the bar is left alone.
pub fn touch_last_bar(bars: &mut [Bar], period_ms: i64, tick: Tick) {
    if let Some(last) = bars.last_mut()
        && tick.time_ms >= last.time_ms
        && tick.time_ms < last.time_ms + period_ms
    {
        last.high = last.high.max(tick.price);
        last.low = last.low.min(tick.price);
        last.close = tick.price;
    }
}

/// Puts a live bar from the server in place: replaces the bar with its time, or adds it after the
/// last. Returns whether a bar was added at the end. A bar older than the first one kept is dropped.
pub fn apply_live_bar(bars: &mut Vec<Bar>, live: Bar) -> bool {
    match bars.binary_search_by_key(&live.time_ms, |b| b.time_ms) {
        Ok(at) => {
            bars[at] = live;
            false
        }
        Err(at) if at == bars.len() => {
            bars.push(live);
            true
        }
        Err(0) => false,
        Err(at) => {
            bars.insert(at, live);
            false
        }
    }
}

/// Groups server bars (oldest first) into the bars of a grouped timeframe (see
/// [`Timeframe::group_key`]). A group opens at its own boundary, or at its first bar for weeks
/// and months.
pub fn group_bars(bars: &[Bar], timeframe: Timeframe) -> Vec<Bar> {
    let mut grouped: Vec<Bar> = Vec::new();
    let mut last_key = None;
    for bar in bars {
        let key = timeframe.group_key(bar.time_ms);
        match grouped.last_mut() {
            Some(group) if last_key == Some(key) => {
                group.high = group.high.max(bar.high);
                group.low = group.low.min(bar.low);
                group.close = bar.close;
                group.volume += bar.volume;
            }
            _ => {
                grouped.push(Bar {
                    time_ms: timeframe.group_open(bar.time_ms).unwrap_or(bar.time_ms),
                    ..*bar
                });
                last_key = Some(key);
            }
        }
    }
    grouped
}

/// The server bars (oldest first) of the newest group.
pub fn last_group(bars: &[Bar], timeframe: Timeframe) -> Vec<Bar> {
    let Some(last) = bars.last() else {
        return Vec::new();
    };
    let key = timeframe.group_key(last.time_ms);
    let start = bars
        .iter()
        .rposition(|b| timeframe.group_key(b.time_ms) != key)
        .map_or(0, |at| at + 1);
    bars[start..].to_vec()
}

/// A live server bar for a grouped timeframe: it joins `tail` (the server bars of the newest
/// group) and the newest grouped bar is built again from them, or starts the next one. Returns
/// whether a grouped bar was added at the end.
pub fn fold_group(
    bars: &mut Vec<Bar>,
    tail: &mut Vec<Bar>,
    live: Bar,
    timeframe: Timeframe,
) -> bool {
    let key = timeframe.group_key(live.time_ms);
    match tail.first().map(|b| timeframe.group_key(b.time_ms)) {
        Some(held) if key < held => return false,
        Some(held) if key == held => {
            apply_live_bar(tail, live);
        }
        _ => *tail = vec![live],
    }
    let Some(group) = group_bars(tail, timeframe).pop() else {
        return false;
    };
    match bars.last_mut() {
        Some(last) if last.time_ms == group.time_ms => {
            *last = group;
            false
        }
        Some(last) if last.time_ms > group.time_ms => false,
        _ => {
            bars.push(group);
            true
        }
    }
}

/// Joins bars fetched for a range that overlaps what is held (a refill after a reconnect). Where a
/// bar exists on both sides the one with more ticks wins: a bar only ever gains ticks, so that is
/// the fresher one, whichever arrived last.
pub fn merge_bars(bars: &mut Vec<Bar>, fetched: Vec<Bar>) {
    if fetched.is_empty() {
        return;
    }
    let mut merged: Vec<Bar> = Vec::with_capacity(bars.len() + fetched.len());
    let (mut a, mut b) = (0, 0);
    while a < bars.len() || b < fetched.len() {
        match (bars.get(a), fetched.get(b)) {
            (Some(x), Some(y)) if x.time_ms == y.time_ms => {
                merged.push(if y.volume > x.volume { *y } else { *x });
                a += 1;
                b += 1;
            }
            (Some(x), Some(y)) if x.time_ms < y.time_ms => {
                merged.push(*x);
                a += 1;
            }
            (Some(_), Some(y)) => {
                merged.push(*y);
                b += 1;
            }
            (Some(x), None) => {
                merged.push(*x);
                a += 1;
            }
            (None, Some(y)) => {
                merged.push(*y);
                b += 1;
            }
            (None, None) => break,
        }
    }
    *bars = merged;
}

/// Puts bars older than everything held in front. Returns how many were added.
pub fn prepend_bars(bars: &mut Vec<Bar>, mut older: Vec<Bar>) -> usize {
    let first = bars.first().map_or(i64::MAX, |b| b.time_ms);
    older.retain(|b| b.time_ms < first);
    older.sort_by_key(|b| b.time_ms);
    older.dedup_by_key(|b| b.time_ms);
    let added = older.len();
    if added > 0 {
        older.append(bars);
        *bars = older;
    }
    added
}

/// Puts ticks older than everything held in front. Returns how many were added.
pub fn prepend_ticks(ticks: &mut Vec<Tick>, mut older: Vec<Tick>) -> usize {
    let first = ticks.first().map_or(i64::MAX, |t| t.time_ms);
    older.retain(|t| t.time_ms < first);
    let added = older.len();
    if added > 0 {
        older.append(ticks);
        *ticks = older;
    }
    added
}

/// Joins ticks fetched to fill a gap: the fetched ticks stand for everything after `after_ms` up
/// to and including `until_ms`, and ticks that arrived live past `until_ms` are kept after them.
pub fn splice_ticks(ticks: &mut Vec<Tick>, fetched: Vec<Tick>, after_ms: i64, until_ms: i64) {
    let keep_before = ticks.partition_point(|t| t.time_ms <= after_ms);
    let keep_after = ticks.partition_point(|t| t.time_ms <= until_ms);
    let tail = ticks.split_off(keep_after);
    ticks.truncate(keep_before);
    ticks.extend(
        fetched
            .into_iter()
            .filter(|t| t.time_ms > after_ms && t.time_ms <= until_ms),
    );
    ticks.extend(tail);
}

/// Drops the oldest points beyond `max`. Returns how many went.
pub fn trim_front<T>(items: &mut Vec<T>, max: usize) -> usize {
    let excess = items.len().saturating_sub(max);
    if excess > 0 {
        items.drain(..excess);
    }
    excess
}

#[cfg(test)]
#[path = "../../../tests/unit/domain/chart/data.rs"]
mod tests;

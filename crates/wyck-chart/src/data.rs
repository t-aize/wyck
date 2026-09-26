//! What the chart holds and how it grows: bars or ticks, kept sorted and bounded.
//!
//! Everything here is plain data in, plain data out, so the rules that keep a live chart correct
//! (which bar a tick belongs to, how a page of older history joins, what a reconnect refill
//! replaces) are unit tested without a window.

use wyck_openapi_model::market::{Bar, Tick};

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
mod tests {
    use super::*;

    fn bar(time_ms: i64, o: i64, h: i64, l: i64, c: i64, volume: i64) -> Bar {
        Bar {
            time_ms,
            open: o,
            high: h,
            low: l,
            close: c,
            volume,
        }
    }

    fn tick(time_ms: i64, price: i64) -> Tick {
        Tick { time_ms, price }
    }

    #[test]
    fn ticks_group_into_buckets() {
        let ticks = [
            tick(1_000, 10),
            tick(1_400, 14),
            tick(1_900, 8),
            tick(2_100, 9),
        ];
        let bars = aggregate_ticks(&ticks, 1_000);
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0], bar(1_000, 10, 14, 8, 8, 3));
        assert_eq!(bars[1], bar(2_000, 9, 9, 9, 9, 1));
    }

    #[test]
    fn buckets_align_before_the_epoch_too() {
        assert_eq!(bucket_start(-1, 1_000), -1_000);
        assert_eq!(bucket_start(1_999, 1_000), 1_000);
    }

    #[test]
    fn a_late_tick_never_breaks_the_order() {
        let mut bars = vec![bar(5_000, 1, 1, 1, 1, 1)];
        assert!(!fold_tick(&mut bars, 1_000, tick(3_000, 5)));
        assert_eq!(bars.len(), 1);
        assert_eq!((bars[0].high, bars[0].close), (5, 5));
        assert!(fold_tick(&mut bars, 1_000, tick(6_000, 2)));
        assert_eq!(bars.len(), 2);
    }

    #[test]
    fn a_tick_only_touches_the_bar_it_belongs_to() {
        let mut bars = vec![bar(60_000, 10, 12, 9, 11, 5)];
        touch_last_bar(&mut bars, 60_000, tick(90_000, 15));
        assert_eq!((bars[0].high, bars[0].close, bars[0].volume), (15, 15, 5));
        touch_last_bar(&mut bars, 60_000, tick(120_000, 99));
        assert_eq!(
            bars[0].high, 15,
            "a tick past the bar waits for its live bar"
        );
        touch_last_bar(&mut bars, 60_000, tick(59_999, 1));
        assert_eq!(bars[0].low, 9, "a tick before the bar is not its own");
    }

    #[test]
    fn a_live_bar_replaces_or_appends() {
        let mut bars = vec![bar(0, 1, 1, 1, 1, 1), bar(60, 1, 1, 1, 1, 1)];
        assert!(!apply_live_bar(&mut bars, bar(60, 1, 3, 1, 2, 4)));
        assert_eq!(bars[1].volume, 4);
        assert!(apply_live_bar(&mut bars, bar(120, 2, 2, 2, 2, 1)));
        assert_eq!(bars.len(), 3);
    }

    #[test]
    fn a_live_bar_before_everything_is_dropped() {
        let mut bars = vec![bar(60, 1, 1, 1, 1, 1)];
        assert!(!apply_live_bar(&mut bars, bar(0, 1, 1, 1, 1, 1)));
        assert_eq!(bars.len(), 1);
    }

    #[test]
    fn merging_keeps_the_bar_with_more_ticks() {
        let mut bars = vec![bar(0, 1, 1, 1, 1, 9), bar(60, 1, 1, 1, 1, 2)];
        merge_bars(
            &mut bars,
            vec![
                bar(0, 1, 2, 1, 2, 3),
                bar(60, 1, 5, 1, 5, 7),
                bar(120, 1, 1, 1, 1, 1),
            ],
        );
        assert_eq!(bars.len(), 3);
        assert_eq!(bars[0].volume, 9);
        assert_eq!(bars[1].volume, 7);
        assert!(bars.windows(2).all(|w| w[0].time_ms < w[1].time_ms));
    }

    #[test]
    fn older_bars_go_in_front_without_duplicates() {
        let mut bars = vec![bar(100, 1, 1, 1, 1, 1)];
        let added = prepend_bars(
            &mut bars,
            vec![
                bar(100, 9, 9, 9, 9, 9),
                bar(50, 1, 1, 1, 1, 1),
                bar(10, 1, 1, 1, 1, 1),
                bar(10, 1, 1, 1, 1, 1),
            ],
        );
        assert_eq!(added, 2);
        let times: Vec<i64> = bars.iter().map(|b| b.time_ms).collect();
        assert_eq!(times, vec![10, 50, 100]);
        assert_eq!(bars[2].open, 1, "the bar already held is not replaced");
    }

    #[test]
    fn a_gap_refill_replaces_the_gap_and_keeps_newer_live_ticks() {
        let mut ticks = vec![tick(1, 1), tick(2, 2), tick(9, 9), tick(12, 12)];
        splice_ticks(
            &mut ticks,
            vec![tick(3, 3), tick(5, 5), tick(10, 10), tick(11, 11)],
            2,
            10,
        );
        let times: Vec<i64> = ticks.iter().map(|t| t.time_ms).collect();
        assert_eq!(times, vec![1, 2, 3, 5, 10, 12]);
    }

    #[test]
    fn trimming_drops_the_oldest() {
        let mut items = vec![1, 2, 3, 4, 5];
        assert_eq!(trim_front(&mut items, 3), 2);
        assert_eq!(items, vec![3, 4, 5]);
        assert_eq!(trim_front(&mut items, 10), 0);
    }

    #[test]
    fn a_time_maps_to_an_index_and_back() {
        let series = Series::Bars(vec![
            bar(1_000, 1, 1, 1, 1, 1),
            bar(2_000, 1, 1, 1, 1, 1),
            bar(4_000, 1, 1, 1, 1, 1),
        ]);
        let step = 1_000.0;
        assert_eq!(series.index_of_time(1_500, step), Some(0.5));
        assert_eq!(
            series.index_of_time(3_000, step),
            Some(1.5),
            "a gap is spread over its points"
        );
        for time in [-3_000, 1_000, 1_250, 2_000, 3_999, 4_000, 9_000] {
            let index = series.index_of_time(time, step).unwrap();
            let back = series.time_of_index(index, step).unwrap();
            assert!((back - time).abs() <= 1, "{time} -> {index} -> {back}");
        }
        assert_eq!(series.index_of_time(0, step), Some(-1.0));
        assert_eq!(series.index_of_time(6_000, step), Some(4.0));
        assert_eq!(Series::Bars(Vec::new()).index_of_time(1, step), None);
    }

    #[test]
    fn the_step_of_ticks_is_their_average_spacing() {
        let ticks = Series::Ticks(vec![tick(0, 1), tick(100, 1), tick(300, 1)]);
        assert_eq!(ticks.step_ms(None), 150.0);
        assert_eq!(ticks.step_ms(Some(60_000)), 60_000.0);
        assert_eq!(Series::Ticks(Vec::new()).step_ms(None), 1_000.0);
    }

    #[test]
    fn the_price_range_follows_the_wicks_or_the_closes() {
        let series = Series::Bars(vec![bar(0, 5, 9, 1, 6, 1), bar(60, 6, 7, 4, 5, 1)]);
        assert_eq!(series.price_range(0, 2, true), Some((1, 9)));
        assert_eq!(series.price_range(0, 2, false), Some((5, 6)));
        assert_eq!(series.price_range(2, 2, true), None);
        assert_eq!(series.price_range(0, 99, true), Some((1, 9)));
    }

    #[test]
    fn server_bars_are_grouped_and_the_newest_group_follows_live_bars() {
        const H: i64 = 3_600_000;
        let h2 = Timeframe::from_code("H2").unwrap();
        let bar = |t: i64, price: i64| Bar {
            time_ms: t * H,
            open: price,
            high: price + 2,
            low: price - 2,
            close: price + 1,
            volume: 10,
        };
        let hourly = [bar(0, 100), bar(1, 110), bar(2, 90), bar(3, 95), bar(4, 80)];
        let mut grouped = group_bars(&hourly, h2);
        assert_eq!(grouped.len(), 3);
        assert_eq!(grouped[0].time_ms, 0);
        assert_eq!(
            (
                grouped[0].open,
                grouped[0].high,
                grouped[0].low,
                grouped[0].close
            ),
            (100, 112, 98, 111)
        );
        assert_eq!(grouped[0].volume, 20);
        assert_eq!(grouped[2].time_ms, 4 * H);
        let mut tail = last_group(&hourly, h2);
        assert_eq!(tail.len(), 1);
        // The 04:00 bar moves: the group follows without counting it twice.
        assert!(!fold_group(&mut grouped, &mut tail, bar(4, 70), h2));
        assert_eq!(grouped.len(), 3);
        assert_eq!(grouped[2].close, 71);
        assert_eq!(grouped[2].volume, 10);
        // 05:00 joins the 04:00 group, 06:00 starts the next one.
        assert!(!fold_group(&mut grouped, &mut tail, bar(5, 60), h2));
        assert_eq!(
            (grouped[2].open, grouped[2].low, grouped[2].close),
            (70, 58, 61)
        );
        assert!(fold_group(&mut grouped, &mut tail, bar(6, 65), h2));
        assert_eq!(grouped.len(), 4);
        assert_eq!(grouped[3].time_ms, 6 * H);
        // A bar of an older group is ignored.
        assert!(!fold_group(&mut grouped, &mut tail, bar(5, 1), h2));
        assert_eq!(grouped[2].close, 61);
    }
}

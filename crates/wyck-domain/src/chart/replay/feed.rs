//! The candles a chart shows at the replay cursor, written into its series.
//!
//! The candles that ended by the cursor are the chart's own bars from the server, so their
//! boundaries are the broker's. Only the candle in progress is rebuilt, from minute bars or, when
//! there are none, along the synthetic path of the candle itself.

use crate::market::{Bar, Period, Tick, Timeframe};

use super::forming::{replay_fold_hybrid, replay_fold_minutes};
use super::path::replay_partial_bar;
use crate::chart::data::group_bars;

/// Slack for a day that lasts 25 hours when the clocks change.
const DAY_SLACK_MS: i64 = 2 * 3_600_000;

/// What a chart replays.
#[derive(Debug, Clone, Copy)]
pub struct ReplayInput<'a> {
    pub timeframe: Timeframe,
    /// Every server bar of the timeframe's period, sorted, past the cursor included.
    pub base: &'a [Bar],
    /// The minute bars of the symbol, sorted. `None` forms a candle along its own path.
    pub minutes: Option<&'a [Bar]>,
    /// The real ticks around the cursor, sorted, with the minute bars: the minute in progress is
    /// then made of them. Without minute bars they are not used.
    pub ticks: Option<&'a [Tick]>,
    /// Changes when `base` was edited other than at its end (older history put in front).
    pub generation: u64,
}

/// Which part of the series a reveal rewrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayChange {
    pub old_len: usize,
    pub new_len: usize,
    /// The first index that may differ from before.
    pub first_changed: usize,
}

/// What the candle in progress is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplaySource {
    /// Real ticks for the minute in progress, minute bars before it.
    Ticks,
    /// Minute bars, the one in progress along a synthetic path.
    Minutes,
    /// The candle itself, along a synthetic path.
    #[default]
    Own,
}

/// What a chart remembers between two moves of the cursor, so a move costs what changed.
#[derive(Debug, Clone, Default)]
pub struct ReplayFeed {
    shown: usize,
    generation: u64,
    source: ReplaySource,
}

impl ReplayFeed {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets what was written: the next reveal rebuilds the whole series.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// What the candle in progress was made of the last time.
    #[must_use]
    pub fn source(&self) -> ReplaySource {
        self.source
    }

    /// Writes into `out` the candles of `input` as the cursor shows them. Moving forward and back
    /// rewrite only the tail, from the candle (or grouped candle) in progress.
    pub fn reveal_into(
        &mut self,
        out: &mut Vec<Bar>,
        input: &ReplayInput<'_>,
        cursor_ms: i64,
    ) -> ReplayChange {
        let old_len = out.len();
        let timeframe = input.timeframe;
        let base = input.base;
        let Some(period) = timeframe.period() else {
            out.clear();
            self.reset();
            return ReplayChange {
                old_len,
                new_len: 0,
                first_changed: 0,
            };
        };
        let count = base.partition_point(|b| b.time_ms < cursor_ms);
        let fresh = self.generation != input.generation || self.shown > base.len();
        let before = if fresh { 0 } else { self.shown };
        let first = group_start(base, before.min(count).saturating_sub(1), timeframe);
        let cut = if fresh {
            0
        } else {
            base.get(first).map_or(0, |bar| {
                let at = timeframe.group_open(bar.time_ms).unwrap_or(bar.time_ms);
                out.partition_point(|b| b.time_ms < at)
            })
        };
        let (closed, forming) = split_last(input, period, count, cursor_ms);
        let mut slice: Vec<Bar> = base
            .get(first..closed)
            .map(<[Bar]>::to_vec)
            .unwrap_or_default();
        slice.extend(forming);
        out.truncate(cut);
        out.extend(group_bars(&slice, timeframe));
        self.shown = count;
        self.source = match (input.minutes, input.ticks) {
            (Some(_), Some(_)) => ReplaySource::Ticks,
            (Some(_), None) => ReplaySource::Minutes,
            (None, _) => ReplaySource::Own,
        };
        self.generation = input.generation;
        ReplayChange {
            old_len,
            new_len: out.len(),
            first_changed: cut,
        }
    }
}

/// The index of the first server bar of the group that holds bar `at`.
fn group_start(base: &[Bar], at: usize, timeframe: Timeframe) -> usize {
    let Some(key) = base.get(at).map(|b| timeframe.group_key(b.time_ms)) else {
        return at;
    };
    let mut first = at;
    while first > 0
        && base
            .get(first - 1)
            .is_some_and(|b| timeframe.group_key(b.time_ms) == key)
    {
        first -= 1;
    }
    first
}

/// Where the candle in progress ends. A day, week or month opens when the broker says so, so the
/// next bar tells; for the others the length is exact.
fn candle_end(period: Period, open_ms: i64, next_open_ms: Option<i64>) -> i64 {
    let nominal = open_ms + period.millis();
    match period {
        Period::D1 => next_open_ms.map_or(nominal, |n| n.min(nominal + DAY_SLACK_MS)),
        Period::W1 | Period::MN1 => next_open_ms.filter(|n| *n > open_ms).unwrap_or(nominal),
        _ => nominal,
    }
}

/// How many base bars are final, and the candle in progress when the last one is not.
fn split_last(
    input: &ReplayInput<'_>,
    period: Period,
    count: usize,
    cursor_ms: i64,
) -> (usize, Option<Bar>) {
    let Some(last) = count.checked_sub(1).and_then(|i| input.base.get(i)) else {
        return (0, None);
    };
    let end = candle_end(
        period,
        last.time_ms,
        input.base.get(count).map(|b| b.time_ms),
    );
    if cursor_ms >= end {
        return (count, None);
    }
    let forming = match (input.minutes, input.ticks) {
        (Some(minutes), Some(ticks)) => replay_fold_hybrid(minutes, ticks, last.time_ms, cursor_ms),
        (Some(minutes), None) => replay_fold_minutes(minutes, last.time_ms, cursor_ms),
        (None, _) => replay_partial_bar(*last, cursor_ms - last.time_ms, end - last.time_ms),
    };
    (count - 1, forming)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::Tick;

    const MINUTE: i64 = 60_000;

    fn minute_bars(count: i64) -> Vec<Bar> {
        (0..count)
            .map(|i| {
                let open = 1_000 + i * 2;
                Bar {
                    time_ms: i * MINUTE,
                    open,
                    high: open + 5,
                    low: open - 3,
                    close: open + 2,
                    volume: 10,
                }
            })
            .collect()
    }

    fn five_minute_bars(minutes: &[Bar]) -> Vec<Bar> {
        group_bars(minutes, Timeframe::Multiple(Period::M1, 5))
    }

    fn reveal(
        feed: &mut ReplayFeed,
        out: &mut Vec<Bar>,
        timeframe: Timeframe,
        base: &[Bar],
        minutes: Option<&[Bar]>,
        cursor_ms: i64,
    ) -> ReplayChange {
        let input = ReplayInput {
            timeframe,
            base,
            minutes,
            ticks: None,
            generation: 0,
        };
        feed.reveal_into(out, &input, cursor_ms)
    }

    fn fresh(
        timeframe: Timeframe,
        base: &[Bar],
        minutes: Option<&[Bar]>,
        cursor_ms: i64,
    ) -> Vec<Bar> {
        let mut out = Vec::new();
        reveal(
            &mut ReplayFeed::new(),
            &mut out,
            timeframe,
            base,
            minutes,
            cursor_ms,
        );
        out
    }

    #[test]
    fn a_candle_opening_after_the_cursor_is_not_shown() {
        let m1 = minute_bars(30);
        let shown = fresh(Timeframe::Bars(Period::M1), &m1, Some(&m1), 10 * MINUTE);
        assert_eq!(shown.len(), 10);
        assert!(shown.iter().all(|b| b.time_ms < 10 * MINUTE));
    }

    #[test]
    fn a_candle_opening_exactly_at_the_cursor_is_not_shown() {
        let m1 = minute_bars(30);
        let shown = fresh(Timeframe::Bars(Period::M1), &m1, Some(&m1), 10 * MINUTE);
        assert_eq!(shown.last().map(|b| b.time_ms), Some(9 * MINUTE));
    }

    #[test]
    fn a_higher_timeframe_candle_grows_as_minutes_are_revealed() {
        let m1 = minute_bars(30);
        let m5 = five_minute_bars(&m1);
        let tf = Timeframe::Bars(Period::M5);
        let early = fresh(tf, &m5, Some(&m1), 7 * MINUTE);
        let later = fresh(tf, &m5, Some(&m1), 9 * MINUTE);
        let (a, b) = (
            early.last().expect("a candle"),
            later.last().expect("a candle"),
        );
        assert_eq!((a.time_ms, b.time_ms), (5 * MINUTE, 5 * MINUTE));
        assert_eq!(a.close, m1.get(6).expect("minute").close);
        assert_eq!(b.close, m1.get(8).expect("minute").close);
        assert!(b.high >= a.high && b.low <= a.low && b.volume > a.volume);
    }

    #[test]
    fn a_candle_is_the_server_bar_once_the_cursor_passes_its_end() {
        let m1 = minute_bars(30);
        let m5 = five_minute_bars(&m1);
        let shown = fresh(Timeframe::Bars(Period::M5), &m5, Some(&m1), 10 * MINUTE);
        assert_eq!(shown.as_slice(), m5.get(..2).expect("two bars"));
    }

    #[test]
    fn without_minutes_a_candle_forms_along_its_own_path() {
        let m1 = minute_bars(30);
        let m5 = five_minute_bars(&m1);
        let shown = fresh(Timeframe::Bars(Period::M5), &m5, None, 7 * MINUTE + 30_000);
        let last = shown.last().expect("a candle");
        let whole = m5.get(1).expect("bar");
        assert_eq!((last.time_ms, last.open), (whole.time_ms, whole.open));
        assert!(last.high <= whole.high && last.low >= whole.low);
        assert!(last.close != whole.close || last.high != whole.high);
    }

    #[test]
    fn a_grouped_timeframe_forms_from_closed_base_bars_and_the_forming_one() {
        let m1 = minute_bars(40);
        let tf = Timeframe::Multiple(Period::M5, 3);
        let m5 = five_minute_bars(&m1);
        let shown = fresh(tf, &m5, Some(&m1), 17 * MINUTE);
        let last = shown.last().expect("a candle");
        assert_eq!(last.time_ms, 15 * MINUTE, "fifteen-minute buckets");
        assert_eq!(last.close, m1.get(16).expect("minute").close);
    }

    #[test]
    fn a_spike_after_the_cursor_never_appears() {
        let mut m1 = minute_bars(30);
        if let Some(spike) = m1.get_mut(20) {
            spike.high = 999_999;
        }
        let m5 = five_minute_bars(&m1);
        for cursor in (0..=20).map(|m| m * MINUTE) {
            let shown = fresh(Timeframe::Bars(Period::M5), &m5, Some(&m1), cursor);
            assert!(shown.iter().all(|b| b.high < 999_999), "cursor {cursor}");
        }
    }

    #[test]
    fn a_weekend_gap_shows_no_candle_and_leaves_the_last_one_final() {
        let mut m1 = minute_bars(10);
        m1.extend(minute_bars(10).into_iter().map(|b| Bar {
            time_ms: b.time_ms + 5_000 * MINUTE,
            ..b
        }));
        let m5 = five_minute_bars(&m1);
        let shown = fresh(Timeframe::Bars(Period::M5), &m5, Some(&m1), 2_000 * MINUTE);
        assert_eq!(shown.len(), 2);
        assert_eq!(shown.as_slice(), m5.get(..2).expect("two bars"));
    }

    #[test]
    fn a_daily_candle_ends_at_the_next_open_on_a_short_day() {
        let day = 24 * 3_600_000;
        let bars = [
            Bar {
                time_ms: 0,
                open: 1,
                high: 2,
                low: 1,
                close: 2,
                volume: 1,
            },
            Bar {
                time_ms: day - 3_600_000,
                open: 2,
                high: 3,
                low: 2,
                close: 3,
                volume: 1,
            },
        ];
        let shown = fresh(Timeframe::Bars(Period::D1), &bars, None, day - 3_600_000);
        assert_eq!(shown.as_slice(), bars.get(..1).expect("one bar"));
    }

    #[test]
    fn a_ticks_chart_shows_nothing() {
        let m1 = minute_bars(5);
        assert_eq!(fresh(Timeframe::Ticks, &m1, None, 3 * MINUTE).len(), 0);
    }

    /// Three ticks in each minute: its open, its low or high, and its close.
    fn ticks_of(minutes: &[Bar]) -> Vec<Tick> {
        minutes
            .iter()
            .flat_map(|m| {
                let extreme = if m.close >= m.open { m.low } else { m.high };
                [
                    Tick {
                        time_ms: m.time_ms + 10_000,
                        price: m.open,
                    },
                    Tick {
                        time_ms: m.time_ms + 30_000,
                        price: extreme,
                    },
                    Tick {
                        time_ms: m.time_ms + 50_000,
                        price: m.close,
                    },
                ]
            })
            .collect()
    }

    #[test]
    fn a_candle_in_progress_is_made_of_real_ticks_and_never_reads_its_minute_bar() {
        let mut m1 = minute_bars(30);
        let ticks = ticks_of(&m1);
        let m5 = five_minute_bars(&m1);
        if let Some(minute) = m1.get_mut(7) {
            minute.high = 999_999;
            minute.low = -999_999;
        }
        let input = ReplayInput {
            timeframe: Timeframe::Bars(Period::M5),
            base: &m5,
            minutes: Some(&m1),
            ticks: Some(&ticks),
            generation: 0,
        };
        let mut feed = ReplayFeed::new();
        let mut out = Vec::new();
        feed.reveal_into(&mut out, &input, 7 * MINUTE + 30_000);
        let candle = out.last().expect("a candle");
        assert!(
            candle.high < 999_999 && candle.low > -999_999,
            "the poisoned minute is not read"
        );
        assert_eq!(feed.source(), ReplaySource::Ticks);
        let minutes_only = ReplayInput {
            ticks: None,
            ..input
        };
        feed.reveal_into(&mut out, &minutes_only, 7 * MINUTE + 30_000);
        assert_eq!(feed.source(), ReplaySource::Minutes);
        feed.reveal_into(
            &mut out,
            &ReplayInput {
                minutes: None,
                ..input
            },
            7 * MINUTE + 30_000,
        );
        assert_eq!(feed.source(), ReplaySource::Own);
    }

    #[test]
    fn any_sequence_of_cursor_moves_equals_a_rebuild_from_scratch() {
        let m1 = minute_bars(400);
        let m5 = five_minute_bars(&m1);
        let ticks = ticks_of(&m1);
        let input = |timeframe, base, minutes| ReplayInput {
            timeframe,
            base,
            minutes,
            ticks: None,
            generation: 0,
        };
        let with_ticks = |timeframe, base| ReplayInput {
            ticks: Some(&ticks[..]),
            ..input(timeframe, base, Some(&m1[..]))
        };
        let cases = [
            input(Timeframe::Bars(Period::M1), &m1, Some(&m1[..])),
            input(Timeframe::Bars(Period::M5), &m5, Some(&m1[..])),
            input(Timeframe::Multiple(Period::M5, 3), &m5, Some(&m1[..])),
            input(Timeframe::Bars(Period::M5), &m5, None),
            input(Timeframe::Multiple(Period::M1, 15), &m1, Some(&m1[..])),
            with_ticks(Timeframe::Bars(Period::M1), &m1),
            with_ticks(Timeframe::Bars(Period::M5), &m5),
            with_ticks(Timeframe::Multiple(Period::M5, 3), &m5),
        ];
        let mut seed = 0x1234_5678_u64;
        for case in cases {
            let mut feed = ReplayFeed::new();
            let mut out = Vec::new();
            let mut cursor = 50 * MINUTE;
            for _ in 0..300 {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let roll = i64::try_from(seed >> 33).expect("fits") % 100;
                cursor = match roll {
                    0..=59 => cursor + 15_000 * (1 + roll % 7),
                    60..=89 => (cursor - 15_000 * (1 + roll % 9)).max(0),
                    _ => (roll - 90) * 40 * MINUTE,
                };
                cursor = cursor.clamp(0, 420 * MINUTE);
                feed.reveal_into(&mut out, &case, cursor);
                let mut expected = Vec::new();
                ReplayFeed::new().reveal_into(&mut expected, &case, cursor);
                assert_eq!(out, expected, "{:?} at {cursor}", case.timeframe);
            }
        }
    }

    #[test]
    fn older_history_put_in_front_rebuilds_the_whole_series() {
        let m1 = minute_bars(30);
        let tail = m1.get(10..).expect("tail").to_vec();
        let mut feed = ReplayFeed::new();
        let mut out = Vec::new();
        let tf = Timeframe::Bars(Period::M1);
        let mut input = ReplayInput {
            timeframe: tf,
            base: &tail,
            minutes: None,
            ticks: None,
            generation: 0,
        };
        feed.reveal_into(&mut out, &input, 20 * MINUTE);
        input = ReplayInput {
            base: &m1,
            generation: 1,
            ..input
        };
        feed.reveal_into(&mut out, &input, 20 * MINUTE);
        assert_eq!(out.len(), 20);
        assert_eq!(out.first().map(|b| b.time_ms), Some(0));
    }

    #[test]
    fn a_move_forward_rewrites_only_the_tail() {
        let m1 = minute_bars(100);
        let mut feed = ReplayFeed::new();
        let mut out = Vec::new();
        let tf = Timeframe::Bars(Period::M1);
        reveal(&mut feed, &mut out, tf, &m1, Some(&m1), 50 * MINUTE);
        let change = reveal(&mut feed, &mut out, tf, &m1, Some(&m1), 53 * MINUTE);
        assert_eq!((change.old_len, change.new_len), (50, 53));
        assert_eq!(change.first_changed, 49);
    }
}

//! Charts made of ticks at the replay cursor: a tick chart is the ticks seen so far, a seconds chart
//! the bars those ticks make.
//!
//! A tick at time `t` is seen when `t` is before the cursor. Ticks of the same millisecond are
//! seen together, since the cursor is a whole millisecond.

use wyck_market::{Bar, Tick};

use super::feed::ReplayChange;
use crate::data::{aggregate_ticks, fold_tick, trim_front};

/// What a tick chart remembers between two moves of the cursor.
#[derive(Debug, Clone, Default)]
pub struct ReplayTickFeed {
    start: usize,
    end: usize,
    generation: u64,
}

impl ReplayTickFeed {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets what was written: the next reveal rebuilds the whole series.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Writes into `out` the last `cap` ticks of `ticks` (sorted) before the cursor. Moving
    /// forward appends and moving back cuts the end; anything else, or a change of `generation`
    /// (ticks put in front or dropped from it), rebuilds.
    pub fn reveal_into(
        &mut self,
        out: &mut Vec<Tick>,
        ticks: &[Tick],
        generation: u64,
        cursor_ms: i64,
        cap: usize,
    ) -> ReplayChange {
        let old_len = out.len();
        let end = ticks.partition_point(|t| t.time_ms < cursor_ms);
        let start = end.saturating_sub(cap.max(1));
        let same = self.generation == generation && self.start == start && self.end <= ticks.len();
        let first_changed = if same && end >= self.end {
            out.extend_from_slice(ticks.get(self.end..end).unwrap_or_default());
            old_len
        } else if same {
            out.truncate(end - start);
            end - start
        } else {
            out.clear();
            out.extend_from_slice(ticks.get(start..end).unwrap_or_default());
            0
        };
        (self.start, self.end, self.generation) = (start, end, generation);
        ReplayChange {
            old_len,
            new_len: out.len(),
            first_changed,
        }
    }
}

/// What a seconds chart remembers between two moves of the cursor.
#[derive(Debug, Clone, Default)]
pub struct ReplaySecondsFeed {
    end: usize,
    generation: u64,
    bucket_ms: i64,
}

impl ReplaySecondsFeed {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets what was written: the next reveal rebuilds the whole series.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Writes into `out` the bars of `bucket_ms` that the ticks before the cursor make (the newest
    /// is the one in progress; a bucket without ticks has no bar), keeping the last `cap_bars`.
    /// Moving forward folds the new ticks in; moving back, or a change of `generation`, rebuilds.
    pub fn reveal_into(
        &mut self,
        out: &mut Vec<Bar>,
        ticks: &[Tick],
        generation: u64,
        bucket_ms: i64,
        cursor_ms: i64,
        cap_bars: usize,
    ) -> ReplayChange {
        let old_len = out.len();
        let bucket_ms = bucket_ms.max(1);
        let end = ticks.partition_point(|t| t.time_ms < cursor_ms);
        let same = self.generation == generation
            && self.bucket_ms == bucket_ms
            && self.end <= end
            && self.end <= ticks.len();
        let mut first_changed = if same {
            for tick in ticks.get(self.end..end).unwrap_or_default() {
                fold_tick(out, bucket_ms, *tick);
            }
            old_len.saturating_sub(1)
        } else {
            *out = aggregate_ticks(ticks.get(..end).unwrap_or_default(), bucket_ms);
            0
        };
        if trim_front(out, cap_bars.max(1)) > 0 {
            first_changed = 0;
        }
        (self.end, self.generation, self.bucket_ms) = (end, generation, bucket_ms);
        ReplayChange {
            old_len,
            new_len: out.len(),
            first_changed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(time_ms: i64, price: i64) -> Tick {
        Tick { time_ms, price }
    }

    fn ticks(count: i64) -> Vec<Tick> {
        (0..count)
            .map(|i| tick(i * 400 + (i % 3), 1_000 + (i * 7) % 50))
            .collect()
    }

    fn walk(seed: &mut u64, cursor: i64, limit: i64) -> i64 {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let roll = i64::try_from(*seed >> 33).expect("fits") % 100;
        let next = match roll {
            0..=59 => cursor + 300 * (1 + roll % 9),
            60..=89 => cursor - 450 * (1 + roll % 7),
            _ => (roll - 90) * limit / 10,
        };
        next.clamp(0, limit)
    }

    #[test]
    fn a_tick_chart_shows_the_ticks_before_the_cursor() {
        let all = ticks(100);
        let mut out = Vec::new();
        ReplayTickFeed::new().reveal_into(&mut out, &all, 0, 4_000, 1_000);
        assert!(out.iter().all(|t| t.time_ms < 4_000));
        assert_eq!(out.len(), all.iter().filter(|t| t.time_ms < 4_000).count());
    }

    #[test]
    fn a_tick_exactly_at_the_cursor_is_not_seen() {
        let all = [tick(0, 1), tick(100, 2), tick(200, 3)];
        let mut out = Vec::new();
        ReplayTickFeed::new().reveal_into(&mut out, &all, 0, 100, 10);
        assert_eq!(out, vec![tick(0, 1)]);
    }

    #[test]
    fn a_tick_chart_keeps_the_last_ticks_up_to_its_cap() {
        let all = ticks(100);
        let mut out = Vec::new();
        ReplayTickFeed::new().reveal_into(&mut out, &all, 0, 40_000, 10);
        assert_eq!(out.len(), 10);
        assert_eq!(out.last(), all.get(99));
    }

    #[test]
    fn any_sequence_of_cursor_moves_gives_the_same_ticks_as_a_rebuild() {
        let all = ticks(500);
        let limit = 500 * 400;
        for cap in [7, 120, 10_000] {
            let (mut feed, mut out, mut cursor, mut seed) =
                (ReplayTickFeed::new(), Vec::new(), 3_000_i64, 99_u64);
            for _ in 0..400 {
                cursor = walk(&mut seed, cursor, limit);
                feed.reveal_into(&mut out, &all, 0, cursor, cap);
                let mut fresh = Vec::new();
                ReplayTickFeed::new().reveal_into(&mut fresh, &all, 0, cursor, cap);
                assert_eq!(out, fresh, "cap {cap} at {cursor}");
            }
        }
    }

    #[test]
    fn a_move_forward_appends_only_the_new_ticks() {
        let all = ticks(100);
        let (mut feed, mut out) = (ReplayTickFeed::new(), Vec::new());
        feed.reveal_into(&mut out, &all, 0, 4_000, 1_000);
        let change = feed.reveal_into(&mut out, &all, 0, 6_000, 1_000);
        assert_eq!(change.first_changed, change.old_len);
        assert!(change.new_len > change.old_len);
    }

    #[test]
    fn ticks_put_in_front_rebuild_the_chart() {
        let all = ticks(100);
        let tail = all.get(50..).expect("tail").to_vec();
        let (mut feed, mut out) = (ReplayTickFeed::new(), Vec::new());
        feed.reveal_into(&mut out, &tail, 0, 40_000, 1_000);
        feed.reveal_into(&mut out, &all, 1, 40_000, 1_000);
        assert_eq!(out.len(), 100);
        assert_eq!(out.first(), all.first());
    }

    #[test]
    fn a_seconds_chart_is_the_bars_the_ticks_before_the_cursor_make() {
        let all = ticks(100);
        let mut out = Vec::new();
        ReplaySecondsFeed::new().reveal_into(&mut out, &all, 0, 1_000, 5_300, 1_000);
        let visible: Vec<Tick> = all.iter().copied().filter(|t| t.time_ms < 5_300).collect();
        assert_eq!(out, aggregate_ticks(&visible, 1_000));
        assert_eq!(
            out.last().map(|b| b.time_ms),
            Some(5_000),
            "the bar in progress"
        );
    }

    #[test]
    fn a_seconds_chart_has_no_bar_for_a_second_without_ticks() {
        let all = [tick(0, 1), tick(5_000, 2)];
        let mut out = Vec::new();
        ReplaySecondsFeed::new().reveal_into(&mut out, &all, 0, 1_000, 6_000, 100);
        assert_eq!(
            out.iter().map(|b| b.time_ms).collect::<Vec<_>>(),
            vec![0, 5_000]
        );
    }

    #[test]
    fn any_sequence_of_cursor_moves_gives_the_same_seconds_as_a_rebuild() {
        let all = ticks(500);
        let limit = 500 * 400;
        for (bucket, cap) in [(1_000, 10_000), (5_000, 4), (15_000, 10_000)] {
            let (mut feed, mut out, mut cursor, mut seed) =
                (ReplaySecondsFeed::new(), Vec::new(), 3_000_i64, 7_u64);
            for _ in 0..400 {
                cursor = walk(&mut seed, cursor, limit);
                feed.reveal_into(&mut out, &all, 0, bucket, cursor, cap);
                let mut fresh = Vec::new();
                ReplaySecondsFeed::new().reveal_into(&mut fresh, &all, 0, bucket, cursor, cap);
                assert_eq!(out, fresh, "bucket {bucket} cap {cap} at {cursor}");
            }
        }
    }

    #[test]
    fn a_tick_after_the_cursor_never_shows_in_a_seconds_bar() {
        let all = [tick(0, 100), tick(500, 999_999)];
        let mut out = Vec::new();
        ReplaySecondsFeed::new().reveal_into(&mut out, &all, 0, 1_000, 400, 100);
        assert_eq!(out.last().map(|b| b.high), Some(100));
    }
}

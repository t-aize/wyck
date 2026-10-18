//! The bars of one symbol and period that a replay holds, and the range of time they cover.

use wyck_chart::{MAX_BARS, ReplayTimeline, merge_bars, prepend_bars};
use wyck_market::Bar;

use super::plan::{Coverage, EMPTY_HISTORY_MS};
use crate::market_data::replay_load::Chunk;

/// Bars of one symbol and period, sorted, past the cursor included.
#[derive(Debug)]
pub struct Store {
    bars: Vec<Bar>,
    generation: u64,
    coverage: Coverage,
    empty_ms: i64,
}

impl Store {
    pub(crate) fn new(cursor_ms: i64) -> Self {
        Self {
            bars: Vec::new(),
            generation: 0,
            coverage: Coverage::at(cursor_ms),
            empty_ms: 0,
        }
    }

    #[must_use]
    pub fn bars(&self) -> &[Bar] {
        &self.bars
    }

    /// Changes whenever bars were put in front of the others or dropped from the front, which
    /// moves their indices. Bars added at the end do not change it.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn coverage(&self) -> Coverage {
        self.coverage
    }

    /// The end of the time asked for, inclusive.
    pub(crate) fn covered_to_ms(&self) -> i64 {
        self.coverage.to_ms
    }

    /// The oldest time the replay can go back to.
    pub(crate) fn oldest_ms(&self) -> i64 {
        self.bars
            .first()
            .map_or(self.coverage.from_ms, |bar| bar.time_ms)
    }

    pub(crate) fn apply_before(&mut self, chunk: Chunk) {
        if chunk.bars.is_empty() {
            self.empty_ms += chunk.to_ms - chunk.from_ms + 1;
        } else {
            self.empty_ms = 0;
            prepend_bars(&mut self.bars, chunk.bars);
            self.generation += 1;
        }
        self.coverage.from_ms = self.coverage.from_ms.min(chunk.from_ms);
        if self.empty_ms >= EMPTY_HISTORY_MS || chunk.from_ms <= 0 {
            self.coverage.exhausted_before = true;
        }
    }

    pub(crate) fn apply_forward(&mut self, chunk: &Chunk) {
        merge_bars(&mut self.bars, &chunk.bars);
        self.coverage.to_ms = self.coverage.to_ms.max(chunk.to_ms);
    }

    /// Drops the bars older than `keep_from_ms` once there are more than the chart could show,
    /// so a long session does not grow without end.
    pub(crate) fn trim(&mut self, keep_from_ms: i64) {
        if self.bars.len() <= MAX_BARS + MAX_BARS / 4 {
            return;
        }
        let drop = self.bars.partition_point(|b| b.time_ms < keep_from_ms);
        if drop == 0 {
            return;
        }
        self.bars.drain(..drop);
        self.coverage.from_ms = self.bars.first().map_or(keep_from_ms, |b| b.time_ms);
        self.coverage.exhausted_before = false;
        self.empty_ms = 0;
        self.generation += 1;
    }
}

/// The times at which any of the stores has a bar, for the clock to jump over empty hours.
pub(crate) struct DataTimes<'a>(pub(crate) Vec<&'a [Bar]>);

impl ReplayTimeline for DataTimes<'_> {
    fn next_data(&self, after_ms: i64) -> Option<i64> {
        self.0
            .iter()
            .filter_map(|bars| bars.get(bars.partition_point(|b| b.time_ms <= after_ms)))
            .map(|b| b.time_ms)
            .min()
    }

    fn prev_data(&self, before_ms: i64) -> Option<i64> {
        self.0
            .iter()
            .filter_map(|bars| {
                bars.partition_point(|b| b.time_ms < before_ms)
                    .checked_sub(1)
                    .and_then(|at| bars.get(at))
            })
            .map(|b| b.time_ms)
            .max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(time_ms: i64) -> Bar {
        Bar {
            time_ms,
            open: 1,
            high: 1,
            low: 1,
            close: 1,
            volume: 1,
        }
    }

    fn chunk(from_ms: i64, to_ms: i64, times: &[i64]) -> Chunk {
        Chunk {
            bars: times.iter().map(|t| bar(*t)).collect(),
            from_ms,
            to_ms,
        }
    }

    #[test]
    fn older_bars_go_in_front_and_change_the_generation() {
        let mut store = Store::new(1_000);
        store.apply_forward(&chunk(1_000, 1_999, &[1_000, 1_500]));
        let generation = store.generation();
        store.apply_before(chunk(0, 999, &[100, 500]));
        assert_eq!(store.bars().len(), 4);
        assert_eq!(store.bars().first().map(|b| b.time_ms), Some(100));
        assert_ne!(store.generation(), generation);
    }

    #[test]
    fn newer_bars_join_at_the_end_without_changing_the_generation() {
        let mut store = Store::new(1_000);
        store.apply_forward(&chunk(1_000, 1_999, &[1_000]));
        let generation = store.generation();
        store.apply_forward(&chunk(2_000, 2_999, &[2_000]));
        assert_eq!(store.generation(), generation);
        assert_eq!(store.covered_to_ms(), 2_999);
    }

    #[test]
    fn empty_chunks_covering_ten_days_mean_the_history_has_begun() {
        let day = 86_400_000;
        let mut store = Store::new(100 * day);
        for step in 1..=9 {
            store.apply_before(chunk((100 - step) * day, (101 - step) * day - 1, &[]));
            assert!(!store.coverage().exhausted_before, "after {step} days");
        }
        store.apply_before(chunk(90 * day, 91 * day - 1, &[]));
        assert!(store.coverage().exhausted_before);
    }

    #[test]
    fn a_bar_in_the_way_resets_the_count_of_empty_days() {
        let day = 86_400_000;
        let mut store = Store::new(100 * day);
        for step in 1..=9 {
            store.apply_before(chunk((100 - step) * day, (101 - step) * day - 1, &[]));
        }
        store.apply_before(chunk(80 * day, 90 * day - 1, &[85 * day]));
        assert!(!store.coverage().exhausted_before);
    }

    #[test]
    fn the_start_of_time_ends_the_history() {
        let mut store = Store::new(1_000);
        store.apply_before(chunk(0, 999, &[]));
        assert!(store.coverage().exhausted_before);
    }

    #[test]
    fn the_timeline_of_several_stores_is_the_union_of_their_times() {
        let (a, b) = (vec![bar(0), bar(10), bar(40)], vec![bar(5), bar(30)]);
        let times = DataTimes(vec![&a, &b]);
        assert_eq!(times.next_data(0), Some(5));
        assert_eq!(times.next_data(10), Some(30));
        assert_eq!(times.next_data(40), None);
        assert_eq!(times.prev_data(40), Some(30));
        assert_eq!(times.prev_data(0), None);
    }

    #[test]
    fn trimming_keeps_a_store_that_is_small_enough() {
        let mut store = Store::new(0);
        store.apply_forward(&chunk(0, 9, &[1, 2, 3]));
        let generation = store.generation();
        store.trim(3);
        assert_eq!(store.bars().len(), 3);
        assert_eq!(store.generation(), generation);
    }
}

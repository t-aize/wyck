//! The ticks of one symbol that a replay holds, the times of its ticks for the clock, and whether
//! the broker has ticks at the place the replay starts.

use wyck_chart::{ReplayTimeline, prepend_ticks};
use wyck_market::Tick;

use super::plan::Coverage;
use crate::market_data::replay_ticks::{FIRST_SPAN_MS, TickChunk, next_span_ms};

/// The most ticks kept for one symbol: 4.8 MB.
pub(crate) const MAX_STORED_TICKS: usize = 300_000;
/// How far past the start the probe looks for a tick before it says the broker has none.
const MAX_SEARCH_MS: i64 = 3 * 86_400_000;

/// Whether the broker has ticks where the replay starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TickSupport {
    /// Not looked at yet.
    #[default]
    Unknown,
    Available,
    /// The broker keeps no ticks that far back: candles are made of minute bars instead.
    Unavailable,
}

/// What a first chunk of ticks says about their availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Available,
    Unavailable,
    /// Nothing yet, and nothing to say there should be (a closed market): look further.
    KeepLooking,
}

/// Reads the answer of the first chunks of ticks. A tick settles it. An empty stretch where minute
/// bars exist is a broker that keeps no ticks there. An empty stretch with no minute bar either is
/// a closed market, which is looked past for a few days.
pub(crate) fn probe_verdict(
    chunk_has_ticks: bool,
    minutes_in_range: bool,
    searched_ms: i64,
) -> Verdict {
    if chunk_has_ticks {
        Verdict::Available
    } else if minutes_in_range || searched_ms >= MAX_SEARCH_MS {
        Verdict::Unavailable
    } else {
        Verdict::KeepLooking
    }
}

/// The ticks of one symbol around the cursor.
#[derive(Debug)]
pub struct TickStore {
    ticks: Vec<Tick>,
    generation: u64,
    coverage: Coverage,
    span_ms: i64,
    /// Where the store began: how far past it the ticks are looked for says how long a closed
    /// market has been looked past.
    origin_ms: i64,
}

impl TickStore {
    pub(crate) fn new(cursor_ms: i64) -> Self {
        Self {
            ticks: Vec::new(),
            generation: 0,
            coverage: Coverage::at(cursor_ms),
            span_ms: FIRST_SPAN_MS,
            origin_ms: cursor_ms,
        }
    }

    #[must_use]
    pub fn ticks(&self) -> &[Tick] {
        &self.ticks
    }

    /// Changes whenever ticks were put in front or dropped from the front. Ticks added at the end
    /// do not change it.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn coverage(&self) -> Coverage {
        self.coverage
    }

    /// How far past where it began the ticks have been asked for.
    pub(crate) fn searched_ms(&self) -> i64 {
        self.coverage.to_ms - self.origin_ms + 1
    }

    /// The length of the next chunk to ask for.
    pub(crate) fn span_ms(&self) -> i64 {
        self.span_ms
    }

    /// Whether the ticks held reach from before the minute the cursor is in up to the cursor, which
    /// is what the candle in progress needs to be made of them.
    pub(crate) fn covers(&self, cursor_ms: i64) -> bool {
        let minute = cursor_ms - cursor_ms.rem_euclid(60_000);
        self.coverage.from_ms <= minute && self.coverage.to_ms >= cursor_ms - 1
    }

    pub(crate) fn apply_before(&mut self, chunk: TickChunk) {
        self.span_ms = next_span_ms(self.span_ms, chunk.ticks.len());
        if prepend_ticks(&mut self.ticks, chunk.ticks) > 0 {
            self.generation += 1;
        }
        self.coverage.from_ms = self.coverage.from_ms.min(chunk.from_ms);
        if chunk.from_ms <= 0 {
            self.coverage.exhausted_before = true;
        }
    }

    pub(crate) fn apply_forward(&mut self, chunk: TickChunk) {
        self.span_ms = next_span_ms(self.span_ms, chunk.ticks.len());
        let last = self.ticks.last().map_or(i64::MIN, |t| t.time_ms);
        self.ticks
            .extend(chunk.ticks.into_iter().filter(|t| t.time_ms > last));
        self.coverage.to_ms = self.coverage.to_ms.max(chunk.to_ms);
    }

    /// Drops ticks that are no longer needed once there are more than the cap: first those older
    /// than `keep_from_ms`, then the oldest of what is left.
    pub(crate) fn trim(&mut self, keep_from_ms: i64) {
        if self.ticks.len() <= MAX_STORED_TICKS {
            return;
        }
        let old = self.ticks.partition_point(|t| t.time_ms < keep_from_ms);
        let drop = old.max(self.ticks.len() - MAX_STORED_TICKS);
        self.ticks.drain(..drop);
        self.coverage.from_ms = self
            .ticks
            .first()
            .map_or(keep_from_ms, |t| t.time_ms.max(self.coverage.from_ms));
        self.coverage.exhausted_before = false;
        self.generation += 1;
    }
}

/// The times of the ticks of several symbols, for a step in ticks and for jumping over the hours
/// with none.
pub(crate) struct TickTimes<'a>(pub(crate) Vec<&'a [Tick]>);

impl ReplayTimeline for TickTimes<'_> {
    fn next_data(&self, after_ms: i64) -> Option<i64> {
        self.0
            .iter()
            .filter_map(|ticks| ticks.get(ticks.partition_point(|t| t.time_ms <= after_ms)))
            .map(|t| t.time_ms)
            .min()
    }

    fn prev_data(&self, before_ms: i64) -> Option<i64> {
        self.0
            .iter()
            .filter_map(|ticks| {
                ticks
                    .partition_point(|t| t.time_ms < before_ms)
                    .checked_sub(1)
                    .and_then(|at| ticks.get(at))
            })
            .map(|t| t.time_ms)
            .max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(time_ms: i64) -> Tick {
        Tick { time_ms, price: 1 }
    }

    fn chunk(from_ms: i64, to_ms: i64, times: &[i64]) -> TickChunk {
        TickChunk {
            ticks: times.iter().map(|t| tick(*t)).collect(),
            from_ms,
            to_ms,
        }
    }

    #[test]
    fn a_tick_settles_the_probe_and_so_does_a_missing_one_next_to_minute_bars() {
        assert_eq!(probe_verdict(true, false, 0), Verdict::Available);
        assert_eq!(probe_verdict(false, true, 0), Verdict::Unavailable);
    }

    #[test]
    fn a_closed_market_is_looked_past_for_a_few_days_and_then_given_up() {
        assert_eq!(probe_verdict(false, false, 0), Verdict::KeepLooking);
        assert_eq!(
            probe_verdict(false, false, 2 * 86_400_000),
            Verdict::KeepLooking
        );
        assert_eq!(
            probe_verdict(false, false, 3 * 86_400_000),
            Verdict::Unavailable
        );
    }

    #[test]
    fn older_ticks_go_in_front_and_change_the_generation() {
        let mut store = TickStore::new(1_000);
        store.apply_forward(chunk(1_000, 1_999, &[1_000, 1_500]));
        let generation = store.generation();
        store.apply_before(chunk(0, 999, &[100, 500]));
        assert_eq!(store.ticks().len(), 4);
        assert_eq!(store.ticks().first().map(|t| t.time_ms), Some(100));
        assert_ne!(store.generation(), generation);
        assert!(store.coverage().exhausted_before, "the start of time");
    }

    #[test]
    fn newer_ticks_join_the_end_without_changing_the_generation_or_doubling_a_tick() {
        let mut store = TickStore::new(0);
        store.apply_forward(chunk(0, 999, &[100, 900]));
        let generation = store.generation();
        store.apply_forward(chunk(1_000, 1_999, &[900, 1_100]));
        assert_eq!(store.generation(), generation);
        assert_eq!(store.ticks().len(), 3, "the tick at 900 is not held twice");
        assert_eq!(store.coverage().to_ms, 1_999);
    }

    #[test]
    fn the_length_of_the_next_chunk_follows_what_the_last_one_held() {
        let mut store = TickStore::new(0);
        let first = store.span_ms();
        store.apply_forward(chunk(0, 999, &[]));
        assert!(store.span_ms() > first, "an empty chunk asks for longer");
    }

    #[test]
    fn the_candle_in_progress_needs_ticks_from_its_minute_up_to_the_cursor() {
        let mut store = TickStore::new(120_000);
        assert!(!store.covers(150_000));
        store.apply_forward(chunk(120_000, 179_999, &[121_000]));
        assert!(store.covers(150_000));
        assert!(!store.covers(250_000), "the ticks stop before the cursor");
        store.apply_before(chunk(60_000, 119_999, &[]));
        assert!(store.covers(130_000));
    }

    #[test]
    fn trimming_keeps_a_small_store_and_cuts_a_big_one_from_the_front() {
        let mut store = TickStore::new(0);
        store.apply_forward(chunk(0, 9, &[1, 2, 3]));
        let generation = store.generation();
        store.trim(3);
        assert_eq!(store.ticks().len(), 3);
        assert_eq!(store.generation(), generation);
        let times: Vec<i64> = (0..i64::try_from(MAX_STORED_TICKS).expect("fits") + 10).collect();
        let mut big = TickStore::new(0);
        big.apply_forward(chunk(0, 1_000_000, &times));
        big.trim(5);
        assert_eq!(big.ticks().len(), MAX_STORED_TICKS);
        assert_ne!(big.generation(), 0);
        assert_eq!(
            big.coverage().from_ms,
            10,
            "it starts where the ticks held do"
        );
    }

    #[test]
    fn the_times_of_several_symbols_are_the_union_of_their_ticks() {
        let (a, b) = (vec![tick(0), tick(10), tick(40)], vec![tick(5), tick(30)]);
        let times = TickTimes(vec![&a, &b]);
        assert_eq!(times.next_data(0), Some(5));
        assert_eq!(times.next_data(40), None);
        assert_eq!(times.prev_data(40), Some(30));
        assert_eq!(times.nth_data_after(0, 3), Some(30), "5, 10, 30");
        assert_eq!(times.nth_data_before(41, 2), Some(30), "40, 30");
    }
}

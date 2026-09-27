//! Turns a Replay session's cursor into the same [`LiveUpdate`]s the real live feed
//! produces, so [`Chart::on_live`](super::Chart::on_live) needs no "replay mode" branch
//! of its own: the chart is structurally unable to see a bar past the cursor, because it
//! is never handed to it. [`ReplayFeed`] only ever reveals bars it already holds; it does
//! not fetch, which is what keeps it a pure, synchronous, easily tested step.

use wyck_openapi::market::{Bar, Period};

use super::live::LiveUpdate;

/// Reveals bars from a fixed, already-loaded set one at a time as the replay cursor
/// crosses them. Tick-by-tick precision (revealing the ticks inside a bar rather than
/// the bar whole) is a later addition with the same shape, tracked as the
/// `tick_precision` setting.
pub struct ReplayFeed {
    symbol_id: i64,
    period: Period,
    bars: Vec<Bar>,
    revealed: usize,
}

impl ReplayFeed {
    /// `bars` is every bar of `period`, oldest first, from the replay's start point
    /// onward: exactly what [`super::history::Chart::start_replay`] holds back from the
    /// chart's already-loaded history.
    #[must_use]
    pub fn new(symbol_id: i64, period: Period, bars: Vec<Bar>) -> Self {
        Self {
            symbol_id,
            period,
            bars,
            revealed: 0,
        }
    }

    /// Every bar whose open time is at or before `cursor_ms` and not already revealed,
    /// oldest first, each as the [`LiveUpdate`] [`Chart::on_live`](super::Chart::on_live)
    /// expects. Empty once the cursor has caught up to what is held, until it advances
    /// past the next bar's time.
    pub fn reveal(&mut self, cursor_ms: i64) -> Vec<LiveUpdate> {
        let mut updates = Vec::new();
        while let Some(bar) = self.bars.get(self.revealed).copied() {
            if bar.time_ms > cursor_ms {
                break;
            }
            updates.push(LiveUpdate {
                symbol_id: self.symbol_id,
                bid: Some(bar.close),
                ask: Some(bar.close),
                timestamp: Some(bar.time_ms),
                bars: vec![(self.period, bar)],
            });
            self.revealed += 1;
        }
        updates
    }

    /// Whether every held bar has been revealed: the replay has caught up to the end of
    /// what was loaded when it started.
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.revealed >= self.bars.len()
    }

    /// The open time of the first held bar: where the replay's "future" begins.
    #[must_use]
    pub fn first_bar_time(&self) -> Option<i64> {
        self.bars.first().map(|bar| bar.time_ms)
    }

    /// The bar period every revealed [`LiveUpdate`] carries.
    #[must_use]
    pub fn period(&self) -> Period {
        self.period
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(time_ms: i64) -> Bar {
        Bar {
            time_ms,
            open: 1,
            high: 2,
            low: 0,
            close: 1,
            volume: 1,
        }
    }

    fn feed() -> ReplayFeed {
        ReplayFeed::new(7, Period::M1, vec![bar(0), bar(60_000), bar(120_000)])
    }

    #[test]
    fn nothing_is_revealed_before_the_first_bar() {
        let mut feed = feed();
        assert!(feed.reveal(-1).is_empty());
        assert!(!feed.is_exhausted());
    }

    #[test]
    fn reveal_returns_every_newly_crossed_bar_oldest_first() {
        let mut feed = feed();
        let updates = feed.reveal(60_000);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].timestamp, Some(0));
        assert_eq!(updates[1].timestamp, Some(60_000));
        assert_eq!(updates[0].bars, vec![(Period::M1, bar(0))]);
    }

    #[test]
    fn a_bar_is_never_revealed_twice() {
        let mut feed = feed();
        feed.reveal(60_000);
        let updates = feed.reveal(60_000);
        assert!(updates.is_empty());
    }

    #[test]
    fn revealing_past_the_end_exhausts_the_feed() {
        let mut feed = feed();
        let updates = feed.reveal(1_000_000);
        assert_eq!(updates.len(), 3);
        assert!(feed.is_exhausted());
    }

    #[test]
    fn first_bar_time_is_where_the_held_range_begins() {
        let feed = feed();
        assert_eq!(feed.first_bar_time(), Some(0));
    }
}

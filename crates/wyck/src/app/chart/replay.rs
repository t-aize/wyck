//! Turns a Replay session's cursor into the same [`LiveUpdate`]s the real live feed
//! produces, so [`Chart::on_live`](super::Chart::on_live) needs no "replay mode" branch
//! of its own: the chart is structurally unable to see a bar past the cursor, because it
//! is never handed to it. [`ReplayFeed`] only ever reveals bars it already holds; it does
//! not fetch, which is what keeps it a pure, synchronous, easily tested step.
//!
//! Moving forward (play, step forward) only ever needs [`ReplayFeed::reveal`]: the bars
//! are already held, nothing is thrown away. Moving to an earlier point is different, and
//! deliberately not supported by scrubbing: TradingView's own Bar Replay has no step-back
//! either, and MetaTrader's Visual Mode restarts from a newly chosen date rather than
//! rewinding a running one. This codebase does the same: "go to an earlier date" means
//! [`super::history::Chart::replay_seek`], which reloads the chart truncated to the new
//! point and builds a fresh [`ReplayFeed`] for what comes after it, rather than trying to
//! un-reveal bars already applied to the chart (which has no primitive for that).

use wyck_openapi::market::{Bar, Period};

use super::live::LiveUpdate;
use wyck_market_data::replay::ReplaySession;

/// An active Replay: the cursor/speed/play state, and the bars still to reveal.
pub struct ReplayState {
    pub session: ReplaySession,
    pub feed: ReplayFeed,
}

/// A read-only snapshot of a [`ReplayState`], for the control bar to render.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplayView {
    pub speed: f64,
    pub playing: bool,
    pub cursor_ms: i64,
    pub start_ms: i64,
    pub exhausted: bool,
}

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
    /// onward: what [`super::history::Chart::replay_seek`] loads for the range after the
    /// picked point.
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
}

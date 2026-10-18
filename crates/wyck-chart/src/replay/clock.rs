//! The replay transport: a cursor in time moved by play, step and seek.
//!
//! The clock never reads the wall time: the caller passes it in. Playback is anchored (a wall time
//! and a count of steps taken since) so a late frame catches up instead of drifting.
//!
//! A cursor `c` means "what happened strictly before `c` is known". A step that divides a day lands
//! on a grid counted from the Unix epoch, so a candle of any timeframe ends on a step; any other
//! step in time is relative (`c` plus the step), so a step back undoes a step forward exactly. A
//! step in ticks lands just after the n-th tick not yet seen.

use wyck_core::math::{f64_to_i64, i64_to_f64};

/// The updates per second the speed menu offers.
pub const REPLAY_RATES: [f64; 6] = [0.1, 0.3, 1.0, 3.0, 10.0, 30.0];

const DAY_MS: i64 = 86_400_000;
const MIN_RATE: f64 = 0.05;
const MAX_RATE: f64 = 60.0;
/// A frame that was very late takes this many steps at most: the rest is dropped.
const MAX_STEPS_PER_CALL: i64 = 1_000;

/// How the clock takes a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockStep {
    /// To the next multiple of this many milliseconds.
    Grid(i64),
    /// By this many milliseconds from where the cursor is.
    Relative(i64),
    /// Over this many ticks of the data.
    Ticks(u32),
}

impl ClockStep {
    /// A step in time: on the grid when it divides a day, else relative to the cursor.
    #[must_use]
    pub fn time(ms: i64) -> Self {
        let ms = ms.max(1);
        if DAY_MS % ms == 0 {
            Self::Grid(ms)
        } else {
            Self::Relative(ms)
        }
    }
}

/// The step of [`ReplayStep::Auto`](super::ReplayStep::Auto): the shortest bar length among the
/// charts, at least `floor_ms` and never under a second (a minute when the data is minute bars, a
/// second when there are ticks).
#[must_use]
pub fn replay_auto_step(chart_bar_ms: impl IntoIterator<Item = i64>, floor_ms: i64) -> i64 {
    let floor_ms = floor_ms.max(1_000);
    chart_bar_ms
        .into_iter()
        .filter(|ms| *ms > 0)
        .min()
        .map_or(floor_ms, |ms| ms.max(floor_ms))
}

/// When market data exists, so the cursor can jump over the hours when nothing traded.
pub trait ReplayTimeline {
    /// The first data time strictly after `after_ms`.
    fn next_data(&self, after_ms: i64) -> Option<i64>;
    /// The last data time strictly before `before_ms`.
    fn prev_data(&self, before_ms: i64) -> Option<i64>;

    /// The `count`-th data time strictly after `after_ms` (the first one for 1), if there are so
    /// many. Data at the same time count as one.
    fn nth_data_after(&self, after_ms: i64, count: u32) -> Option<i64> {
        let mut at = after_ms;
        for _ in 0..count {
            at = self.next_data(at)?;
        }
        Some(at)
    }

    /// The `count`-th data time strictly before `before_ms`, going back (the last one for 1).
    fn nth_data_before(&self, before_ms: i64, count: u32) -> Option<i64> {
        let mut at = before_ms;
        for _ in 0..count {
            at = self.prev_data(at)?;
        }
        Some(at)
    }
}

/// A timeline that never skips: every step is taken, with or without data.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReplayNoGaps;

impl ReplayTimeline for ReplayNoGaps {
    fn next_data(&self, _after_ms: i64) -> Option<i64> {
        None
    }

    fn prev_data(&self, _before_ms: i64) -> Option<i64> {
        None
    }
}

/// Sorted data times.
impl ReplayTimeline for [i64] {
    fn next_data(&self, after_ms: i64) -> Option<i64> {
        self.get(self.partition_point(|t| *t <= after_ms)).copied()
    }

    fn prev_data(&self, before_ms: i64) -> Option<i64> {
        self.partition_point(|t| *t < before_ms)
            .checked_sub(1)
            .and_then(|at| self.get(at))
            .copied()
    }
}

/// The range the cursor may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayBounds {
    /// The earliest cursor: the start of the data held.
    pub floor_ms: i64,
    /// The latest cursor: what is loaded and, at most, the present.
    pub cap_ms: i64,
}

/// What [`ReplayClock::advance`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReplayAdvance {
    /// How many steps were taken.
    pub steps: i64,
    /// Whether a step was due but the cap stopped it.
    pub hit_cap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplayClock {
    cursor_ms: i64,
    step: ClockStep,
    rate_hz: f64,
    playing: bool,
    anchor_wall_ms: i64,
    steps_since_anchor: i64,
}

impl ReplayClock {
    /// A paused clock at `cursor_ms`, one update per second.
    #[must_use]
    pub fn new(cursor_ms: i64, step: ClockStep) -> Self {
        Self {
            cursor_ms,
            step: normal(step),
            rate_hz: 1.0,
            playing: false,
            anchor_wall_ms: 0,
            steps_since_anchor: 0,
        }
    }

    #[must_use]
    pub fn cursor_ms(&self) -> i64 {
        self.cursor_ms
    }

    #[must_use]
    pub fn step(&self) -> ClockStep {
        self.step
    }

    #[must_use]
    pub fn rate_hz(&self) -> f64 {
        self.rate_hz
    }

    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Changes the step. The cursor stays where it is.
    pub fn set_step(&mut self, step: ClockStep) {
        self.step = normal(step);
    }

    /// Changes the updates per second (kept between 0.05 and 60). While playing, the cursor does
    /// not jump: the new rate counts from `wall_ms`.
    pub fn set_rate(&mut self, rate_hz: f64, wall_ms: i64) {
        self.rate_hz = if rate_hz.is_finite() {
            rate_hz.clamp(MIN_RATE, MAX_RATE)
        } else {
            1.0
        };
        self.reanchor(wall_ms);
    }

    /// Starts, or resumes, playing at `wall_ms`.
    pub fn play(&mut self, wall_ms: i64) {
        self.playing = true;
        self.reanchor(wall_ms);
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Puts the cursor at `cursor_ms` (inside the bounds) and pauses: going to a point does not
    /// resume playing on its own.
    pub fn seek(&mut self, cursor_ms: i64, bounds: ReplayBounds) {
        self.cursor_ms = cursor_ms.clamp(bounds.floor_ms, bounds.cap_ms.max(bounds.floor_ms));
        self.playing = false;
    }

    /// Takes the steps due at `wall_ms`. Does nothing while paused.
    pub fn advance<T: ReplayTimeline + ?Sized>(
        &mut self,
        wall_ms: i64,
        bounds: ReplayBounds,
        timeline: &T,
    ) -> ReplayAdvance {
        let mut result = ReplayAdvance::default();
        if !self.playing {
            return result;
        }
        let elapsed = (wall_ms - self.anchor_wall_ms).max(0);
        let due = f64_to_i64((i64_to_f64(elapsed) * self.rate_hz / 1_000.0).floor())
            .min(MAX_STEPS_PER_CALL + self.steps_since_anchor);
        while self.steps_since_anchor < due {
            let Some(next) = self.next_cursor(bounds, timeline) else {
                result.hit_cap = true;
                self.reanchor(wall_ms);
                return result;
            };
            self.cursor_ms = next;
            self.steps_since_anchor += 1;
            result.steps += 1;
        }
        result
    }

    /// One step forward by hand. Pauses. Returns whether the cursor moved.
    pub fn step_forward<T: ReplayTimeline + ?Sized>(
        &mut self,
        bounds: ReplayBounds,
        timeline: &T,
    ) -> bool {
        self.playing = false;
        self.next_cursor(bounds, timeline).is_some_and(|next| {
            self.cursor_ms = next;
            true
        })
    }

    /// One step back by hand. Pauses. Returns whether the cursor moved.
    pub fn step_back<T: ReplayTimeline + ?Sized>(
        &mut self,
        bounds: ReplayBounds,
        timeline: &T,
    ) -> bool {
        self.playing = false;
        self.prev_cursor(bounds, timeline).is_some_and(|prev| {
            self.cursor_ms = prev;
            true
        })
    }

    fn reanchor(&mut self, wall_ms: i64) {
        self.anchor_wall_ms = wall_ms;
        self.steps_since_anchor = 0;
    }

    /// Where the next step lands: the next grid point or the cursor plus the step, and when
    /// nothing happens before it, the first point that shows what happens. A step in ticks lands
    /// just after the n-th tick not yet seen, and waits (`None`) when there are not so many.
    fn next_cursor<T: ReplayTimeline + ?Sized>(
        &self,
        bounds: ReplayBounds,
        timeline: &T,
    ) -> Option<i64> {
        let cursor = self.cursor_ms;
        if cursor >= bounds.cap_ms {
            return None;
        }
        let unseen = timeline.next_data(cursor.saturating_sub(1));
        let next = match self.step {
            ClockStep::Grid(step) => {
                let next = grid_after(cursor, step);
                unseen
                    .filter(|unseen| *unseen >= next)
                    .map_or(next, |unseen| grid_after(unseen, step))
            }
            ClockStep::Relative(step) => {
                let next = cursor.saturating_add(step);
                unseen
                    .filter(|unseen| *unseen >= next)
                    .map_or(next, |unseen| unseen.saturating_add(1))
            }
            ClockStep::Ticks(count) => timeline
                .nth_data_after(cursor.saturating_sub(1), count)?
                .saturating_add(1),
        };
        let next = next.min(bounds.cap_ms);
        (next > cursor).then_some(next)
    }

    /// Where a step back lands: the previous grid point or the cursor minus the step, and when
    /// nothing happened since it, the point that hides the newest data. A step in ticks hides that
    /// many.
    fn prev_cursor<T: ReplayTimeline + ?Sized>(
        &self,
        bounds: ReplayBounds,
        timeline: &T,
    ) -> Option<i64> {
        let cursor = self.cursor_ms;
        if cursor <= bounds.floor_ms {
            return None;
        }
        let newest = timeline.prev_data(cursor);
        let prev = match self.step {
            ClockStep::Grid(step) => {
                let prev = grid_before(cursor, step);
                newest
                    .filter(|newest| prev > *newest)
                    .map_or(prev, |newest| newest - newest.rem_euclid(step))
            }
            ClockStep::Relative(step) => {
                let prev = cursor.saturating_sub(step);
                newest.filter(|newest| prev > *newest).unwrap_or(prev)
            }
            ClockStep::Ticks(count) => timeline
                .nth_data_before(cursor, count)
                .unwrap_or(bounds.floor_ms),
        };
        let prev = prev.max(bounds.floor_ms);
        (prev < cursor).then_some(prev)
    }
}

/// A step that cannot be taken is made one that can.
fn normal(step: ClockStep) -> ClockStep {
    match step {
        ClockStep::Grid(ms) => ClockStep::Grid(ms.max(1)),
        ClockStep::Relative(ms) => ClockStep::Relative(ms.max(1)),
        ClockStep::Ticks(count) => ClockStep::Ticks(count.max(1)),
    }
}

/// The first multiple of `step` after `time`.
fn grid_after(time: i64, step: i64) -> i64 {
    (time.div_euclid(step) + 1) * step
}

/// The last multiple of `step` before `time`.
fn grid_before(time: i64, step: i64) -> i64 {
    (-(-time).div_euclid(step) - 1) * step
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE_MS: i64 = 60_000;

    const BOUNDS: ReplayBounds = ReplayBounds {
        floor_ms: 0,
        cap_ms: 100 * MINUTE_MS,
    };

    #[test]
    fn stepping_forward_lands_on_the_next_multiple_of_the_step() {
        let mut clock = ReplayClock::new(2 * MINUTE_MS + 17_000, ClockStep::time(5 * MINUTE_MS));
        assert!(clock.step_forward(BOUNDS, &ReplayNoGaps));
        assert_eq!(clock.cursor_ms(), 5 * MINUTE_MS);
        assert!(clock.step_forward(BOUNDS, &ReplayNoGaps));
        assert_eq!(clock.cursor_ms(), 10 * MINUTE_MS);
    }

    #[test]
    fn stepping_back_lands_on_the_previous_multiple_of_the_step() {
        let mut clock = ReplayClock::new(12 * MINUTE_MS, ClockStep::time(5 * MINUTE_MS));
        assert!(clock.step_back(BOUNDS, &ReplayNoGaps));
        assert_eq!(clock.cursor_ms(), 10 * MINUTE_MS);
        assert!(clock.step_back(BOUNDS, &ReplayNoGaps));
        assert_eq!(clock.cursor_ms(), 5 * MINUTE_MS);
    }

    #[test]
    fn a_step_back_never_goes_under_the_floor() {
        let mut clock = ReplayClock::new(MINUTE_MS, ClockStep::time(5 * MINUTE_MS));
        assert!(clock.step_back(BOUNDS, &ReplayNoGaps));
        assert_eq!(clock.cursor_ms(), 0);
        assert!(!clock.step_back(BOUNDS, &ReplayNoGaps));
    }

    #[test]
    fn playing_takes_a_step_per_period_of_the_rate_without_drift() {
        let mut clock = ReplayClock::new(0, ClockStep::time(MINUTE_MS));
        clock.set_rate(2.0, 0);
        clock.play(10_000);
        let mut taken = 0;
        for frame in 1..=100 {
            taken += clock
                .advance(10_000 + frame * 33, BOUNDS, &ReplayNoGaps)
                .steps;
        }
        assert_eq!(taken, 6, "3 300 ms at 2 updates per second");
        assert_eq!(clock.cursor_ms(), 6 * MINUTE_MS);
    }

    #[test]
    fn a_late_frame_takes_several_steps_at_once() {
        let mut clock = ReplayClock::new(0, ClockStep::time(MINUTE_MS));
        clock.set_rate(10.0, 0);
        clock.play(0);
        let moved = clock.advance(1_000, BOUNDS, &ReplayNoGaps);
        assert_eq!(moved.steps, 10);
        assert_eq!(clock.cursor_ms(), 10 * MINUTE_MS);
    }

    #[test]
    fn changing_the_rate_while_playing_does_not_jump() {
        let mut clock = ReplayClock::new(0, ClockStep::time(MINUTE_MS));
        clock.play(0);
        clock.advance(2_500, BOUNDS, &ReplayNoGaps);
        let before = clock.cursor_ms();
        clock.set_rate(30.0, 2_500);
        assert_eq!(clock.cursor_ms(), before);
        assert_eq!(clock.advance(2_500, BOUNDS, &ReplayNoGaps).steps, 0);
    }

    #[test]
    fn a_paused_clock_does_not_move() {
        let mut clock = ReplayClock::new(0, ClockStep::time(MINUTE_MS));
        assert_eq!(clock.advance(60_000, BOUNDS, &ReplayNoGaps).steps, 0);
        assert_eq!(clock.cursor_ms(), 0);
    }

    #[test]
    fn a_cap_holds_the_cursor_and_reports_it() {
        let bounds = ReplayBounds {
            floor_ms: 0,
            cap_ms: 3 * MINUTE_MS + 30_000,
        };
        let mut clock = ReplayClock::new(0, ClockStep::time(MINUTE_MS));
        clock.set_rate(10.0, 0);
        clock.play(0);
        let moved = clock.advance(2_000, bounds, &ReplayNoGaps);
        assert!(moved.hit_cap);
        assert_eq!(moved.steps, 4, "three whole steps, then the cap itself");
        assert_eq!(clock.cursor_ms(), bounds.cap_ms);
        assert!(clock.is_playing());
    }

    #[test]
    fn a_gap_with_no_data_is_skipped_to_the_step_that_shows_the_next_data() {
        let times: &[i64] = &[0, MINUTE_MS, 60 * MINUTE_MS];
        let mut clock = ReplayClock::new(2 * MINUTE_MS, ClockStep::time(MINUTE_MS));
        assert!(clock.step_forward(BOUNDS, times));
        assert_eq!(
            clock.cursor_ms(),
            61 * MINUTE_MS,
            "the minute at 60 is now past"
        );
    }

    #[test]
    fn data_right_after_the_cursor_is_not_skipped() {
        let times: &[i64] = &[0, MINUTE_MS, 2 * MINUTE_MS];
        let mut clock = ReplayClock::new(MINUTE_MS, ClockStep::time(MINUTE_MS));
        assert!(clock.step_forward(BOUNDS, times));
        assert_eq!(clock.cursor_ms(), 2 * MINUTE_MS);
    }

    #[test]
    fn stepping_back_over_a_gap_lands_on_the_data_before_it() {
        let times: &[i64] = &[0, MINUTE_MS, 60 * MINUTE_MS];
        let mut clock = ReplayClock::new(61 * MINUTE_MS, ClockStep::time(MINUTE_MS));
        assert!(clock.step_back(BOUNDS, times));
        assert_eq!(clock.cursor_ms(), 60 * MINUTE_MS);
        assert!(clock.step_back(BOUNDS, times));
        assert_eq!(
            clock.cursor_ms(),
            MINUTE_MS,
            "the minute at 1 is hidden, only the one at 0 is seen"
        );
    }

    #[test]
    fn seeking_clamps_to_the_bounds_and_pauses() {
        let mut clock = ReplayClock::new(0, ClockStep::time(MINUTE_MS));
        clock.play(0);
        clock.seek(1_000 * MINUTE_MS, BOUNDS);
        assert_eq!(clock.cursor_ms(), BOUNDS.cap_ms);
        assert!(!clock.is_playing());
    }

    #[test]
    fn the_auto_step_is_the_shortest_chart_and_never_under_its_floor() {
        assert_eq!(
            replay_auto_step([3_600_000, 300_000, 86_400_000], MINUTE_MS),
            300_000
        );
        assert_eq!(replay_auto_step([15_000], MINUTE_MS), MINUTE_MS);
        assert_eq!(replay_auto_step(std::iter::empty(), MINUTE_MS), MINUTE_MS);
        assert_eq!(replay_auto_step([15_000, 300_000], 1_000), 15_000);
        assert_eq!(replay_auto_step([200], 0), 1_000, "never under a second");
    }

    #[test]
    fn a_step_that_divides_a_day_is_on_the_grid_and_another_is_relative() {
        assert_eq!(
            ClockStep::time(5 * MINUTE_MS),
            ClockStep::Grid(5 * MINUTE_MS)
        );
        assert_eq!(ClockStep::time(90_000), ClockStep::Grid(90_000));
        assert_eq!(
            ClockStep::time(7 * MINUTE_MS),
            ClockStep::Relative(7 * MINUTE_MS)
        );
        assert_eq!(ClockStep::time(5 * DAY_MS), ClockStep::Relative(5 * DAY_MS));
    }

    #[test]
    fn a_relative_step_moves_by_its_length_and_back_undoes_it() {
        let mut clock = ReplayClock::new(3 * MINUTE_MS + 20_000, ClockStep::time(7 * MINUTE_MS));
        assert!(clock.step_forward(BOUNDS, &ReplayNoGaps));
        assert_eq!(clock.cursor_ms(), 10 * MINUTE_MS + 20_000);
        assert!(clock.step_forward(BOUNDS, &ReplayNoGaps));
        assert_eq!(clock.cursor_ms(), 17 * MINUTE_MS + 20_000);
        assert!(clock.step_back(BOUNDS, &ReplayNoGaps));
        assert!(clock.step_back(BOUNDS, &ReplayNoGaps));
        assert_eq!(clock.cursor_ms(), 3 * MINUTE_MS + 20_000);
    }

    #[test]
    fn a_relative_step_skips_a_gap_to_the_step_that_shows_the_next_data() {
        let times: &[i64] = &[0, MINUTE_MS, 60 * MINUTE_MS];
        let mut clock = ReplayClock::new(2 * MINUTE_MS, ClockStep::time(7 * MINUTE_MS));
        assert!(clock.step_forward(BOUNDS, times));
        assert_eq!(
            clock.cursor_ms(),
            60 * MINUTE_MS + 1,
            "just after the tick that was unseen"
        );
        assert!(clock.step_back(BOUNDS, times));
        assert_eq!(
            clock.cursor_ms(),
            53 * MINUTE_MS + 1,
            "seven minutes back: the bar of minute 60 is hidden again"
        );
        assert!(clock.step_back(BOUNDS, times));
        assert_eq!(
            clock.cursor_ms(),
            MINUTE_MS,
            "then the gap is crossed in one step"
        );
    }

    #[test]
    fn a_step_in_ticks_lands_just_after_the_nth_tick_not_yet_seen() {
        let ticks: &[i64] = &[10, 20, 30, 40, 50];
        let bounds = ReplayBounds {
            floor_ms: 0,
            cap_ms: 1_000,
        };
        let mut clock = ReplayClock::new(15, ClockStep::Ticks(2));
        assert!(clock.step_forward(bounds, ticks));
        assert_eq!(clock.cursor_ms(), 31, "ticks 20 and 30 are seen, 40 is not");
        assert!(clock.step_forward(bounds, ticks));
        assert_eq!(clock.cursor_ms(), 51);
        assert!(
            !clock.step_forward(bounds, ticks),
            "no more ticks: it waits"
        );
        assert!(clock.step_back(bounds, ticks));
        assert_eq!(clock.cursor_ms(), 40, "ticks 40 and 50 are hidden");
    }

    #[test]
    fn one_tick_at_a_time_walks_every_tick_forward_and_back() {
        let ticks: &[i64] = &[10, 20, 30];
        let bounds = ReplayBounds {
            floor_ms: 0,
            cap_ms: 1_000,
        };
        let mut clock = ReplayClock::new(0, ClockStep::Ticks(1));
        let mut seen = Vec::new();
        while clock.step_forward(bounds, ticks) {
            seen.push(clock.cursor_ms());
        }
        assert_eq!(seen, vec![11, 21, 31]);
        let mut back = Vec::new();
        while clock.step_back(bounds, ticks) {
            back.push(clock.cursor_ms());
        }
        assert_eq!(back, vec![30, 20, 10, 0]);
    }

    #[test]
    fn ticks_of_the_same_millisecond_show_together() {
        let ticks: &[i64] = &[10, 10, 10, 20];
        let bounds = ReplayBounds {
            floor_ms: 0,
            cap_ms: 1_000,
        };
        let mut clock = ReplayClock::new(0, ClockStep::Ticks(1));
        assert!(clock.step_forward(bounds, ticks));
        assert_eq!(clock.cursor_ms(), 11);
    }
}

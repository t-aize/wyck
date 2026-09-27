//! A replay session: a scrubbable cursor over history, with play/pause/speed and
//! step-by-step controls, exactly what a TradingView-style replay control bar drives.
//! Pure and deterministic: it holds no market data and does no I/O or wall-clock reads of
//! its own. The app's `PacedClock` (in the `wyck` crate) is what actually reads real wall
//! time and calls [`ReplaySession::advance_to`] on a timer; this type never reaches for
//! the clock itself, which is what makes it trivially unit-testable.

/// The default playback speed multiplier (real time).
const DEFAULT_SPEED: f64 = 1.0;
/// The slowest and fastest speed multipliers a caller can select. Zero would never
/// advance the cursor; a negative value would run the replay backwards while "playing",
/// neither of which a speed control should be able to produce.
const MIN_SPEED: f64 = 0.05;
const MAX_SPEED: f64 = 100.0;

/// A replay session's cursor and playback state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplaySession {
    start_ms: i64,
    step_ms: i64,
    speed: f64,
    playing: bool,
    cursor_ms: i64,
    /// The wall-clock time and cursor position last anchored by [`Self::play`] or a
    /// speed change while playing. [`Self::advance_to`] computes the new cursor from
    /// this fixed baseline (`anchor_cursor_ms + elapsed * speed`) rather than
    /// accumulating a delta on every call, so repeated small ticks cannot drift from
    /// floating-point rounding error.
    anchor_wall_ms: i64,
    anchor_cursor_ms: i64,
}

impl ReplaySession {
    /// A new, paused session positioned at `start_ms`, stepping by `step_ms` at a time
    /// (a bar's period in milliseconds for bar-by-bar replay, or a smaller increment for
    /// tick-by-tick). `step_ms` below 1 is treated as 1: a step must move the cursor.
    #[must_use]
    pub fn new(start_ms: i64, step_ms: i64) -> Self {
        Self {
            start_ms,
            step_ms: step_ms.max(1),
            speed: DEFAULT_SPEED,
            playing: false,
            cursor_ms: start_ms,
            anchor_wall_ms: 0,
            anchor_cursor_ms: start_ms,
        }
    }

    /// The current replay time, in Unix milliseconds.
    #[must_use]
    pub fn cursor_ms(&self) -> i64 {
        self.cursor_ms
    }

    /// The picked start point.
    #[must_use]
    pub fn start_ms(&self) -> i64 {
        self.start_ms
    }

    /// Whether the session is currently playing (auto-advancing).
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// The current speed multiplier.
    #[must_use]
    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// Moves the cursor to exactly `time_ms` and pauses: re-picking a point (TradingView's
    /// "Go to") does not resume playback on its own.
    pub fn seek(&mut self, time_ms: i64) {
        self.cursor_ms = time_ms;
        self.playing = false;
    }

    /// Moves the cursor back to the start point and pauses.
    pub fn jump_to_start(&mut self) {
        self.seek(self.start_ms);
    }

    /// Moves the cursor by exactly one step, forward or back, and pauses: stepping is a
    /// discrete manual action, distinct from playback.
    pub fn step(&mut self, forward: bool) {
        self.playing = false;
        self.cursor_ms += if forward { self.step_ms } else { -self.step_ms };
    }

    /// Starts (or resumes) auto-advancing from `wall_now_ms`, the caller's current wall
    /// time. Every later call to [`Self::advance_to`] moves the cursor relative to this
    /// moment until playback is paused, sought, stepped, or re-anchored by
    /// [`Self::set_speed`].
    pub fn play(&mut self, wall_now_ms: i64) {
        self.playing = true;
        self.anchor_wall_ms = wall_now_ms;
        self.anchor_cursor_ms = self.cursor_ms;
    }

    /// Stops auto-advancing. The cursor stays exactly where it is.
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Recomputes the cursor for `wall_now_ms` of real time having passed since the last
    /// anchor, at the current speed. A no-op while paused, and never moves the cursor
    /// backward (a caller passing an out-of-order `wall_now_ms` cannot rewind playback).
    pub fn advance_to(&mut self, wall_now_ms: i64) {
        if !self.playing {
            return;
        }
        let elapsed = (wall_now_ms - self.anchor_wall_ms).max(0);
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let delta = (elapsed as f64 * self.speed) as i64;
        self.cursor_ms = self.cursor_ms.max(self.anchor_cursor_ms + delta);
    }

    /// Sets the playback speed multiplier, clamped to `[0.05, 100.0]`. If currently
    /// playing, re-anchors at `wall_now_ms` first so the cursor does not jump: without
    /// this, changing speed mid-playback would retroactively apply the new speed to time
    /// that already elapsed under the old one.
    pub fn set_speed(&mut self, speed: f64, wall_now_ms: i64) {
        if self.playing {
            self.advance_to(wall_now_ms);
            self.anchor_wall_ms = wall_now_ms;
            self.anchor_cursor_ms = self.cursor_ms;
        }
        self.speed = speed.clamp(MIN_SPEED, MAX_SPEED);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_session_starts_paused_at_its_start_point() {
        let session = ReplaySession::new(1_000, 60_000);
        assert_eq!(session.cursor_ms(), 1_000);
        assert_eq!(session.start_ms(), 1_000);
        assert!(!session.is_playing());
        assert_eq!(session.speed(), DEFAULT_SPEED);
    }

    #[test]
    fn seek_moves_the_cursor_and_pauses() {
        let mut session = ReplaySession::new(0, 60_000);
        session.play(0);
        session.seek(5_000);
        assert_eq!(session.cursor_ms(), 5_000);
        assert!(!session.is_playing());
    }

    #[test]
    fn jump_to_start_returns_to_the_picked_point() {
        let mut session = ReplaySession::new(10_000, 1_000);
        session.seek(50_000);
        session.jump_to_start();
        assert_eq!(session.cursor_ms(), 10_000);
    }

    #[test]
    fn stepping_moves_by_exactly_one_step_and_pauses() {
        let mut session = ReplaySession::new(0, 60_000);
        session.play(0);
        session.step(true);
        assert_eq!(session.cursor_ms(), 60_000);
        assert!(!session.is_playing());
        session.step(false);
        assert_eq!(session.cursor_ms(), 0);
    }

    #[test]
    fn a_step_size_below_one_is_floored_to_one() {
        let mut session = ReplaySession::new(0, 0);
        session.step(true);
        assert_eq!(session.cursor_ms(), 1);
    }

    #[test]
    fn playing_advances_the_cursor_proportionally_to_elapsed_wall_time() {
        let mut session = ReplaySession::new(0, 1_000);
        session.play(1_000);
        session.advance_to(3_000); // 2s of wall time at 1x
        assert_eq!(session.cursor_ms(), 2_000);
    }

    #[test]
    fn speed_scales_how_fast_the_cursor_advances() {
        let mut session = ReplaySession::new(0, 1_000);
        session.set_speed(10.0, 0);
        session.play(0);
        session.advance_to(1_000); // 1s of wall time at 10x
        assert_eq!(session.cursor_ms(), 10_000);
    }

    #[test]
    fn advance_to_is_a_no_op_while_paused() {
        let mut session = ReplaySession::new(0, 1_000);
        session.advance_to(5_000);
        assert_eq!(session.cursor_ms(), 0);
    }

    #[test]
    fn advance_to_never_rewinds_on_an_out_of_order_call() {
        let mut session = ReplaySession::new(0, 1_000);
        session.play(1_000);
        session.advance_to(3_000);
        let after_forward = session.cursor_ms();
        session.advance_to(2_000); // an earlier wall time arriving late
        assert_eq!(session.cursor_ms(), after_forward);
    }

    #[test]
    fn changing_speed_mid_playback_does_not_jump_the_cursor() {
        let mut session = ReplaySession::new(0, 1_000);
        session.play(0);
        session.advance_to(1_000); // cursor at 1_000 (1x for 1s)
        let before = session.cursor_ms();
        session.set_speed(50.0, 1_000); // re-anchors here, does not jump yet
        assert_eq!(session.cursor_ms(), before);
        session.advance_to(1_100); // 100ms more, now at 50x
        assert_eq!(session.cursor_ms(), before + 5_000);
    }

    #[test]
    fn speed_is_clamped_to_a_sane_range() {
        let mut session = ReplaySession::new(0, 1_000);
        session.set_speed(0.0, 0);
        assert_eq!(session.speed(), MIN_SPEED);
        session.set_speed(1_000.0, 0);
        assert_eq!(session.speed(), MAX_SPEED);
    }

    #[test]
    fn pausing_freezes_the_cursor_where_it_is() {
        let mut session = ReplaySession::new(0, 1_000);
        session.play(0);
        session.advance_to(1_000);
        session.pause();
        let frozen = session.cursor_ms();
        session.advance_to(10_000);
        assert_eq!(session.cursor_ms(), frozen);
    }
}

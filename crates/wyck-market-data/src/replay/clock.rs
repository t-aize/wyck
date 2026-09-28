//! A clock abstraction for the Replay feature's stepping logic, so it does not depend on
//! wall-clock time directly. [`TestClock`] only moves when told to, in milliseconds, with
//! no notion of "real" time at all.

/// A point in (simulated) time, advanced only when told to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct TestClock {
    now_ms: i64,
}

impl TestClock {
    /// A clock starting at `now_ms`.
    #[must_use]
    pub fn new(now_ms: i64) -> Self {
        Self { now_ms }
    }

    /// The current (simulated) time, in Unix milliseconds.
    #[must_use]
    pub fn now_ms(&self) -> i64 {
        self.now_ms
    }

    /// Moves the clock to exactly `time_ms`. Not restricted to move forward: a replay
    /// clock is scrubbed, not just played.
    pub fn set(&mut self, time_ms: i64) {
        self.now_ms = time_ms;
    }

    /// Moves the clock by `delta_ms` (negative moves it back).
    pub fn advance(&mut self, delta_ms: i64) {
        self.now_ms += delta_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_clock_starts_where_told() {
        assert_eq!(TestClock::new(42).now_ms(), 42);
    }

    #[test]
    fn set_moves_to_an_exact_time_forward_or_back() {
        let mut clock = TestClock::new(100);
        clock.set(50);
        assert_eq!(clock.now_ms(), 50);
        clock.set(200);
        assert_eq!(clock.now_ms(), 200);
    }

    #[test]
    fn advance_adds_a_signed_delta() {
        let mut clock = TestClock::new(100);
        clock.advance(10);
        assert_eq!(clock.now_ms(), 110);
        clock.advance(-30);
        assert_eq!(clock.now_ms(), 80);
    }
}

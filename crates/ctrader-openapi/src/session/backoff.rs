//! How long [`crate::session::Session`] waits between two attempts to connect.

use std::time::Duration;

/// How long to wait between two attempts to connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    /// The wait after the first failure.
    pub initial: Duration,
    /// The longest wait.
    pub max: Duration,
    /// What each wait is multiplied by (at least 1).
    pub factor: u32,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
            factor: 2,
        }
    }
}

impl Backoff {
    /// The wait before attempt number `attempt` (1 for the first retry), without jitter.
    ///
    /// ```
    /// use std::time::Duration;
    /// use ctrader_openapi::session::Backoff;
    ///
    /// let backoff = Backoff { initial: Duration::from_secs(1), max: Duration::from_secs(10), factor: 2 };
    /// assert_eq!(backoff.delay(1), Duration::from_secs(1));
    /// assert_eq!(backoff.delay(3), Duration::from_secs(4));
    /// assert_eq!(backoff.delay(9), Duration::from_secs(10)); // capped
    /// ```
    #[must_use]
    pub fn delay(&self, attempt: u32) -> Duration {
        let factor = u128::from(self.factor.max(1));
        let mut wait = self.initial.as_millis().max(1);
        for _ in 1..attempt.max(1) {
            wait = wait.saturating_mul(factor);
            if wait >= self.max.as_millis() {
                break;
            }
        }
        Duration::from_millis(u64::try_from(wait.min(self.max.as_millis())).unwrap_or(u64::MAX))
    }

    /// [`Backoff::delay`] plus up to a fifth more, taken from `noise` (any number: the low bits
    /// are used), so that many programs do not retry at the same instant.
    #[must_use]
    pub fn jittered(&self, attempt: u32, noise: u32) -> Duration {
        let base = self.delay(attempt);
        let extra = base.as_millis() / 5 * u128::from(noise % 1000) / 1000;
        base + Duration::from_millis(u64::try_from(extra).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_doubles_up_to_the_cap() {
        let backoff = Backoff {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(10),
            factor: 2,
        };
        let waits: Vec<u64> = (1..=6).map(|n| backoff.delay(n).as_secs()).collect();
        assert_eq!(waits, vec![1, 2, 4, 8, 10, 10]);
    }

    #[test]
    fn a_huge_attempt_number_does_not_overflow() {
        let backoff = Backoff::default();
        assert_eq!(backoff.delay(u32::MAX), Duration::from_secs(60));
        assert_eq!(
            backoff.delay(0),
            Duration::from_secs(1),
            "attempt 0 is the first wait"
        );
    }

    #[test]
    fn a_factor_of_one_keeps_the_wait_constant_and_zero_is_treated_as_one() {
        let constant = Backoff {
            initial: Duration::from_millis(500),
            max: Duration::from_secs(60),
            factor: 1,
        };
        assert_eq!(constant.delay(9), Duration::from_millis(500));
        let zero = Backoff {
            factor: 0,
            ..constant
        };
        assert_eq!(zero.delay(9), Duration::from_millis(500));
    }

    #[test]
    fn jitter_only_adds_and_never_more_than_a_fifth() {
        let backoff = Backoff::default();
        let base = backoff.delay(3);
        for noise in [0, 1, 500, 999, 1000, 123_456_789] {
            let jittered = backoff.jittered(3, noise);
            assert!(jittered >= base);
            assert!(jittered <= base + base / 5, "{jittered:?}");
        }
        assert_eq!(backoff.jittered(3, 0), base);
    }
}

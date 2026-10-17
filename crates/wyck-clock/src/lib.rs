//! The one place the workspace reads the wall clock.
//!
//! Everything that needs "now" (the expiry of a token, the stamp of a backup, the age of an alert)
//! asks [`now`] or [`unix_ms`], and nothing else calls `SystemTime::now` or `chrono::*::now`
//! (`clippy.toml` forbids it). That makes time something a test can decide:
//!
//! - [`with_clock`] gives the current thread another clock for the length of a closure, for a unit
//!   test that needs "ten minutes later" without sleeping;
//! - [`install`] gives the whole process another clock, once, at start, for a run that is
//!   replayed or simulated.
//!
//! The default is [`SystemClock`]. Only `Instant` (monotonic time, for timeouts and rate limits)
//! stays outside: it has no calendar meaning and cannot be set.

use std::cell::RefCell;
use std::fmt::Debug;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A source of the current date and time.
pub trait Clock: Debug + Send + Sync {
    /// The current time.
    fn now(&self) -> SystemTime;
}

pub type SharedClock = Arc<dyn Clock>;

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    #[expect(
        clippy::disallowed_methods,
        reason = "the one read of the wall clock of the workspace; everything else asks this crate"
    )]
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// A clock that only moves when it is told to: for tests.
#[derive(Debug)]
pub struct ManualClock {
    now: Mutex<SystemTime>,
}

impl ManualClock {
    /// A clock stopped at `at`.
    #[must_use]
    pub fn new(at: SystemTime) -> Self {
        Self {
            now: Mutex::new(at),
        }
    }

    /// A clock stopped at `ms` milliseconds after the Unix epoch.
    #[must_use]
    pub fn at_unix_ms(ms: u64) -> Self {
        Self::new(UNIX_EPOCH + Duration::from_millis(ms))
    }

    /// Moves the clock to `at`.
    pub fn set(&self, at: SystemTime) {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner) = at;
    }

    /// Moves the clock forward by `by`.
    pub fn advance(&self, by: Duration) {
        let mut now = self.now.lock().unwrap_or_else(PoisonError::into_inner);
        *now = now.checked_add(by).unwrap_or(*now);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> SystemTime {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

static INSTALLED: OnceLock<SharedClock> = OnceLock::new();

thread_local! {
    static OVERRIDE: RefCell<Option<SharedClock>> = const { RefCell::new(None) };
}

/// Gives the whole process `clock`. Only the first call has an effect.
///
/// # Errors
///
/// Gives `clock` back when a clock was installed already.
pub fn install(clock: SharedClock) -> Result<(), SharedClock> {
    INSTALLED.set(clock)
}

/// Runs `f` with `clock` as the clock of this thread, and puts the previous one back afterwards,
/// even if `f` panics. Work `f` hands to other threads does not see it.
pub fn with_clock<R>(clock: SharedClock, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<SharedClock>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            OVERRIDE.with(|slot| *slot.borrow_mut() = previous);
        }
    }
    let previous = OVERRIDE.with(|slot| slot.borrow_mut().replace(clock));
    let _restore = Restore(previous);
    f()
}

/// The current time: the clock of this thread if [`with_clock`] gave one, else the installed one,
/// else the system's.
#[must_use]
pub fn now() -> SystemTime {
    let overridden = OVERRIDE.with(|slot| slot.borrow().clone());
    if let Some(clock) = overridden {
        return clock.now();
    }
    match INSTALLED.get() {
        Some(clock) => clock.now(),
        None => SystemClock.now(),
    }
}

/// The current time in milliseconds since the Unix epoch; 0 for a clock set before it, and the
/// largest `i64` for one set after the year 292 million.
#[must_use]
pub fn unix_ms() -> i64 {
    now().duration_since(UNIX_EPOCH).map_or(0, |since| {
        i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_clock_moves_only_when_told() {
        let clock = Arc::new(ManualClock::at_unix_ms(1_000));
        let shared: SharedClock = Arc::<ManualClock>::clone(&clock);
        with_clock(shared, || {
            assert_eq!(unix_ms(), 1_000);
            assert_eq!(unix_ms(), 1_000);
            clock.advance(Duration::from_secs(2));
            assert_eq!(unix_ms(), 3_000);
            clock.set(UNIX_EPOCH);
            assert_eq!(unix_ms(), 0);
        });
    }

    #[test]
    fn the_override_is_undone_and_nests() {
        let outer = Arc::new(ManualClock::at_unix_ms(10));
        let inner = Arc::new(ManualClock::at_unix_ms(20));
        with_clock(outer, || {
            with_clock(inner, || assert_eq!(unix_ms(), 20));
            assert_eq!(unix_ms(), 10);
        });
        assert!(unix_ms() > 1_577_836_800_000);
    }

    #[test]
    fn the_override_is_undone_after_a_panic() {
        let clock = Arc::new(ManualClock::at_unix_ms(5));
        let result = std::panic::catch_unwind(|| with_clock(clock, || panic!("inside")));
        assert!(result.is_err());
        assert!(unix_ms() > 1_577_836_800_000);
    }

    #[test]
    fn another_thread_does_not_see_the_override() {
        let clock = Arc::new(ManualClock::at_unix_ms(7));
        with_clock(clock, || {
            let other = std::thread::spawn(unix_ms).join().unwrap();
            assert!(other > 1_577_836_800_000);
        });
    }

    #[test]
    fn a_clock_before_the_epoch_reads_as_zero() {
        let clock = Arc::new(ManualClock::new(UNIX_EPOCH - Duration::from_secs(1)));
        with_clock(clock, || assert_eq!(unix_ms(), 0));
    }
}

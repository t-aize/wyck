//! Market data the screens share: the live hub, history loading and the clock.

pub mod live;
pub mod load;

/// The current time in Unix milliseconds, from the clock of the workspace (`wyck-clock`).
pub fn now_ms() -> i64 {
    wyck_clock::unix_ms()
}

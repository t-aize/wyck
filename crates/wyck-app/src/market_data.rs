//! Market data the screens share: the live hub, history loading and the clock.

pub mod live;
pub mod load;

use std::time::{SystemTime, UNIX_EPOCH};

/// The current time in Unix milliseconds.
#[expect(
    clippy::disallowed_methods,
    reason = "wall-clock read, to be routed through an injectable clock"
)]
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
}

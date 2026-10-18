//! Replay core: a cursor in time, the candles a chart shows at that cursor, and how a replay starts.
//!
//! Pure and deterministic: nothing here reads a clock or loads data. The app feeds it wall times
//! and bars; a replaying chart's series is the prefix that `ReplayFeed` writes, so everything that
//! reads the series (indicators, scripts, export) only ever sees the past.

mod clock;
mod feed;
mod forming;
mod path;
mod start;
mod step;
mod tick_feed;

pub use clock::{
    ClockStep, REPLAY_RATES, ReplayAdvance, ReplayBounds, ReplayClock, ReplayNoGaps,
    ReplayTimeline, replay_auto_step,
};
pub use feed::{ReplayChange, ReplayFeed, ReplayInput, ReplaySource};
pub use forming::{replay_fold_hybrid, replay_fold_minutes, replay_fold_ticks};
pub use path::replay_partial_bar;
pub use start::{replay_align_to_bar, replay_random_start};
pub use step::{REPLAY_STEP_HELP, ReplayStep, parse_step};
pub use tick_feed::{ReplaySecondsFeed, ReplayTickFeed};

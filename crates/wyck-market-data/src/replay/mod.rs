//! The Replay feature's pure logic: a deterministic clock ([`clock::TestClock`]) and the
//! scrubbable, play/pause/speed session built on it ([`session::ReplaySession`]). Neither
//! touches the catalog, a chart, or GPUI: the app crate's `ReplayFeed` (not yet built)
//! drives a session and turns each newly-crossed bar/tick into the same `LiveUpdate`
//! shape the real live feed produces.

pub mod clock;
pub mod session;

pub use clock::TestClock;
pub use session::ReplaySession;

//! The Replay feature's pure logic: the scrubbable, play/pause/speed session
//! ([`session::ReplaySession`]) and its saved preferences. Neither touches the catalog, a
//! chart, or GPUI: the app's `ReplayFeed` drives a session and turns each newly-crossed
//! bar/tick into the same `LiveUpdate` shape the real live feed produces.

pub mod prefs;
pub mod session;

pub use prefs::ReplayPrefs;
pub use session::ReplaySession;

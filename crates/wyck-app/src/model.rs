//! The data a front end's windows share, with no UI toolkit in it.
//!
//! [`AppModel`] is what every window renders: the latest engine state, the activity log, the
//! notices shown to the user and the banner that explains a startup problem. The UI wraps it in a
//! toolkit's state container and calls the methods below; the methods are plain Rust and tested as
//! such.

use std::collections::VecDeque;
use std::sync::Arc;

use wyck_engine::domain::UnixMillis;
use wyck_engine::{EngineState, Event};

use crate::messages::Notice;
use crate::presentation::{ActivityRow, activity_row};

/// How many notices are kept.
const MAX_NOTICES: usize = 30;
/// How many activity rows are kept.
const MAX_ACTIVITY: usize = 60;

/// A notice and when it was raised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedNotice {
    /// When, in Unix milliseconds.
    pub at: UnixMillis,
    /// What.
    pub notice: Notice,
}

/// The shared data of all windows.
#[derive(Debug, Clone)]
pub struct AppModel {
    /// The latest engine state.
    pub state: Arc<EngineState>,
    /// What happened, most recent first, without the routine refreshes.
    pub activity: Vec<ActivityRow>,
    /// Notices, most recent first.
    pub notices: VecDeque<TimedNotice>,
    /// Problems that outlive a notice: no profile, a session that ended badly, a shortcut that
    /// could not be registered. Shown until the user has dealt with them.
    pub banners: Vec<Notice>,
}

impl AppModel {
    /// A model showing `state`, with the startup `banners`.
    #[must_use]
    pub fn new(state: Arc<EngineState>, banners: Vec<Notice>) -> Self {
        Self {
            state,
            activity: Vec::new(),
            notices: VecDeque::new(),
            banners,
        }
    }

    /// Takes a new engine state and the engine's recent events (oldest first).
    pub fn apply(&mut self, state: Arc<EngineState>, events: &[Event]) {
        self.state = state;
        self.activity = events
            .iter()
            .rev()
            .filter_map(activity_row)
            .take(MAX_ACTIVITY)
            .collect();
    }

    /// Records a notice, most recent first, keeping the last few.
    pub fn push_notice(&mut self, notice: Notice, now: UnixMillis) {
        self.notices.push_front(TimedNotice { at: now, notice });
        self.notices.truncate(MAX_NOTICES);
    }

    /// The most recent notice, if any.
    #[must_use]
    pub fn latest_notice(&self) -> Option<&TimedNotice> {
        self.notices.front()
    }
}

#[cfg(test)]
mod tests {
    use wyck_engine::{Engine, EngineConfig, EventKind};

    use super::*;
    use crate::messages::Level;

    fn state() -> Arc<EngineState> {
        let config = EngineConfig {
            calendar_enabled: false,
            ..EngineConfig::default()
        };
        let engine = Engine::start(config).unwrap();
        engine.handle().state()
    }

    fn event(at: i64, kind: EventKind) -> Event {
        Event {
            at,
            account: None,
            command: None,
            kind,
        }
    }

    #[test]
    fn the_activity_log_is_newest_first_and_skips_routine_events() {
        let mut model = AppModel::new(state(), Vec::new());
        model.apply(
            state(),
            &[
                event(
                    1_000,
                    EventKind::RefreshFailed {
                        message: "first".into(),
                    },
                ),
                event(2_000, EventKind::AccountUpdated),
                event(
                    3_000,
                    EventKind::RefreshFailed {
                        message: "second".into(),
                    },
                ),
            ],
        );
        assert_eq!(model.activity.len(), 2);
        assert!(model.activity[0].text.contains("second"));
        assert!(model.activity[1].text.contains("first"));
    }

    #[test]
    fn the_activity_log_is_bounded() {
        let mut model = AppModel::new(state(), Vec::new());
        let events: Vec<Event> = (0..500)
            .map(|i| {
                event(
                    i,
                    EventKind::RefreshFailed {
                        message: format!("n{i}"),
                    },
                )
            })
            .collect();
        model.apply(state(), &events);
        assert_eq!(model.activity.len(), MAX_ACTIVITY);
        assert!(
            model.activity[0].text.contains("n499"),
            "the newest is kept"
        );
    }

    #[test]
    fn notices_keep_the_latest_and_are_bounded() {
        let mut model = AppModel::new(state(), Vec::new());
        assert!(model.latest_notice().is_none());
        for i in 0..100 {
            model.push_notice(Notice::info(format!("n{i}")), i);
        }
        assert_eq!(model.notices.len(), MAX_NOTICES);
        let latest = model.latest_notice().unwrap();
        assert_eq!(latest.notice.title, "n99");
        assert_eq!(latest.notice.level, Level::Info);
    }
}

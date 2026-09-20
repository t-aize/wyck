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

use crate::messages::{Level, Notice};
use crate::presentation::{ActivityRow, activity_row};

/// How many notices are kept.
const MAX_NOTICES: usize = 30;
/// How many activity rows are kept.
const MAX_ACTIVITY: usize = 60;
/// How many toasts are on screen at once.
const MAX_TOASTS: usize = 4;

/// How long a toast takes to fade out before it is dropped, in milliseconds.
pub const TOAST_EXIT_MS: i64 = 160;

/// How long a toast stays, in milliseconds. Errors stay longer: they are worth reading twice.
#[must_use]
pub const fn toast_ttl_ms(level: Level) -> i64 {
    match level {
        Level::Error => 15_000,
        Level::Warning => 10_000,
        _ => 6_000,
    }
}

/// A notice and when it was raised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedNotice {
    /// When, in Unix milliseconds.
    pub at: UnixMillis,
    /// What.
    pub notice: Notice,
}

/// A notice on screen, until it expires or is dismissed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// Identifies the toast to dismiss it.
    pub id: u64,
    /// When it appeared.
    pub at: UnixMillis,
    /// What it says.
    pub notice: Notice,
    /// When it started to leave, once it has expired or been dismissed. The screen fades it out
    /// during [`TOAST_EXIT_MS`] and the model then drops it.
    pub leaving: Option<UnixMillis>,
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
    /// Notices currently on screen, most recent first.
    pub toasts: Vec<Toast>,
    next_toast: u64,
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
            toasts: Vec::new(),
            next_toast: 0,
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

    /// Records a notice, most recent first, keeping the last few, and shows it as a toast.
    pub fn push_notice(&mut self, notice: Notice, now: UnixMillis) {
        self.next_toast += 1;
        self.toasts.insert(
            0,
            Toast {
                id: self.next_toast,
                at: now,
                notice: notice.clone(),
                leaving: None,
            },
        );
        self.toasts.truncate(MAX_TOASTS);
        self.notices.push_front(TimedNotice { at: now, notice });
        self.notices.truncate(MAX_NOTICES);
    }

    /// Starts a toast's exit. Returns whether there was such a toast that was not already leaving.
    pub fn dismiss_toast(&mut self, id: u64, now: UnixMillis) -> bool {
        match self
            .toasts
            .iter_mut()
            .find(|t| t.id == id && t.leaving.is_none())
        {
            Some(toast) => {
                toast.leaving = Some(now);
                true
            }
            None => false,
        }
    }

    /// Starts the exit of the toasts that have been on screen long enough, and drops the ones
    /// whose exit is over. Returns whether anything changed.
    pub fn expire_toasts(&mut self, now: UnixMillis) -> bool {
        let mut changed = false;
        for toast in &mut self.toasts {
            if toast.leaving.is_none()
                && now.saturating_sub(toast.at) >= toast_ttl_ms(toast.notice.level)
            {
                toast.leaving = Some(now);
                changed = true;
            }
        }
        let before = self.toasts.len();
        self.toasts.retain(|t| {
            t.leaving
                .is_none_or(|at| now.saturating_sub(at) < TOAST_EXIT_MS)
        });
        changed || self.toasts.len() != before
    }

    /// Removes the banner at `index`. Returns whether there was one.
    pub fn dismiss_banner(&mut self, index: usize) -> bool {
        if index < self.banners.len() {
            self.banners.remove(index);
            true
        } else {
            false
        }
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
    fn a_notice_is_a_toast_until_it_expires_or_is_dismissed() {
        let mut model = AppModel::new(state(), Vec::new());
        model.push_notice(Notice::info("a"), 0);
        model.push_notice(Notice::info("b"), 1_000);
        assert_eq!(model.toasts.len(), 2);
        assert_eq!(model.toasts[0].notice.title, "b", "newest first");

        assert!(!model.expire_toasts(5_000), "nothing is old enough yet");
        assert!(
            model.expire_toasts(6_500),
            "a is 6.5 s old and starts to leave"
        );
        assert_eq!(model.toasts.len(), 2, "it fades out before it goes");
        assert_eq!(model.toasts[1].leaving, Some(6_500));
        assert!(!model.expire_toasts(6_600), "still fading");
        assert!(
            model.expire_toasts(6_500 + TOAST_EXIT_MS),
            "the fade is over"
        );
        assert_eq!(model.toasts.len(), 1);
        let id = model.toasts[0].id;
        assert!(model.dismiss_toast(id, 7_000));
        assert!(!model.dismiss_toast(id, 7_010), "already leaving");
        assert_eq!(model.toasts[0].leaving, Some(7_000));
        assert!(model.expire_toasts(7_000 + TOAST_EXIT_MS));
        assert!(model.toasts.is_empty());
        assert_eq!(model.notices.len(), 2, "the history keeps them");
    }

    #[test]
    fn an_error_toast_outlives_an_info_one() {
        let mut model = AppModel::new(state(), Vec::new());
        model.push_notice(Notice::info("i"), 0);
        model.push_notice(
            Notice {
                level: Level::Error,
                title: "e".into(),
                detail: None,
                hint: None,
            },
            0,
        );
        model.expire_toasts(7_000);
        model.expire_toasts(7_000 + TOAST_EXIT_MS);
        assert_eq!(model.toasts.len(), 1);
        assert_eq!(model.toasts[0].notice.title, "e");
        assert!(model.toasts[0].leaving.is_none());
    }

    #[test]
    fn only_a_few_toasts_are_shown_at_once() {
        let mut model = AppModel::new(state(), Vec::new());
        for i in 0..10 {
            model.push_notice(Notice::info(format!("n{i}")), i);
        }
        assert_eq!(model.toasts.len(), MAX_TOASTS);
        assert_eq!(model.toasts[0].notice.title, "n9");
    }

    #[test]
    fn a_banner_can_be_dismissed_by_position() {
        let mut model = AppModel::new(state(), vec![Notice::info("one"), Notice::info("two")]);
        assert!(model.dismiss_banner(0));
        assert_eq!(model.banners[0].title, "two");
        assert!(!model.dismiss_banner(5));
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

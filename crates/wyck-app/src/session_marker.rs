//! Knowing that the last run ended badly **while something was at stake**.
//!
//! A trading app that crashes while orders are in flight leaves the user with a question: what
//! state is my account in? The marker answers the first half. At startup a small file is written
//! next to the logs; a clean exit removes it. If the file is still there at the next startup, the
//! last session was killed, crashed or lost power.
//!
//! That alone is not worth a warning: stopping a session from an editor or a terminal looks the
//! same, and while the engine is in dry-run mode no order can be in flight, so the alarm would
//! only teach the user to ignore it. The marker therefore records whether the session was ever
//! **at risk** (see [`at_risk`]): the engine armed, an order in flight, or an order whose outcome
//! is unknown. Only a session that ended badly after that is reported ([`PreviousSession::at_risk`]).
//! A marker that cannot be read is reported too, since nothing can be said about it.
//!
//! The marker deliberately says nothing about the orders themselves: it cannot know. It only makes
//! sure the user is asked to look.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use wyck_engine::state::WarningKind;
use wyck_engine::{EngineState, TradingMode};

/// What is known about a session that did not end cleanly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousSession {
    /// When it started, in Unix milliseconds, if the marker could be read.
    pub started_at: Option<i64>,
    /// Its process id, if the marker could be read.
    pub pid: Option<u32>,
    /// Whether something was at stake when it ended: it had been at risk, or the marker could not
    /// be read. Only then is the user warned.
    pub at_risk: bool,
}

/// The marker of the running session. Call [`SessionMarker::finish`] on a clean exit.
#[derive(Debug)]
#[must_use = "call `finish` on a clean exit, or the next start may report a crash"]
pub struct SessionMarker {
    path: PathBuf,
    started_at: i64,
    pid: u32,
    at_risk: AtomicBool,
}

const FILE_NAME: &str = "session.marker";

/// Whether the engine is in a state where a sudden end could leave an order the user does not know
/// about: armed, an order in flight, or an order whose outcome is unknown.
#[must_use]
pub fn at_risk(state: &EngineState) -> bool {
    state.mode != TradingMode::DryRun
        || !state.orders_in_flight.is_empty()
        || state
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::UnknownOrder)
}

fn contents(started_at: i64, pid: u32, at_risk: bool) -> String {
    format!(
        "started_at={started_at}\npid={pid}\nat_risk={}\n",
        u8::from(at_risk)
    )
}

impl SessionMarker {
    /// Writes the marker for this session and reports the previous one if it did not finish.
    ///
    /// # Errors
    ///
    /// An I/O error when the marker cannot be written. The caller should carry on without it:
    /// losing crash detection is not a reason to refuse to start.
    pub fn begin(
        dir: &Path,
        started_at: i64,
        pid: u32,
    ) -> io::Result<(Self, Option<PreviousSession>)> {
        fs::create_dir_all(dir)?;
        let path = dir.join(FILE_NAME);
        let previous = match fs::read_to_string(&path) {
            Ok(text) => Some(parse(&text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(_) => Some(PreviousSession {
                started_at: None,
                pid: None,
                at_risk: true,
            }),
        };
        fs::write(&path, contents(started_at, pid, false))?;
        Ok((
            Self {
                path,
                started_at,
                pid,
                at_risk: AtomicBool::new(false),
            },
            previous,
        ))
    }

    /// Records that this session is now at risk (see [`at_risk`]), so that ending badly from here
    /// on is reported at the next start. Only the first call writes; later ones do nothing.
    pub fn mark_at_risk(&self) {
        if self.at_risk.swap(true, Ordering::Relaxed) {
            return;
        }
        if let Err(error) = fs::write(&self.path, contents(self.started_at, self.pid, true)) {
            tracing::warn!(%error, "could not record that the session is at risk");
        }
    }

    /// Whether [`mark_at_risk`](Self::mark_at_risk) has been called.
    #[must_use]
    pub fn is_at_risk(&self) -> bool {
        self.at_risk.load(Ordering::Relaxed)
    }

    /// Removes the marker: this session ended cleanly.
    pub fn finish(&self) {
        if let Err(error) = fs::remove_file(&self.path) {
            tracing::warn!(%error, "could not remove the session marker");
        }
    }
}

fn parse(text: &str) -> PreviousSession {
    let value = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
            .map(str::trim)
    };
    let started_at = value("started_at").and_then(|v| v.parse().ok());
    let pid = value("pid").and_then(|v| v.parse().ok());
    // A marker without the `at_risk` line comes from a build that could only send dry runs. One
    // that says nothing at all is garbage, and nothing can be said about what it stood for.
    let readable = started_at.is_some() || pid.is_some();
    PreviousSession {
        started_at,
        pid,
        at_risk: !readable || value("at_risk") == Some("1"),
    }
}

#[cfg(test)]
mod tests {
    use wyck_engine::state::Warning;
    use wyck_engine::{Engine, EngineConfig};

    use super::*;

    fn state() -> EngineState {
        let config = EngineConfig {
            ..EngineConfig::default()
        };
        let engine = Engine::start(config).unwrap();
        let state = (*engine.handle().state()).clone();
        drop(engine);
        state
    }

    #[test]
    fn a_clean_exit_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let (marker, previous) = SessionMarker::begin(dir.path(), 1_000, 42).unwrap();
        assert_eq!(previous, None, "a first run has no previous session");
        assert!(dir.path().join(FILE_NAME).exists());
        marker.finish();
        assert!(!dir.path().join(FILE_NAME).exists());

        let (_marker, previous) = SessionMarker::begin(dir.path(), 2_000, 43).unwrap();
        assert_eq!(previous, None, "the last session finished cleanly");
    }

    #[test]
    fn a_session_killed_while_nothing_was_at_stake_is_not_an_alarm() {
        let dir = tempfile::tempdir().unwrap();
        let (marker, _) = SessionMarker::begin(dir.path(), 1_789_839_318_399, 4242).unwrap();
        drop(marker); // stopped from an editor: `finish` is never called
        let (_marker, previous) = SessionMarker::begin(dir.path(), 2_000, 43).unwrap();
        assert_eq!(
            previous,
            Some(PreviousSession {
                started_at: Some(1_789_839_318_399),
                pid: Some(4242),
                at_risk: false,
            })
        );
    }

    #[test]
    fn a_session_killed_after_it_was_at_risk_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let (marker, _) = SessionMarker::begin(dir.path(), 1_000, 4242).unwrap();
        assert!(!marker.is_at_risk());
        marker.mark_at_risk();
        marker.mark_at_risk();
        assert!(marker.is_at_risk());
        drop(marker);
        let (_marker, previous) = SessionMarker::begin(dir.path(), 2_000, 43).unwrap();
        let previous = previous.unwrap();
        assert!(previous.at_risk);
        assert_eq!(previous.started_at, Some(1_000), "the start time is kept");
        assert_eq!(previous.pid, Some(4242));
    }

    #[test]
    fn a_marker_written_by_an_older_build_was_never_at_risk() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(FILE_NAME),
            "started_at=1789848319968\npid=15940\n",
        )
        .unwrap();
        let (_marker, previous) = SessionMarker::begin(dir.path(), 1, 1).unwrap();
        assert_eq!(previous.map(|p| p.at_risk), Some(false));
    }

    #[test]
    fn an_unreadable_marker_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "garbage").unwrap();
        let (_marker, previous) = SessionMarker::begin(dir.path(), 1, 1).unwrap();
        assert_eq!(
            previous,
            Some(PreviousSession {
                started_at: None,
                pid: None,
                at_risk: true,
            })
        );
    }

    #[test]
    fn a_dry_run_engine_is_not_at_risk_and_an_armed_one_is() {
        let mut state = state();
        assert!(!at_risk(&state), "a fresh engine is in dry run");
        state.mode = TradingMode::Armed;
        assert!(at_risk(&state));
    }

    #[test]
    fn an_order_in_flight_or_an_unknown_order_is_a_risk() {
        let mut state = state();
        state.orders_in_flight.push("EURUSD".to_owned());
        assert!(at_risk(&state));

        let mut state = self::state();
        state.warnings.push(Warning {
            id: "unknown-order".to_owned(),
            kind: WarningKind::UnknownOrder,
            message: "outcome unknown".to_owned(),
            raised_at: 0,
        });
        assert!(at_risk(&state));

        let mut state = self::state();
        state.warnings.push(Warning {
            id: "news".to_owned(),
            kind: WarningKind::News,
            message: "release soon".to_owned(),
            raised_at: 0,
        });
        assert!(
            !at_risk(&state),
            "a news warning is not a risk of that kind"
        );
    }

    #[test]
    fn marking_a_session_at_risk_survives_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let (marker, _) = SessionMarker::begin(dir.path(), 5, 6).unwrap();
        marker.mark_at_risk();
        let text = fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert!(text.contains("at_risk=1"), "{text}");
        assert!(text.contains("started_at=5") && text.contains("pid=6"));
    }
}

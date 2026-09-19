//! Knowing that the last run ended badly.
//!
//! A trading app that crashes while orders are in flight leaves the user with a question: what
//! state is my account in? The marker answers the first half. At startup a small file is written
//! next to the logs; a clean exit removes it. If the file is still there at the next startup, the
//! last session was killed, crashed or lost power, and the front end says so and points at the
//! platform and the log.
//!
//! The marker deliberately says nothing about orders: it cannot know. It only makes sure the
//! user is asked to look.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// What is known about a session that did not end cleanly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousSession {
    /// When it started, in Unix milliseconds, if the marker could be read.
    pub started_at: Option<i64>,
    /// Its process id, if the marker could be read.
    pub pid: Option<u32>,
}

/// The marker of the running session. Call [`SessionMarker::finish`] on a clean exit.
#[derive(Debug)]
#[must_use = "call `finish` on a clean exit, or the next start reports a crash"]
pub struct SessionMarker {
    path: PathBuf,
}

const FILE_NAME: &str = "session.marker";

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
            }),
        };
        fs::write(&path, format!("started_at={started_at}\npid={pid}\n"))?;
        Ok((Self { path }, previous))
    }

    /// Removes the marker: this session ended cleanly.
    pub fn finish(self) {
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
    PreviousSession {
        started_at: value("started_at").and_then(|v| v.parse().ok()),
        pid: value("pid").and_then(|v| v.parse().ok()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_marker_left_behind_is_reported_with_what_it_says() {
        let dir = tempfile::tempdir().unwrap();
        let (marker, _) = SessionMarker::begin(dir.path(), 1_789_839_318_399, 4242).unwrap();
        drop(marker); // a crash: `finish` is never called
        let (_marker, previous) = SessionMarker::begin(dir.path(), 2_000, 43).unwrap();
        assert_eq!(
            previous,
            Some(PreviousSession {
                started_at: Some(1_789_839_318_399),
                pid: Some(4242)
            })
        );
    }

    #[test]
    fn an_unreadable_marker_still_counts_as_an_unclean_end() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "garbage").unwrap();
        let (_marker, previous) = SessionMarker::begin(dir.path(), 1, 1).unwrap();
        assert_eq!(
            previous,
            Some(PreviousSession {
                started_at: None,
                pid: None
            })
        );
    }
}

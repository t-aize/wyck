//! Logging for the application.
//!
//! The libraries only emit `tracing` events; the application chooses what to do with them.
//! This installs one subscriber that writes a daily rolling file (kept for two weeks) under the
//! data directory, filtered by `WYCK_LOG`, plus stderr in debug builds. It also installs a
//! panic hook, so a crash leaves a line in the log with its location and message.
//!
//! **No secret reaches a log line.** The engine's connection request and the settings of this
//! crate print `[REDACTED]` for a token in `Debug`, and a test below keeps it that way.

use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// How many daily files are kept.
const KEEP_DAYS: usize = 14;

/// Logging could not start.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// The filter directive is not valid.
    #[error("invalid log filter `{filter}`: {reason}")]
    Filter {
        /// What was given.
        filter: String,
        /// Why it failed.
        reason: String,
    },
    /// The log directory or file could not be created.
    #[error("could not create the log file in {dir}: {reason}")]
    File {
        /// The directory.
        dir: PathBuf,
        /// Why it failed.
        reason: String,
    },
}

/// Keeps the background writer alive. Drop it at exit to flush the last lines.
#[must_use = "dropping the guard stops file logging"]
pub struct LogGuard {
    _worker: WorkerGuard,
    dir: PathBuf,
}

impl LogGuard {
    /// The directory the log files are written to.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// Installs the global subscriber and the panic hook.
///
/// # Errors
///
/// [`LogError::Filter`] for an invalid directive, [`LogError::File`] when the file cannot be
/// created. A second call in the same process fails to install and is ignored.
pub fn init(dir: &Path, filter: &str) -> Result<LogGuard, LogError> {
    std::fs::create_dir_all(dir).map_err(|e| LogError::File {
        dir: dir.to_path_buf(),
        reason: e.to_string(),
    })?;
    let filter = EnvFilter::try_new(filter).map_err(|e| LogError::Filter {
        filter: filter.to_owned(),
        reason: e.to_string(),
    })?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("wyck")
        .filename_suffix("log")
        .max_log_files(KEEP_DAYS)
        .build(dir)
        .map_err(|e| LogError::File {
            dir: dir.to_path_buf(),
            reason: e.to_string(),
        })?;
    let (writer, worker) = tracing_appender::non_blocking(appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(writer);
    let stderr_layer = cfg!(debug_assertions)
        .then(|| tracing_subscriber::fmt::layer().with_writer(std::io::stderr));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        .try_init();

    install_panic_hook();
    Ok(LogGuard {
        _worker: worker,
        dir: dir.to_path_buf(),
    })
}

/// Logs a panic (location and message) before the default hook runs.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "the application panicked");
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use secrecy::SecretString;
    use wyck_engine::broker::{ConnectRequest, ServiceKind};

    use super::*;

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
        type Writer = Capture;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn a_token_never_reaches_a_log_line() {
        let capture = Capture::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(capture.clone())
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            let request = ConnectRequest::new(
                ServiceKind::CtraderRemote,
                "https://example.invalid/mcp",
                Some(SecretString::from("super-secret-token")),
            );
            tracing::info!(?request, "connecting");
            let settings = crate::settings::AppSettings::from_lookup(|n| {
                (n == "WYCK_TOKEN").then(|| "another-secret".to_owned())
            })
            .unwrap();
            tracing::info!(?settings, "settings");
        });
        let logged = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
        assert!(
            logged.contains("connecting"),
            "the event was logged: {logged}"
        );
        assert!(!logged.contains("super-secret-token"), "{logged}");
        assert!(!logged.contains("another-secret"), "{logged}");
    }

    #[test]
    fn an_invalid_filter_is_refused_with_the_directive() {
        let dir = tempfile::tempdir().unwrap();
        let Err(error) = init(dir.path(), "info,wyck_app=[") else {
            panic!("the filter must be refused");
        };
        assert!(matches!(error, LogError::Filter { .. }), "{error:?}");
    }

    #[test]
    fn the_log_directory_is_created_when_it_does_not_exist() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("logs");
        let guard = init(&nested, "info").unwrap();
        assert_eq!(guard.dir(), nested);
        assert!(nested.is_dir());
    }

    #[test]
    fn logging_starts_and_writes_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let guard = init(dir.path(), "info").unwrap();
        tracing::info!("hello from the test");
        drop(guard);
        let written = std::fs::read_dir(dir.path()).unwrap().count();
        assert!(written >= 1, "a rolling file is created");
    }
}

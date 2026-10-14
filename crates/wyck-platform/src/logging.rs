//! The log files of the app and the report written when it panics.
//!
//! A release build on Windows has no console, so what the app says on stderr is lost. This module
//! keeps a copy on disk, where a user can find it and send it with a bug report:
//!
//! * **A log file per day**, `wyck.YYYY-MM-DD.log`, in the folder that `AppPaths::logs_dir` (of
//!   `wyck-storage`) names. The last `KEPT_LOG_DAYS` days are kept, older files go when a new one
//!   is started. The file gets `info` for Wyck and `warn` for everything else, so a day stays
//!   small; `RUST_LOG` changes it, for the file and for stderr alike.
//! * **Writes off the interface thread.** The lines go through a channel to a worker thread, so a
//!   slow disk or a virus scanner never freezes a chart. When the channel is full lines are dropped,
//!   not waited for. [`flush`] writes out what is still waiting; call it when the app quits.
//! * **A report per panic**, `panic-YYYYMMDD-HHMMSS-<pid>-<n>.txt` in the same folder, written
//!   straight from the panic hook: the message, the place, the thread, the version and a backtrace
//!   (captured whatever `RUST_BACKTRACE` says). The last `KEPT_REPORTS` are kept. The report is
//!   written by the thread that panics, not by the worker, so it is there even when the process
//!   dies a moment later.
//!
//! Neither a log line nor a report carries a secret: the rest of the app never formats one, and a
//! panic message is the text of the code that panicked.

use std::backtrace::Backtrace;
use std::fs;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use tracing_appender::non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard};
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::build_info::{BuildMode, VERSION};

/// How many daily log files are kept. The library keeps one fewer than it is given, so this is
/// one more than the days wanted: today and the 13 before.
pub(crate) const KEPT_LOG_DAYS: usize = 14;
/// How many panic reports are kept.
pub(crate) const KEPT_REPORTS: usize = 10;

const LOG_PREFIX: &str = "wyck";
const LOG_SUFFIX: &str = "log";
const REPORT_PREFIX: &str = "panic-";
const REPORT_SUFFIX: &str = ".txt";

/// What stderr shows unless `RUST_LOG` says otherwise: the detail a developer wants.
const STDERR_DEFAULT: &str = "wyck=debug,warn";
/// What the file keeps unless `RUST_LOG` says otherwise: enough to follow a session, small enough
/// to send.
const FILE_DEFAULT: &str = "wyck=info,warn";

/// Keeps the worker of the log file alive. Taken out, and so dropped, by [`flush`].
static GUARD: Mutex<Option<WorkerGuard>> = Mutex::new(None);

/// Tells apart two reports written in the same second by one process.
static REPORTS_WRITTEN: AtomicU32 = AtomicU32::new(0);

/// Why the log file could not be opened. Logging then goes to stderr alone.
#[derive(Debug, thiserror::Error)]
enum OpenError {
    #[error("the log folder could not be created: {0}")]
    Folder(#[from] std::io::Error),
    #[error("the log file could not be opened: {0}")]
    File(#[from] tracing_appender::rolling::InitError),
}

fn filter(default: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default))
}

fn guard_slot() -> std::sync::MutexGuard<'static, Option<WorkerGuard>> {
    GUARD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Starts logging: to stderr, and to a file in `dir` when there is one and it can be opened.
/// Does nothing the second time.
pub fn init(dir: Option<&Path>) {
    let stderr = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_filter(filter(STDERR_DEFAULT));

    let mut open_error = None;
    let file = dir.and_then(|dir| match open_log_file(dir) {
        Ok((writer, guard)) => {
            *guard_slot() = Some(guard);
            Some(
                tracing_subscriber::fmt::layer()
                    .with_writer(writer)
                    .with_ansi(false)
                    .with_target(true)
                    .with_filter(filter(FILE_DEFAULT)),
            )
        }
        Err(error) => {
            open_error = Some(error);
            None
        }
    });

    let started = tracing_subscriber::registry()
        .with(stderr)
        .with(file)
        .try_init()
        .is_ok();
    if !started {
        return;
    }
    if let Some(error) = open_error {
        tracing::warn!(%error, "logging to stderr only");
    }
    tracing::info!(
        version = VERSION,
        build = BuildMode::CURRENT.label(),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        log_folder = ?dir,
        "Wyck started"
    );
}

/// Writes out the lines still waiting for the worker of the log file. Call it when the app quits:
/// a process that exits does not wait for the worker by itself.
pub fn flush() {
    let guard = guard_slot().take();
    drop(guard);
}

fn open_log_file(dir: &Path) -> Result<(NonBlocking, WorkerGuard), OpenError> {
    fs::create_dir_all(dir)?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_PREFIX)
        .filename_suffix(LOG_SUFFIX)
        .max_log_files(KEPT_LOG_DAYS)
        .build(dir)?;
    Ok(NonBlockingBuilder::default()
        .lossy(true)
        .thread_name("wyck-log")
        .finish(appender))
}

/// Writes a report in `dir` whenever the app panics, and logs it. The hook that was there before
/// (the one that prints the message on stderr) still runs afterwards.
///
/// Install it first thing, before a thread is started. The hook never fails: when the report
/// cannot be written the panic goes on as it would have.
pub fn install_panic_hook(dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let backtrace = Backtrace::force_capture().to_string();
        let path = record_panic(
            &dir,
            &info.to_string(),
            thread.name().unwrap_or("unnamed"),
            &backtrace,
        );
        tracing::error!(
            thread = thread.name().unwrap_or("unnamed"),
            panic_message = %message_of(info),
            location = %info.location().map(ToString::to_string).unwrap_or_default(),
            report = ?path,
            "the app panicked"
        );
        previous(info);
    }));
}

/// The text a panic carries, whatever type it was raised with.
fn message_of(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "(not a text)".to_owned()
    }
}

/// Writes the report of a panic into `dir` and trims the old ones. `None` when it could not be
/// written. Never panics.
fn record_panic(dir: &Path, summary: &str, thread: &str, backtrace: &str) -> Option<PathBuf> {
    let now = chrono::Local::now();
    let text = report_text(&now.to_rfc3339(), summary, thread, backtrace);
    // The counter is padded so that, within one second, the names still sort in the order the
    // reports were written.
    let name = format!(
        "{REPORT_PREFIX}{}-{}-{:05}{REPORT_SUFFIX}",
        now.format("%Y%m%d-%H%M%S"),
        std::process::id(),
        REPORTS_WRITTEN.fetch_add(1, Ordering::Relaxed)
    );
    fs::create_dir_all(dir).ok()?;
    let path = dir.join(name);
    fs::write(&path, text).ok()?;
    prune(dir, REPORT_PREFIX, REPORT_SUFFIX, KEPT_REPORTS);
    Some(path)
}

fn report_text(time: &str, summary: &str, thread: &str, backtrace: &str) -> String {
    format!(
        "Wyck panic report\n\
         Version: {VERSION} ({})\n\
         System: {} {}\n\
         Time: {time}\n\
         Thread: {thread}\n\
         \n\
         {summary}\n\
         \n\
         Backtrace:\n\
         {backtrace}\n",
        BuildMode::CURRENT.label(),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

/// The panic reports in `dir`, newest first.
pub fn reports(dir: &Path) -> Vec<PathBuf> {
    let mut found = matching(dir, REPORT_PREFIX, REPORT_SUFFIX);
    found.reverse();
    found
}

/// The files of `dir` whose name starts with `prefix` and ends with `suffix`, oldest first (the
/// names carry the date, so the name order is the time order).
fn matching(dir: &Path, prefix: &str, suffix: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix))
        })
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}

/// Removes the oldest files `matching` finds until `keep` are left. A file that cannot be removed
/// stays.
fn prune(dir: &Path, prefix: &str, suffix: &str, keep: usize) {
    let found = matching(dir, prefix, suffix);
    let extra = found.len().saturating_sub(keep);
    for path in found.into_iter().take(extra) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn lines_logged_reach_the_file_once_the_worker_is_flushed() {
        let dir = tempfile::tempdir().unwrap();
        let (writer, guard) = open_log_file(&dir.path().join("logs")).unwrap();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(writer)
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("a line for the file");
        });
        drop(guard);

        let files = matching(&dir.path().join("logs"), LOG_PREFIX, LOG_SUFFIX);
        assert_eq!(files.len(), 1, "one file for today");
        let text = fs::read_to_string(&files[0]).unwrap();
        assert!(text.contains("a line for the file"), "{text}");
        assert!(
            !text.contains("\u{1b}["),
            "no color codes in a file: {text:?}"
        );
    }

    #[test]
    fn old_log_files_go_when_a_new_one_is_started() {
        let dir = tempfile::tempdir().unwrap();
        for day in 1..=20 {
            fs::write(dir.path().join(format!("wyck.2020-01-{day:02}.log")), "old").unwrap();
        }
        fs::write(dir.path().join("notes.txt"), "not a log").unwrap();

        let opened = open_log_file(dir.path()).unwrap();
        drop(opened);

        // The library removes the oldest by the time each file was created, and these twenty were
        // made within the same instant: which ones go is not for this test to say. How many stay
        // is: the library keeps one fewer than the limit and then makes today's file, so the days
        // wanted are there, today included.
        let logs = matching(dir.path(), LOG_PREFIX, LOG_SUFFIX);
        assert_eq!(logs.len(), KEPT_LOG_DAYS, "{logs:?}");
        assert!(
            dir.path().join("notes.txt").exists(),
            "other files are left alone"
        );
    }

    #[test]
    fn a_folder_that_cannot_be_made_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        fs::write(&blocker, "x").unwrap();
        let result = open_log_file(&blocker.join("logs"));
        assert!(matches!(result, Err(OpenError::Folder(_))));
    }

    #[test]
    fn a_report_tells_where_what_and_when() {
        let text = report_text(
            "2026-10-12T10:00:00+02:00",
            "panicked at src/x.rs:1:2:\nboom",
            "main",
            "0: frame",
        );
        for wanted in [
            "Wyck panic report",
            &format!("Version: {VERSION}"),
            std::env::consts::OS,
            "Time: 2026-10-12T10:00:00+02:00",
            "Thread: main",
            "panicked at src/x.rs:1:2:\nboom",
            "Backtrace:\n0: frame",
        ] {
            assert!(text.contains(wanted), "missing {wanted:?} in {text}");
        }
    }

    #[test]
    fn a_panic_leaves_a_report_and_only_the_latest_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("logs");
        let mut written = Vec::new();
        for n in 0..(KEPT_REPORTS + 4) {
            let path = record_panic(&folder, &format!("panic {n}"), "worker", "trace").unwrap();
            assert!(path.is_file());
            written.push(path);
        }

        let kept = reports(&folder);
        assert_eq!(kept.len(), KEPT_REPORTS);
        assert_eq!(
            kept.first(),
            written.last(),
            "the newest comes first: {kept:?}"
        );
        assert!(!written[0].exists(), "the oldest went");
        let text = fs::read_to_string(&kept[0]).unwrap();
        assert!(
            text.contains("panic 13") && text.contains("Thread: worker"),
            "{text}"
        );
    }

    #[test]
    fn reports_are_only_the_panic_files_and_a_missing_folder_has_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(reports(&dir.path().join("nothing")).is_empty());
        fs::write(dir.path().join("wyck.2026-10-12.log"), "log").unwrap();
        fs::write(dir.path().join("panic-1.txt"), "report").unwrap();
        fs::write(dir.path().join("panic-1.txt.bad"), "other").unwrap();
        assert_eq!(names(dir.path()).len(), 3);
        assert_eq!(reports(dir.path()), vec![dir.path().join("panic-1.txt")]);
    }

    #[test]
    fn a_report_that_cannot_be_written_is_none_not_a_second_panic() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        fs::write(&blocker, "x").unwrap();
        assert_eq!(record_panic(&blocker.join("logs"), "s", "t", "b"), None);
    }

    #[test]
    fn the_hook_writes_a_report_for_a_real_panic() {
        // The hook is process-wide: install it, panic on a thread, put the old one back.
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("logs");
        let before = std::panic::take_hook();
        install_panic_hook(folder.clone());

        let outcome = std::thread::Builder::new()
            .name("doomed".to_owned())
            .spawn(|| {
                if std::hint::black_box(true) {
                    panic!("a test panic with a message");
                }
            })
            .unwrap()
            .join();
        let _ = std::panic::take_hook();
        std::panic::set_hook(before);

        assert!(outcome.is_err());
        // Another test that panics while the hook is in place leaves a report too: look for ours.
        let ours = reports(&folder)
            .into_iter()
            .filter_map(|path| fs::read_to_string(path).ok())
            .find(|text| text.contains("a test panic with a message"));
        let text = ours.expect("the panic left a report");
        assert!(text.contains("Thread: doomed"), "{text}");
        assert!(text.contains("Backtrace:"), "{text}");
    }
}

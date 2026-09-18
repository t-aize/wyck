//! File-based structured logging.
//!
//! `stdout` is owned by the terminal UI (raw mode + alternate screen), so a stray
//! `println!`/log line written there would corrupt the rendered frame. Every log line
//! from `wyck` and the crates it drives (`ctrader-mcp`, `wyck-config`) goes to a
//! daily-rotating file instead.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

/// The environment variable that overrides the default log filter, using
/// `tracing_subscriber`'s standard `EnvFilter` directive syntax (e.g.
/// `WYCK_LOG=wyck=debug,ctrader_mcp=trace`).
const LOG_FILTER_ENV_VAR: &str = "WYCK_LOG";

/// The default filter when [`LOG_FILTER_ENV_VAR`] isn't set: `info` for `wyck` and the
/// two in-workspace library crates, `warn` for everything else (dependency noise).
const DEFAULT_FILTER: &str = "warn,wyck=info,ctrader_mcp=info,wyck_config=info";

/// Initializes the global `tracing` subscriber to write to `{dir}/wyck.log`, rotated
/// daily.
///
/// # Why the returned guard must be kept alive
///
/// The writer is non-blocking: log calls hand their formatted line to a background
/// thread that does the actual file I/O, so a slow disk never stalls the caller (in
/// particular, never stalls the render loop). That background thread's channel is torn
/// down when the returned [`WorkerGuard`] drops — dropping it immediately after this
/// function returns would silently discard every buffered log line written before the
/// program's natural exit. Bind it to a variable that lives for the rest of `main`
/// (e.g. `let _log_guard = logging::init(...)?;`) and let it drop only when `main`
/// itself returns.
///
/// # Errors
///
/// Returns an [`std::io::Error`] if `dir` cannot be created.
pub fn init(dir: &Path) -> std::io::Result<WorkerGuard> {
    std::fs::create_dir_all(dir)?;

    let file_appender = tracing_appender::rolling::daily(dir, "wyck.log");
    let (writer, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_env(LOG_FILTER_ENV_VAR)
        .unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    tracing_subscriber::fmt()
        .with_writer(writer)
        // ANSI color codes in a log FILE just show up as garbage escape sequences —
        // they're only useful when the output is a color-capable terminal, which this
        // writer never is.
        .with_ansi(false)
        .with_env_filter(filter)
        .init();

    Ok(guard)
}

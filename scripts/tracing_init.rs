//! Installs a `tracing` subscriber that prints to stderr, so the events `wyck` emits (see the
//! "logging" sections of `src/openapi/mod.rs` and `src/config/mod.rs`) actually show up somewhere:
//! the library only emits them, it never installs a subscriber itself.
//!
//! Shared by every script here, and by `tests/live.rs` with `--nocapture`.

/// Installs the subscriber. Reads `RUST_LOG` for the level per module (e.g.
/// `RUST_LOG=wyck=trace,wyck::openapi::transport=debug`); without it, defaults to `debug` for
/// `wyck` and `warn` for everything else, which is already the most useful level for a script run
/// by hand: every request, retry, reconnect and credential-store operation, without the very high
/// volume `trace` spans (every rate-limited request, every heartbeat).
pub fn init() {
    use tracing_subscriber::EnvFilter;

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("wyck=debug,warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}

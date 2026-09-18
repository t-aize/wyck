//! `wyck` — a terminal trading panel for cTrader.
//!
//! # Shape of the program
//!
//! - [`wyck_config`] resolves OS-standard config/data directories, loads (or
//!   initializes) the app config, and holds the credential backend.
//! - [`engine`] owns the async [`ctrader_mcp`] connection on its own [`tokio::spawn`]ed
//!   task, talking to [`app::App`] over two `mpsc` channels ([`engine::EngineCommand`]
//!   in, [`engine::EngineEvent`] out) — see that module's doc comment for why.
//! - [`app`] is the render/input loop: draw the current [`ui::Screen`], then wait for
//!   whichever comes first, a terminal event or an engine event.
//!
//! Resolving the active profile's token (a synchronous [`wyck_config::secret::SecretStore`]
//! call) happens here in `main`, before the terminal or the async loop exist — see
//! [`resolve_initial_connect`].

mod app;
mod engine;
mod logging;
mod terminal;
mod ui;

use app::{App, InitialConnect};
use color_eyre::eyre::Result;
use engine::Engine;
use wyck_config::{AppPaths, KeyringSecretStore, WyckConfig};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let paths = AppPaths::discover()?;
    let _log_guard = logging::init(paths.data_dir())?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting wyck");

    // `KeyringSecretStore` is the recommended default backend (see its doc comment):
    // the OS owns key management entirely, so there is no passphrase to collect here.
    // Offering `EncryptedFileSecretStore` as a user-selectable alternative (for
    // environments with no OS keyring) is a follow-up — it needs a passphrase-entry
    // screen this first pass doesn't have yet.
    let config = WyckConfig::load(paths, Box::new(KeyringSecretStore::default()))?;
    let initial_connect = resolve_initial_connect(&config);

    let (engine, engine_commands, engine_events) = Engine::new();
    let engine_handle = tokio::spawn(engine.run());

    let app = App::new(config, engine_commands, initial_connect);

    // Scoped so `_terminal_guard` drops (restoring the terminal) before `main` returns
    // — including on an early `Err` — rather than only at the very end of `main`, so
    // `color_eyre`'s error report (if any) prints to a normal, restored terminal.
    let app_result = {
        let (_terminal_guard, mut terminal) = terminal::TerminalGuard::enter()?;
        app.run(&mut terminal, engine_events).await
    };

    // The app loop has exited; the engine's next tick or command-recv would just spin
    // forever with nothing left to serve. Aborting is safe here — it isn't holding a
    // resource that needs a graceful async shutdown sequence.
    engine_handle.abort();

    app_result
}

/// Resolves the active profile's connection details before the terminal or the async
/// event loop start. This is a deliberate one-shot synchronous cost at startup:
/// [`wyck_config::secret::SecretStore`]'s backends are synchronous by design (an OS
/// keyring lookup is an inherently blocking call), so doing this once here — rather
/// than from inside the async render loop — avoids ever blocking that loop on it.
///
/// Returns `None` (falling back to the first-run screen) if there is no active
/// profile, the profile has no endpoint configured, or the token can't be resolved for
/// any reason — every failure path is logged, none of them are fatal to startup.
fn resolve_initial_connect(config: &WyckConfig) -> Option<InitialConnect> {
    let profile = config.active_profile()?;
    let Some(endpoint) = profile.endpoint.clone() else {
        tracing::warn!(profile = %profile.id, "active profile has no endpoint configured; showing the first-run screen");
        return None;
    };

    match config.token_for(&profile.id) {
        Ok(Some(token)) => Some(InitialConnect {
            display_name: profile.display_name.clone(),
            endpoint,
            token,
        }),
        Ok(None) => {
            tracing::warn!(profile = %profile.id, "active profile has no stored token; showing the first-run screen");
            None
        }
        Err(source) => {
            tracing::warn!(profile = %profile.id, error = %source, "failed to resolve the active profile's token");
            None
        }
    }
}

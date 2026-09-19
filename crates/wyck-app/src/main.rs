//! `wyck-app`: the application. `cargo run` opens the window.
//!
//! This file only wires things together: it reads the settings, starts logging, the crash marker
//! and the engine, builds the connection request, and hands over to [`wyck_app::shell`]. The
//! window's content is [`BlankView`](wyck_app::shell::BlankView) until a real view is plugged in
//! where `shell::run` is called.

#![forbid(unsafe_code)]
// A release build is a window, not a console program. Logs go to a file.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::process::ExitCode;
use std::sync::Arc;

use gpui_kit::AppContext as _;
use wyck_app::controller::AppController;
use wyck_app::logging;
use wyck_app::messages::{Level, Notice};
use wyck_app::session_marker::SessionMarker;
use wyck_app::settings::{AppSettings, ConnectionChoice};
use wyck_app::shell::{self, AppArgs, BlankView, Hooks};
use wyck_app::startup::{StartupError, connect_request, open_user_config};
use wyck_config::AppPaths;
use wyck_engine::domain::now_millis;

fn main() -> ExitCode {
    let mut settings = match AppSettings::from_env() {
        Ok(settings) => settings,
        Err(error) => {
            // No window exists yet, so the console is the only place to say it.
            eprintln!("wyck-app: invalid setting: {error}");
            return ExitCode::from(2);
        }
    };

    let data_dir = match AppPaths::discover() {
        Ok(paths) => Some(paths.data_dir().to_path_buf()),
        Err(error) => {
            eprintln!("wyck-app: no data directory ({error}); logging and crash detection are off");
            None
        }
    };
    let _log = data_dir.as_ref().and_then(|dir| {
        logging::init(&dir.join("logs"), &settings.log_filter)
            .map_err(|error| eprintln!("wyck-app: logging is off: {error}"))
            .ok()
    });
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "wyck-app starting");

    let mut banners = Vec::new();
    let marker = data_dir.as_ref().and_then(|dir| {
        match SessionMarker::begin(dir, now_millis(), std::process::id()) {
            Ok((marker, previous)) => {
                if let Some(previous) = previous {
                    tracing::warn!(?previous, "the previous session did not end cleanly");
                    banners.push(Notice {
                        level: Level::Warning,
                        title: "The last session did not end cleanly".to_owned(),
                        detail: Some("The application crashed or was killed. An order may have been in flight.".to_owned()),
                        hint: Some("Check your positions and orders in the platform before trading."),
                    });
                }
                Some(marker)
            }
            Err(error) => {
                tracing::warn!(%error, "crash detection is off");
                None
            }
        }
    });

    let controller = match AppController::start(&settings) {
        Ok(controller) => Arc::new(controller),
        Err(error) => {
            tracing::error!(%error, "the engine could not start");
            eprintln!("wyck-app: the engine could not start: {error}");
            return ExitCode::FAILURE;
        }
    };

    let choice = std::mem::replace(&mut settings.connection, ConnectionChoice::ActiveProfile);
    let connection = connect_request(choice, open_user_config);
    if let Err(StartupError::Config(reason)) = &connection {
        tracing::warn!(%reason, "the configuration could not be read");
    }

    shell::run(
        AppArgs {
            settings,
            controller,
            connection,
            banners,
        },
        Hooks::default(),
        |_shell, _window, cx| cx.new(|_| BlankView),
    );

    tracing::info!("wyck-app stopped");
    if let Some(marker) = marker {
        marker.finish();
    }
    ExitCode::SUCCESS
}

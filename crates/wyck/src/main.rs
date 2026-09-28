//! Entry point for the `wyck` desktop application: the window and the connection flow.

#[path = "services/alerts.rs"]
mod alerts;
mod appearance;
mod assets;
#[path = "services/backup.rs"]
mod backup;
mod build_info;
mod chart;
mod connection;
mod dashboard;
mod indicators;
mod multichart;
mod runtime;
mod settings_hub;
mod title_bar;
#[path = "services/token_store.rs"]
mod token_store;
mod trading;
#[path = "services/updates.rs"]
mod updates;
mod workspace;

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{App, Bounds, WindowBounds, px, size};
use gpui_kit::component::{Root, TitleBar};

/// Opens the app window and runs the event loop. Returns when the app quits.
fn main() {
    init_tracing();

    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            updates::init(cx);
            // The saved look is in force before a window opens, so the first frame is the right one.
            match wyck_config::AppPaths::discover() {
                Ok(paths) => {
                    // An import the user asked for waits for this moment, before anything reads the
                    // documents it replaces.
                    match backup::apply_pending_reset(paths.config_dir()) {
                        Ok(true) => tracing::info!(
                            "reset the look, layout, charts and every account's data"
                        ),
                        Ok(false) => {}
                        Err(error) => tracing::warn!(%error, "could not apply the pending reset"),
                    }
                    let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string();
                    match backup::apply_pending(paths.config_dir(), &stamp) {
                        Ok(Some(applied)) => tracing::info!(?applied, "restored a backup"),
                        Ok(None) => {}
                        Err(error) => tracing::warn!(%error, "could not restore the backup"),
                    }
                    appearance::init(wyck_config::DocumentStore::global(&paths), cx);
                    indicators::init(Some(&paths), cx);
                }
                Err(_) => {
                    wyck_ui::theme::apply(cx);
                    indicators::init(None, cx);
                }
            }
            wyck_ui::text_input::init(cx);
            dashboard::init(cx);
            chart::init(cx);
            indicators::editor::init(cx);
            wyck_ui::modal::init(cx);

            cx.text_system()
                .add_fonts(vec![Cow::Borrowed(assets::FONT)])
                .expect("the bundled Inter font failed to load");

            cx.on_window_closed(|cx, _window_id| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            // Most of the screen: a trading terminal wants the room. Never smaller than what
            // the dashboard is laid out for.
            let screen = cx
                .primary_display()
                .map(|display| display.bounds().size)
                .map_or((1180.0, 800.0), |size| {
                    (f32::from(size.width), f32::from(size.height))
                });
            let (width, height) = (
                (screen.0 * 0.9).clamp(1180.0, 2400.0),
                (screen.1 * 0.9).clamp(800.0, 1500.0),
            );
            let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
            let mut options = TitleBar::window_options();
            options.window_bounds = Some(WindowBounds::Windowed(bounds));
            if let Some(titlebar) = options.titlebar.as_mut() {
                titlebar.title = Some("Wyck".into());
            }
            #[cfg(target_os = "linux")]
            {
                options.window_decorations = Some(gpui::WindowDecorations::Client);
            }
            cx.open_window(options, |window, cx| {
                // With the mode on System, the theme follows the system as it changes.
                window
                    .observe_window_appearance(|_window, cx| appearance::refresh_system(cx))
                    .detach();
                let flow = cx.new(connection::ConnectionFlow::new);
                cx.new(|cx| Root::new(flow, window, cx))
            })
            .expect("failed to open the main window");

            updates::check(cx, true);

            cx.activate(true);
        });
}

/// The settings folder, or `None` when the system gives the app none.
fn config_dir() -> Option<std::path::PathBuf> {
    wyck_config::AppPaths::discover()
        .ok()
        .map(|paths| paths.config_dir().to_path_buf())
}

/// Installs a `tracing` subscriber so the events `wyck_config` and `wyck_openapi` emit (and
/// the app's own) show up on stderr; the library only emits them, it never installs a subscriber
/// itself. Reads `RUST_LOG`, defaulting to `debug` for `wyck` and `warn` for everything else.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("wyck=debug,warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}

//! Wyck, a desktop trading terminal for cTrader: the window, the connection flow and every screen.

#[path = "services/alerts/mod.rs"]
mod alerts;
mod appearance;
mod assets;
mod build_info;
mod chart;
pub mod chart_core;
mod connection;
mod dashboard;
mod indicators;
#[cfg(test)]
mod keymap_guard;
mod multichart;
mod runtime;
mod settings_hub;
mod title_bar;
#[path = "services/token_store.rs"]
mod token_store;
mod trading;
// Public while the merged crates are cleaned up: unused items would otherwise fail clippy.
pub mod ui;
#[path = "services/updates.rs"]
mod updates;
mod workspace;

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{App, Bounds, WindowBounds, px, size};
use gpui_kit::component::{Root, TitleBar};

/// Opens the app window and runs the event loop. Returns when the app quits.
pub fn run() {
    init_tracing();

    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            updates::init(cx);
            // The saved look is in force before a window opens, so the first frame is the right one.
            match app_paths() {
                Some(paths) => {
                    // A reset or an import the user asked for waits for this moment, before
                    // anything reads the documents it replaces.
                    let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string();
                    match wyck_config::backup::apply_pending(paths, &stamp) {
                        Ok(applied) if applied != Default::default() => {
                            tracing::info!(?applied, "applied what was waiting for this start");
                        }
                        Ok(_) => {}
                        Err(error) => tracing::warn!(%error, "could not apply what was waiting"),
                    }
                    appearance::init(paths.documents(), cx);
                    indicators::init(Some(paths), cx);
                    keep_a_daily_copy(paths, cx);
                }
                None => {
                    crate::ui::kit::theme::apply(cx);
                    indicators::init(None, cx);
                }
            }
            crate::ui::kit::text_input::init(cx);
            dashboard::init(cx);
            chart::init(cx);
            indicators::editor::init(cx);
            crate::ui::kit::modal::init(cx);

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
                // The size of the interface the user chose, for what is sized in rems.
                window.set_rem_size(gpui::px(16.0 * appearance::get(cx).ui_scale as f32 / 100.0));
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

/// Saves an automatic copy of everything the user made, at most one a day and the last few kept,
/// off the interface thread: what a bad import, a reset or a broken disk cannot take away.
fn keep_a_daily_copy(paths: &'static wyck_config::AppPaths, cx: &mut App) {
    let scripts = wyck_config::scripts::ScriptStore::new(indicators::dir(cx));
    cx.background_executor()
        .spawn(async move {
            let created = chrono::Local::now().to_rfc3339();
            let policy = wyck_config::backup::AutoPolicy::default();
            match paths.backups().auto_snapshot(
                Some(&scripts),
                build_info::VERSION,
                &created,
                policy,
            ) {
                Ok(Some(entry)) => tracing::info!(id = entry.id, "saved an automatic backup"),
                Ok(None) => {}
                Err(error) => tracing::warn!(%error, "could not save an automatic backup"),
            }
        })
        .detach();
}

/// The folders of the app, resolved once: `None` when the system gives it none. Everything that
/// needs a path of the app asks here, so there is one answer for the whole run.
fn app_paths() -> Option<&'static wyck_config::AppPaths> {
    static PATHS: std::sync::OnceLock<Option<wyck_config::AppPaths>> = std::sync::OnceLock::new();
    PATHS
        .get_or_init(|| wyck_config::AppPaths::discover().ok())
        .as_ref()
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

//! Wyck, a desktop trading terminal for cTrader: the window, the connection flow and every screen.

// The interface modules are public for now and document private helpers.
#![allow(rustdoc::private_intra_doc_links)]

pub mod app;
pub mod domain;
pub mod infra;
#[cfg(test)]
mod keymap_guard;
pub mod ui;

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{App, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use gpui_kit::component::Root;

use app::appearance;
use app::scripts;
use app::updates;
use infra::platform::build_info::{self, BuildMode};
use ui::assets;
use ui::features::{chart, indicators};
use ui::shell::{connection, dashboard};

/// The smallest the window can be made: below this the dashboard has no room for its bars.
const MIN_WINDOW: (f32, f32) = (900.0, 600.0);

/// The identifier desktops use to group the windows of the app and find its icon.
const APP_ID: &str = "sh.wyck.wyck";

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
                    match crate::infra::storage::backup::apply_pending(paths, &stamp) {
                        Ok(applied) if applied != Default::default() => {
                            tracing::info!(?applied, "applied what was waiting for this start");
                        }
                        Ok(_) => {}
                        Err(error) => tracing::warn!(%error, "could not apply what was waiting"),
                    }
                    appearance::init(paths.documents(), cx);
                    scripts::init(Some(paths), cx);
                    keep_a_daily_copy(paths, cx);
                }
                None => {
                    crate::ui::kit::theme::apply(cx);
                    scripts::init(None, cx);
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
            // The system draws the title bar and its buttons.
            let title = if BuildMode::CURRENT == BuildMode::Development {
                "Wyck (dev)"
            } else {
                "Wyck"
            };
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                window_min_size: Some(size(px(MIN_WINDOW.0), px(MIN_WINDOW.1))),
                app_id: Some(APP_ID.to_owned()),
                ..Default::default()
            };
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
fn keep_a_daily_copy(paths: &'static crate::infra::storage::AppPaths, cx: &mut App) {
    let scripts = crate::infra::storage::scripts::ScriptStore::new(scripts::dir(cx));
    cx.background_executor()
        .spawn(async move {
            let created = chrono::Local::now().to_rfc3339();
            let policy = crate::infra::storage::backup::AutoPolicy::default();
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
fn app_paths() -> Option<&'static crate::infra::storage::AppPaths> {
    static PATHS: std::sync::OnceLock<Option<crate::infra::storage::AppPaths>> =
        std::sync::OnceLock::new();
    PATHS
        .get_or_init(|| crate::infra::storage::AppPaths::discover().ok())
        .as_ref()
}

/// Installs a `tracing` subscriber so the events `infra::storage` and `infra::ctrader` emit (and
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

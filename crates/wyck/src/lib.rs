//! Wyck, a desktop trading terminal for cTrader: the window, the connection flow and every screen.

// The interface modules are public for now and document private helpers.
#![allow(rustdoc::private_intra_doc_links)]

pub mod app;
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
use ui::assets;
use ui::features::indicators;
use ui::shell::app_view::AppView;
use wyck_platform::build_info::{self, BuildMode};
use wyck_platform::logging;

/// The smallest the window can be made: below this the dashboard has no room for its bars.
const MIN_WINDOW: (f32, f32) = (900.0, 600.0);

/// The identifier desktops use to group the windows of the app and find its icon.
const APP_ID: &str = "sh.wyck.wyck";

/// Opens the app window and runs the event loop. Returns when the app quits.
pub fn run() {
    // The first thing, so that whatever goes wrong afterwards is on disk: the events
    // `wyck_storage` and `wyck_ctrader` emit (and the app's own) go to stderr and to a log
    // file, and a panic leaves a report next to it. The library only emits events, it never
    // installs a subscriber itself.
    let logs = app_paths().map(|paths| paths.logs_dir());
    logging::init(logs);
    app::locations::log_startup();
    if let Some(dir) = logs {
        logging::install_panic_hook(dir.to_path_buf());
    } else {
        tracing::warn!("this system gives the app no folder: nothing is saved, logs go to stderr");
    }

    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            // A process that exits does not wait for the log worker: write out what it holds.
            cx.on_app_quit(|_| async { logging::flush() }).detach();
            updates::init(cx);
            // The saved look is in force before a window opens, so the first frame is the right one.
            match app_paths() {
                Some(paths) => {
                    // A reset or an import the user asked for waits for this moment, before
                    // anything reads the documents it replaces.
                    let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string();
                    match wyck_storage::backup::staged::apply_pending(paths, &stamp) {
                        Ok(applied) if applied != Default::default() => {
                            tracing::info!(?applied, "applied what was waiting for this start");
                        }
                        Ok(_) => {}
                        Err(error) => tracing::warn!(%error, "could not apply what was waiting"),
                    }
                    // Documents saved by an older version are brought to this one now, whatever
                    // the distance, before anything reads them.
                    let migrated = app::migrations::run(paths);
                    if !migrated.is_empty() {
                        tracing::info!(?migrated, "brought the saved documents to this version");
                    }
                    appearance::init(paths.documents(), ui::kit::theme::put_in_force, cx);
                    scripts::init(Some(paths), cx);
                    keep_a_daily_copy(paths, cx);
                }
                None => {
                    crate::ui::kit::theme::apply(cx);
                    scripts::init(None, cx);
                }
            }
            indicators::editor::init(cx);
            crate::ui::shell::keymap::apply(&Default::default(), cx);

            // Without the bundled font the system's own is used: the app is still usable.
            if let Err(error) = cx
                .text_system()
                .add_fonts(vec![Cow::Borrowed(assets::FONT)])
            {
                tracing::error!(%error, "the bundled Inter font failed to load");
            }

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
            let opened = cx.open_window(options, |window, cx| {
                // The size of the interface the user chose, for what is sized in rems.
                window.set_rem_size(gpui::px(16.0 * appearance::get(cx).ui_scale as f32 / 100.0));
                let shell = cx.new(|cx| AppView::new(window, cx));
                cx.new(|cx| Root::new(shell, window, cx))
            });
            // An app with no window has nothing to show: say why in the log and leave.
            if let Err(error) = opened {
                tracing::error!(%error, "could not open the main window");
                cx.quit();
                return;
            }

            updates::check(cx, Some(ui::shell::settings_hub::announce_update));
            ui::shell::settings_hub::announce_folders(cx);

            cx.activate(true);
        });
    logging::flush();
}

/// Saves an automatic copy of everything the user made, at most one a day and the last few kept,
/// off the interface thread: what a bad import, a reset or a broken disk cannot take away.
fn keep_a_daily_copy(paths: &'static wyck_storage::AppPaths, cx: &mut App) {
    let scripts = wyck_storage::scripts::ScriptStore::new(scripts::dir(cx));
    cx.background_executor()
        .spawn(async move {
            let created = chrono::Local::now().to_rfc3339();
            let policy = wyck_storage::backup::store::AutoPolicy::default();
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
///
/// The first call also makes the move of folders the user asked for (it has to be done before
/// anything reads a document), which is why the first call is the first thing `run` does.
fn app_paths() -> Option<&'static wyck_storage::AppPaths> {
    static PATHS: std::sync::OnceLock<Option<wyck_storage::AppPaths>> = std::sync::OnceLock::new();
    PATHS.get_or_init(app::locations::prepare).as_ref()
}

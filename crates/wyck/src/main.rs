//! Wyck, a desktop trading terminal for cTrader: the program that starts the app and opens its
//! window. Every screen is in `wyck-ui` and everything else is a library crate of the
//! workspace; this file only puts them together.

// A release build on Windows is a window app: without this the system opens a console beside it.
// A debug build keeps the console, where the logs are printed.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{App, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use wyck_app::appearance;
use wyck_app::scripts;
use wyck_app::shortcuts::ShortcutPrefs;
use wyck_app::updates;
use wyck_core::math::u32_to_f32;
use wyck_platform::build_info::{self, BuildMode};
use wyck_platform::logging;
use wyck_storage::AppPaths;
use wyck_storage::backup::staged::{self, Applied};
use wyck_ui::kit::assets;
use wyck_ui::shell::app_view::AppView;

/// The smallest the window can be made: below this the dashboard has no room for its bars.
const MIN_WINDOW: (f32, f32) = (900.0, 600.0);

/// The identifier desktops use to group the windows of the app and find its icon.
const APP_ID: &str = "sh.wyck.wyck";

fn main() {
    start_logging();
    wyck_ui::kit::application()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            wyck_ui::kit::init(cx);
            // A process that exits does not wait for the log worker: write out what it holds.
            cx.on_app_quit(|_| async { logging::flush() }).detach();
            updates::init(cx);
            // The saved look is in force before a window opens, so the first frame is the right one.
            prepare_saved_data(cx);
            wyck_ui::editor::init(cx);
            wyck_ui::shell::keymap::apply(&ShortcutPrefs::default(), cx);
            load_font(cx);
            cx.on_window_closed(|cx, _window_id| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            // An app with no window has nothing to show: leave.
            if !open_main_window(cx) {
                cx.quit();
                return;
            }
            updates::check(cx, Some(wyck_ui::shell::settings_hub::announce_update));
            wyck_ui::shell::settings_hub::announce_folders(cx);
            cx.activate(true);
        });
    logging::flush();
}

/// The first thing, so that whatever goes wrong afterwards is on disk: the events `wyck_storage`
/// and `wyck_ctrader` emit (and the app's own) go to stderr and to a log file, and a panic leaves a
/// report next to it. The libraries only emit events, they never install a subscriber themselves.
fn start_logging() {
    let logs = wyck_app::app_paths().map(AppPaths::logs_dir);
    logging::init(logs);
    wyck_app::locations::log_startup();
    if let Some(dir) = logs {
        logging::install_panic_hook(dir.to_path_buf());
    } else {
        tracing::warn!("this system gives the app no folder: nothing is saved, logs go to stderr");
    }
}

/// Applies what waited for this start, brings the saved documents to this version and puts the
/// saved look and the scripts in force. Without a folder for the app, the default look is used.
fn prepare_saved_data(cx: &mut App) {
    let Some(paths) = wyck_app::app_paths() else {
        wyck_ui::kit::theme::apply(cx);
        scripts::init(None, cx);
        return;
    };
    // A reset or an import the user asked for waits for this moment, before anything reads the
    // documents it replaces.
    let stamp = chrono::DateTime::<chrono::Local>::from(wyck_clock::now())
        .format("%Y-%m-%d-%H%M%S")
        .to_string();
    match staged::apply_pending(paths, &stamp) {
        Ok(applied) if applied != Applied::default() => {
            tracing::info!(?applied, "applied what was waiting for this start");
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "could not apply what was waiting"),
    }
    // Documents saved by an older version are brought to this one now, whatever the distance,
    // before anything reads them.
    let migrated = wyck_app::migrations::run(paths);
    if !migrated.is_empty() {
        tracing::info!(?migrated, "brought the saved documents to this version");
    }
    appearance::init(paths.documents(), wyck_ui::kit::theme::put_in_force, cx);
    scripts::init(Some(paths), cx);
    keep_a_daily_copy(paths, cx);
}

/// Without the bundled font the system's own is used: the app is still usable.
fn load_font(cx: &mut App) {
    if let Err(error) = cx
        .text_system()
        .add_fonts(vec![Cow::Borrowed(assets::FONT)])
    {
        tracing::error!(%error, "the bundled Inter font failed to load");
    }
}

/// Opens the window: most of the screen, because a trading terminal wants the room, and never
/// smaller than what the dashboard is laid out for. Says why in the log when it cannot.
fn open_main_window(cx: &mut App) -> bool {
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
        window.set_rem_size(px(16.0 * u32_to_f32(appearance::get(cx).ui_scale) / 100.0));
        let shell = cx.new(|cx| AppView::new(window, cx));
        cx.new(|cx| wyck_ui::kit::Root::new(shell, window, cx))
    });
    if let Err(error) = &opened {
        tracing::error!(%error, "could not open the main window");
    }
    opened.is_ok()
}

/// Saves an automatic copy of everything the user made, at most one a day and the last few kept,
/// off the interface thread: what a bad import, a reset or a broken disk cannot take away.
fn keep_a_daily_copy(paths: &'static AppPaths, cx: &mut App) {
    let scripts = wyck_storage::scripts::ScriptStore::new(scripts::dir(cx));
    cx.background_executor()
        .spawn(async move {
            let created = chrono::DateTime::<chrono::Local>::from(wyck_clock::now()).to_rfc3339();
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

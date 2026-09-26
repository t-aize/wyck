//! The `wyck` desktop application: window and the connection flow.

#[path = "services/alerts.rs"]
mod alerts;
#[path = "ui/anim.rs"]
mod anim;
mod appearance;
mod assets;
#[path = "services/backup.rs"]
mod backup;
mod chart;
#[path = "ui/color_picker.rs"]
mod color_picker;
#[path = "ui/confirm.rs"]
mod confirm;
mod connection;
mod dashboard;
mod indicators;
#[path = "ui/menu.rs"]
mod menu;
#[path = "ui/modal.rs"]
mod modal;
mod multichart;
mod runtime;
#[path = "ui/settings_hub.rs"]
mod settings_hub;
#[path = "ui/settings_ui.rs"]
mod settings_ui;
#[path = "ui/text_input.rs"]
mod text_input;
mod theme;
#[path = "ui/toast.rs"]
mod toast;
#[path = "services/token_store.rs"]
mod token_store;
mod trading;
#[path = "ui/widgets.rs"]
mod widgets;
mod workspace;

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{App, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use gpui_kit::component::Root;

/// Opens the app window and runs the event loop. Returns when the app quits.
pub fn run() {
    init_tracing();

    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            // The saved look is in force before a window opens, so the first frame is the right one.
            match wyck_config::AppPaths::discover() {
                Ok(paths) => {
                    // An import the user asked for waits for this moment, before anything reads the
                    // documents it replaces.
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
                    theme::apply(cx);
                    indicators::init(None, cx);
                }
            }
            text_input::init(cx);
            dashboard::init(cx);
            chart::init(cx);
            indicators::editor::init(cx);
            modal::init(cx);

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
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    // The OS's own titlebar: real Windows caption buttons and Snap Layouts, real
                    // macOS traffic lights, whatever the Linux compositor draws for everyone else.
                    titlebar: Some(TitlebarOptions {
                        title: Some("Wyck".into()),
                        appears_transparent: false,
                        traffic_light_position: None,
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    // With the mode on System, the theme follows the system as it changes.
                    window
                        .observe_window_appearance(|_window, cx| appearance::refresh_system(cx))
                        .detach();
                    let flow = cx.new(connection::ConnectionFlow::new);
                    cx.new(|cx| Root::new(flow, window, cx))
                },
            )
            .expect("failed to open the main window");

            cx.activate(true);
        });
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

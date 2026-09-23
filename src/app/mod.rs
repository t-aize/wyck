//! The `wyck` desktop application: window and the connection flow.

mod connection;
mod runtime;
mod text_input;
mod theme;

use gpui::prelude::*;
use gpui::{App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

/// Opens the app window and runs the event loop. Returns when the app quits.
pub fn run() {
    init_tracing();

    Application::new().run(|cx: &mut App| {
        text_input::init(cx);

        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(920.0), px(640.0)), cx);
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
            |_window, cx| cx.new(connection::ConnectionFlow::new),
        )
        .expect("failed to open the main window");

        cx.activate(true);
    });
}

/// Installs a `tracing` subscriber so the events `wyck::config` and `wyck::openapi` emit (and
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

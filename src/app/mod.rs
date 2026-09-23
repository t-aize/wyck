//! The `wyck` desktop application: window and the connection flow.

mod alerts;
mod anim;
mod assets;
mod chart;
mod connection;
mod dashboard;
mod multichart;
mod runtime;
mod text_input;
mod theme;
mod toast;
mod token_store;
mod trading;
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
            theme::apply(cx);
            text_input::init(cx);
            dashboard::init(cx);
            chart::init(cx);

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
                    let flow = cx.new(connection::ConnectionFlow::new);
                    cx.new(|cx| Root::new(flow, window, cx))
                },
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

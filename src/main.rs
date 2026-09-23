//! Entry point for the `wyck` desktop application: a GPUI window shell.
//!
//! This is a starting point, not a finished panel: one window, one root view. The `openapi`
//! and `config` modules from the `wyck` library are what this will eventually drive; wiring
//! them in comes later, once there's a view worth feeding data to.

use gpui::{
    App, Application, Bounds, Context, KeyBinding, Render, SharedString, TitlebarOptions, Window,
    WindowBounds, WindowOptions, actions, div, prelude::*, px, rgb, size,
};

actions!(wyck, [Quit]);

struct WyckApp {
    status: SharedString,
}

impl Render for WyckApp {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .on_action(|_: &Quit, window, _cx| window.remove_window())
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xd4d4d4))
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_4()
                    .h(px(36.0))
                    .bg(rgb(0x161616))
                    .border_b_1()
                    .border_color(rgb(0x2d2d2d))
                    .text_sm()
                    .child("wyck"),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .child(self.status.clone()),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(900.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("wyck".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| {
                cx.new(|_cx| WyckApp {
                    status: "cTrader connection: not started".into(),
                })
            },
        )
        .expect("failed to open the main window");

        cx.activate(true);
    });
}

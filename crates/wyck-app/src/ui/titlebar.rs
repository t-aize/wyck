//! The title bar, drawn by the application instead of the operating system.
//!
//! It is 38 pixels high, at any interface scale (only the window and its content grow): the logo and the name on the left, the version on the right, then the
//! minimize, maximize (or restore) and close buttons. The window is created without a native
//! title bar (see [`window_options`]), so this bar is all there is, and it does three jobs a
//! native one would:
//!
//! - **Dragging and double click.** The bar is a `Drag` control area. On Windows the system then
//!   moves the window and maximizes it on a double click, with snap layouts and edge snapping
//!   intact. On Linux the bar starts the move itself, and maximizes on a double click.
//! - **The three buttons.** On Windows each is a `Min`, `Max` or `Close` control area, so the
//!   system does the work and the buttons behave exactly like native ones (including the snap
//!   layout flyout on the maximize button). On Linux they call the window methods on click.
//! - **macOS** keeps the system's traffic lights: the bar leaves room for them and draws no
//!   buttons of its own.

use gpui_kit::App;
use gpui_kit::prelude::*;
use gpui_kit::{
    Bounds, Div, ElementId, FontWeight, MouseButton, Pixels, Stateful, TitlebarOptions, Window,
    WindowBounds, WindowControlArea, WindowOptions, div, point, px, size, svg,
};

use super::motion::Hover;
use super::theme::{self, sz};

/// The height of the bar.
pub const HEIGHT: f32 = 38.;
/// The width of each window button.
const BUTTON_WIDTH: f32 = 44.;
/// Room for the traffic lights on macOS.
const MAC_INSET: f32 = 80.;

/// The size the main window opens at.
pub const DEFAULT_SIZE: (f32, f32) = (960., 720.);
/// The smallest the main window can be made.
pub const MIN_SIZE: (f32, f32) = (800., 560.);

/// A window size given in design pixels, scaled (the window is larger with the content, the bar is not), and cut to 92 percent of `available` (the screen, in
/// logical pixels) when that is known and smaller.
#[must_use]
pub fn fitted(design: (f32, f32), available: Option<(f32, f32)>) -> (f32, f32) {
    let (width, height) = (f32::from(sz(design.0)), f32::from(sz(design.1)));
    match available {
        Some((w, h)) => (width.min(w * 0.92), height.min(h * 0.92)),
        None => (width, height),
    }
}

/// The options of a window that draws its own title bar: no native bar, a size, a minimum size.
#[must_use]
pub fn window_options(cx: &App) -> WindowOptions {
    // The screen the window opens on, in logical pixels: the window never opens larger than it.
    let available = cx.primary_display().map(|display| {
        let screen = display.bounds().size;
        (f32::from(screen.width), f32::from(screen.height))
    });
    let (width, height) = fitted(DEFAULT_SIZE, available);
    let (min_width, min_height) = fitted(MIN_SIZE, available);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(width), px(height)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some("Wyck".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(12.), px(12.))),
        }),
        window_min_size: Some(size(px(min_width), px(min_height))),
        // The bar moves the window itself, so AppKit must not treat it as a move region too.
        app_owns_titlebar_drag: true,
        ..WindowOptions::default()
    }
}

/// The version text: `v0.1.0`, with `-dev` in a debug build.
#[must_use]
pub fn version_label() -> String {
    let suffix = if cfg!(debug_assertions) { "-dev" } else { "" };
    format!("v{}{suffix}", env!("CARGO_PKG_VERSION"))
}

/// The logo: a light tile with three dark bars.
fn logo() -> Div {
    div()
        .relative()
        .size(px(16.))
        .flex_none()
        .child(
            svg()
                .path("wyck/logo-tile.svg")
                .absolute()
                .size_full()
                .text_color(theme::accent()),
        )
        .child(
            svg()
                .path("wyck/logo-bars.svg")
                .absolute()
                .size_full()
                .text_color(theme::card()),
        )
}

/// One of the three buttons. Its ground and its icon fade to the hover colors in 120 ms: gray
/// for minimize and maximize, red for close.
fn button(
    id: &'static str,
    area: WindowControlArea,
    icon: &'static str,
    close: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let hover = Hover::track(id, window, cx);
    let (ground, lit) = if close {
        (theme::red(), theme::fg())
    } else {
        (theme::muted(), theme::fg())
    };
    let color = hover.mix(theme::dim(), lit);
    let button = div()
        .id(ElementId::from(id))
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .w(px(BUTTON_WIDTH))
        .h_full()
        .bg(hover.mix(theme::alpha(ground, 0.0), ground))
        .on_hover(hover.handler())
        .when(close, |b| b.active(|s| s.bg(theme::red_pressed())))
        .child(svg().path(icon).size(px(10.)).flex_none().text_color(color));
    if cfg!(target_os = "windows") {
        // The system handles the click; nothing to do here.
        button.window_control_area(area)
    } else {
        button.on_click(move |_, window, cx| {
            cx.stop_propagation();
            on_click(window, cx);
        })
    }
}

/// The bar. `window` says whether to show the maximize or the restore icon.
pub fn titlebar(window: &mut Window, cx: &mut App) -> impl IntoElement {
    let macos = cfg!(target_os = "macos");
    let maximized = window.is_maximized();

    let controls = (!macos).then(|| {
        div()
            .flex()
            .flex_row()
            .items_stretch()
            .flex_none()
            .h_full()
            .child(button(
                "window-minimize",
                WindowControlArea::Min,
                "wyck/win-min.svg",
                false,
                |window, _| window.minimize_window(),
                window,
                cx,
            ))
            .child(button(
                "window-maximize",
                WindowControlArea::Max,
                if maximized {
                    "wyck/win-restore.svg"
                } else {
                    "wyck/win-max.svg"
                },
                false,
                |window, _| window.zoom_window(),
                window,
                cx,
            ))
            .child(button(
                "window-close",
                WindowControlArea::Close,
                "wyck/win-close.svg",
                true,
                |window, _| window.remove_window(),
                window,
                cx,
            ))
    });

    let drag = div()
        .id("titlebar-drag")
        .flex()
        .flex_row()
        .flex_1()
        .items_center()
        .gap(px(10.))
        .h_full()
        .window_control_area(WindowControlArea::Drag)
        .when(cfg!(target_os = "linux"), |bar| {
            bar.on_mouse_down(MouseButton::Left, |event, window, _| {
                if event.click_count >= 2 {
                    window.zoom_window();
                } else {
                    window.start_window_move();
                }
            })
        })
        .child(
            div().flex().items_center().gap(px(7.)).child(logo()).child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::fg())
                    .child("Wyck"),
            ),
        )
        .child(div().flex_1())
        .child(
            div()
                .mr(px(14.))
                .font_features(theme::tabular())
                .text_size(px(11.))
                .text_color(theme::dim())
                .child(version_label()),
        );

    let left_inset: Pixels = px(if macos { MAC_INSET } else { 14. });
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .w_full()
        .h(px(HEIGHT))
        .pl(left_inset)
        .bg(theme::card())
        .border_b_1()
        .border_color(theme::border())
        .child(drag)
        .children(controls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_label_names_the_crate_version() {
        let label = version_label();
        assert!(label.starts_with(concat!("v", env!("CARGO_PKG_VERSION"))));
        assert_eq!(label.ends_with("-dev"), cfg!(debug_assertions));
    }

    #[test]
    fn a_window_never_opens_larger_than_its_screen() {
        let (w, h) = fitted((1000., 700.), Some((1366., 768.)));
        assert!(w <= 1366. * 0.92 && h <= 768. * 0.92, "{w}x{h}");
        let (w, h) = fitted((100., 100.), Some((4000., 3000.)));
        assert_eq!((w, h), (f32::from(sz(100.)), f32::from(sz(100.))));
        assert_eq!(fitted((100., 100.), None).0, f32::from(sz(100.)));
    }

    #[test]
    fn the_minimum_size_fits_inside_the_default_size() {
        assert!(MIN_SIZE.0 <= DEFAULT_SIZE.0 && MIN_SIZE.1 <= DEFAULT_SIZE.1);
    }
}

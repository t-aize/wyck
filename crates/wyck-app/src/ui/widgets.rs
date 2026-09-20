//! Small drawing helpers shared by the screens: the card, the buttons, the info rows, the
//! badges and the animated indicators.
//!
//! Each function returns an element and holds no state. Sizes are in logical pixels and follow
//! the design: a card is 380 to 440 wide, a button 38 high, a row 11 pixels of padding.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt as _, BoxShadow, Div, ElementId, FontWeight, Hsla, SharedString,
    Stateful, Svg, Transformation, div, ease_in_out, hsla, linear_color_stop, linear_gradient,
    percentage, point, pulsating_between, px, relative, svg, transparent_black,
};

use super::theme;
use crate::presentation::{Badge, Tone};

/// The icons of the application. Each is a file under `assets/icons`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    /// A desktop monitor: the local session.
    Monitor,
    /// A cloud: the remote server.
    Cloud,
    /// A right chevron.
    ChevronRight,
    /// A left chevron.
    ChevronLeft,
    /// A check mark.
    Check,
    /// A circle with an exclamation mark.
    CircleAlert,
    /// A circle with a cross.
    CircleX,
    /// A circle with a check mark.
    CircleCheck,
    /// A circle with an "i".
    Info,
    /// An empty circle: an unchecked item.
    Circle,
    /// A warning triangle.
    TriangleAlert,
    /// An open eye: show the token.
    Eye,
    /// A crossed eye: hide the token.
    EyeOff,
    /// A clipboard: paste.
    Clipboard,
    /// Four corners: the bridge address.
    Scan,
    /// A window: the platform version.
    AppWindow,
    /// A person: the account.
    User,
    /// An arrow leaving a box: a link that opens the browser.
    ArrowUpRight,
    /// A cross: dismiss.
    X,
}

impl Glyph {
    fn path(self) -> &'static str {
        match self {
            Self::Monitor => "wyck/monitor.svg",
            Self::Cloud => "wyck/cloud.svg",
            Self::ChevronRight => "wyck/chevron-right.svg",
            Self::ChevronLeft => "wyck/chevron-left.svg",
            Self::Check => "wyck/check.svg",
            Self::CircleAlert => "wyck/circle-alert.svg",
            Self::CircleX => "wyck/circle-x.svg",
            Self::CircleCheck => "wyck/circle-check.svg",
            Self::Info => "wyck/info.svg",
            Self::Circle => "wyck/circle.svg",
            Self::TriangleAlert => "wyck/triangle-alert.svg",
            Self::Eye => "wyck/eye.svg",
            Self::EyeOff => "wyck/eye-off.svg",
            Self::Clipboard => "wyck/clipboard.svg",
            Self::Scan => "wyck/scan.svg",
            Self::AppWindow => "wyck/app-window.svg",
            Self::User => "wyck/user.svg",
            Self::ArrowUpRight => "wyck/arrow-up-right.svg",
            Self::X => "wyck/x.svg",
        }
    }
}

/// An icon of `size` pixels in `color`.
pub fn glyph(glyph: Glyph, size: f32, color: Hsla) -> Svg {
    svg()
        .path(glyph.path())
        .size(px(size))
        .flex_none()
        .text_color(color)
}

/// The panel of a screen: a rounded, bordered, softly shadowed card of `width` pixels whose
/// content is centered.
pub fn card(width: f32) -> Div {
    let shadow = |alpha: f32, y: f32, blur: f32, spread: f32| BoxShadow {
        color: hsla(0., 0., 0., alpha),
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    };
    div()
        .w(px(width))
        .flex_none()
        .my_auto()
        .flex()
        .flex_col()
        .items_center()
        .px(px(34.))
        .py(px(36.))
        .bg(theme::card())
        .border_1()
        .border_color(theme::border())
        .rounded(px(12.))
        .shadow(vec![shadow(0.4, 1., 2., 0.), shadow(0.6, 12., 32., -16.)])
}

/// The heading of a card.
pub fn title(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(19.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::fg())
        .text_center()
        .child(text.into())
}

/// The paragraph under a heading, at most `max_width` pixels wide.
pub fn lead(text: impl Into<SharedString>, max_width: f32) -> Div {
    div()
        .mt(px(8.))
        .mb(px(24.))
        .max_w(px(max_width))
        .text_size(px(13.))
        .line_height(relative(1.6))
        .text_color(theme::dim())
        .text_center()
        .child(text.into())
}

/// The full-width main button.
pub fn primary_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w_full()
        .h(px(38.))
        .rounded(px(8.))
        .bg(theme::accent())
        .text_color(theme::card())
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|style| style.bg(theme::accent_hover()))
        .child(label.into())
}

/// A quiet text link, with an optional icon after the text.
pub fn text_link(
    id: &'static str,
    label: impl Into<SharedString>,
    trailing: Option<Glyph>,
) -> Stateful<Div> {
    div()
        .id(id)
        .group(id)
        .flex()
        .items_center()
        .gap(px(4.))
        .text_size(px(12.))
        .text_color(theme::dim())
        .cursor_pointer()
        .hover(|style| style.text_color(theme::fg()))
        .child(label.into())
        .children(trailing.map(|g| {
            glyph(g, 12., theme::dim()).group_hover(id, |style| style.text_color(theme::fg()))
        }))
}

/// The "Back" link in the top left corner of a screen.
pub fn back_button(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .group(id)
        .absolute()
        .top(px(24.))
        .left(px(28.))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(12.))
        .text_color(theme::dim())
        .cursor_pointer()
        .hover(|style| style.text_color(theme::fg()))
        .child(
            glyph(Glyph::ChevronLeft, 12., theme::dim())
                .group_hover(id, |style| style.text_color(theme::fg())),
        )
        .child("Back")
}

/// A bordered list of rows on the window background.
pub fn panel() -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .bg(theme::bg())
        .border_1()
        .border_color(theme::border())
        .rounded(px(10.))
        .overflow_hidden()
}

/// A row of a [`panel`]: an optional icon, a label that takes the space, and a value. `divider`
/// draws the line under it, so every row but the last has one.
pub fn row(
    icon: Option<Glyph>,
    label: impl Into<SharedString>,
    value: impl IntoElement,
    divider: bool,
) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.))
        .px(px(14.))
        .py(px(11.))
        .when(divider, |row| {
            row.border_b_1().border_color(theme::border())
        })
        .children(icon.map(|g| glyph(g, 14., theme::dim())))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .line_height(relative(1.5))
                .text_size(px(12.))
                .text_color(theme::dim())
                .child(label.into()),
        )
        .child(value)
}

/// Text in the monospace font, for tokens, addresses and figures.
pub fn mono(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(theme::MONO)
        .text_size(px(12.))
        .text_color(theme::fg())
        .child(text.into())
}

/// A small uppercase label with its own colors.
pub fn pill(text: impl Into<SharedString>, color: Hsla, background: Hsla) -> Div {
    let text: SharedString = text.into();
    div()
        .px(px(6.))
        .py(px(2.))
        .rounded(px(4.))
        .bg(background)
        .text_color(color)
        .text_size(px(9.5))
        .font_weight(FontWeight::MEDIUM)
        .child(text.to_uppercase())
}

/// A [`Badge`] from the presentation layer, colored by its tone.
pub fn badge(badge: &Badge) -> Div {
    match badge.tone {
        Tone::Neutral => pill(badge.text.clone(), theme::dim(), theme::muted()),
        tone => {
            let color = theme::tone_color(tone);
            pill(badge.text.clone(), color, theme::alpha(color, 0.14))
        }
    }
}

/// A round 52 pixel badge with an icon: the mark at the top of a card.
pub fn status_disc(icon: Glyph, color: Hsla, background: Hsla) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(52.))
        .mb(px(16.))
        .rounded_full()
        .bg(background)
        .child(glyph(icon, 22., color))
}

/// A tinted box with a warning icon and a line of monospace text: an error as the server said it.
pub fn error_box(text: impl Into<SharedString>) -> Div {
    div()
        .w_full()
        .mb(px(18.))
        .flex()
        .flex_row()
        .items_start()
        .gap(px(10.))
        .px(px(13.))
        .py(px(11.))
        .rounded(px(8.))
        .bg(theme::alpha(theme::red(), 0.06))
        .border_1()
        .border_color(theme::alpha(theme::red(), 0.25))
        .child(
            div()
                .mt(px(2.))
                .child(glyph(Glyph::TriangleAlert, 14., theme::red())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(theme::MONO)
                .text_size(px(11.5))
                .line_height(relative(1.5))
                .text_color(theme::red_text())
                .child(text.into()),
        )
}

/// The waiting indicator: a spinning arc around a disc holding `icon`. With `rings`, two circles
/// swell and fade around it.
pub fn spinner(icon: Glyph, disc: f32, rings: bool) -> Div {
    const BOX: f32 = 56.;
    let ring = |id: &'static str, phase: f32| {
        div()
            .absolute()
            .rounded_full()
            .border_1()
            .border_color(theme::dim())
            .with_animation(
                id,
                Animation::new(Duration::from_millis(1800)).repeat(),
                move |el, delta| {
                    let t = (delta + phase) % 1.0;
                    let eased = 1.0 - (1.0 - t) * (1.0 - t);
                    let side = BOX * (0.9 + 0.7 * eased);
                    let offset = (BOX - side) / 2.0;
                    el.top(px(offset))
                        .left(px(offset))
                        .size(px(side))
                        .opacity(0.5 * (1.0 - eased))
                },
            )
    };
    div()
        .relative()
        .size(px(BOX))
        .mb(px(18.))
        .flex()
        .items_center()
        .justify_center()
        .when(rings, |el| {
            el.child(ring("ring-a", 0.0))
                .child(ring("ring-b", 1.0 / 3.0))
        })
        .child(
            svg()
                .path("wyck/ring.svg")
                .absolute()
                .top(px(4.))
                .left(px(4.))
                .size(px(48.))
                .text_color(theme::muted()),
        )
        .child(
            svg()
                .path("wyck/arc.svg")
                .absolute()
                .top(px(4.))
                .left(px(4.))
                .size(px(48.))
                .text_color(theme::fg())
                .with_animation(
                    "spin",
                    Animation::new(Duration::from_secs(1)).repeat(),
                    |el, delta| el.with_transformation(Transformation::rotate(percentage(delta))),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(disc))
                .rounded_full()
                .bg(theme::muted())
                .child(glyph(icon, disc * 0.47, theme::fg())),
        )
}

/// A dot that breathes: something is happening.
pub fn pulse_dot(color: Hsla) -> impl IntoElement {
    div().size(px(6.)).rounded_full().bg(color).with_animation(
        "pulse-dot",
        Animation::new(Duration::from_millis(1600))
            .repeat()
            .with_easing(pulsating_between(0.35, 1.0)),
        |el, delta| el.opacity(delta),
    )
}

/// The thin indeterminate bar at the bottom of a waiting panel.
pub fn progress_bar() -> Div {
    let fade = |from: Hsla, to: Hsla| {
        linear_gradient(90., linear_color_stop(from, 0.), linear_color_stop(to, 1.))
    };
    div()
        .relative()
        .w_full()
        .h(px(2.))
        .bg(theme::muted())
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .top_0()
                .h_full()
                .w(relative(0.4))
                .flex()
                .flex_row()
                .child(
                    div()
                        .h_full()
                        .w(relative(0.5))
                        .bg(fade(transparent_black(), theme::dim())),
                )
                .child(
                    div()
                        .h_full()
                        .w(relative(0.5))
                        .bg(fade(theme::dim(), transparent_black())),
                )
                .with_animation(
                    "progress-slide",
                    Animation::new(Duration::from_millis(1400))
                        .repeat()
                        .with_easing(ease_in_out),
                    |el, delta| el.left(relative(-0.24 + 0.88 * delta)),
                ),
        )
}

/// The "or" between two ways of doing the same thing.
pub fn or_divider() -> Div {
    let line = || div().flex_1().h(px(1.)).bg(theme::border());
    div()
        .w_full()
        .mt(px(20.))
        .mb(px(4.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12.))
        .child(line())
        .child(
            div()
                .text_size(px(11.))
                .text_color(theme::dim())
                .child("or"),
        )
        .child(line())
}

/// A full-width outlined button, for actions that are not the main one.
pub fn secondary_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w_full()
        .h(px(38.))
        .rounded(px(8.))
        .bg(theme::bg())
        .border_1()
        .border_color(theme::alpha(theme::fg(), 0.16))
        .text_color(theme::fg())
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|style| style.bg(theme::muted()))
        .child(label.into())
}

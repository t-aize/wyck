//! Small drawing helpers shared by the screens: the card, the buttons, the info rows, the
//! badges and the animated indicators.
//!
//! Each function returns an element. Sizes are in logical pixels and follow the design: a card is
//! 380 to 460 wide, a button 38 high, a row 11 pixels of padding. The ones that react to the
//! pointer (buttons, links) take the window so that the reaction fades instead of flipping, see
//! [`motion::Hover`]. The [`Card`] brings its children in one after the other.

use std::time::Duration;

use gpui_kit::base::{Presence, Transition, TransitionId};
use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, BoxShadow, Div, ElementId, FontWeight, Hsla,
    SharedString, Stateful, Svg, Transformation, Window, div, ease_in_out, hsla, linear_color_stop,
    linear_gradient, percentage, point, pulsating_between, relative, svg, transparent_black,
};

use super::motion::{self, Hover};
use super::theme::{self, sz};
use crate::presentation::{Badge, Tone};
use crate::symbols::{AssetClass, Mark, SymbolIcon};

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
    /// Four corners: the server address.
    Scan,
    /// A window: the platform version.
    AppWindow,
    /// A person: the account.
    User,
    /// An arrow leaving a box: a link that opens the browser.
    ArrowUpRight,
    /// A cross: dismiss.
    X,
    /// Three sliders: the chart's indicators.
    Sliders,
    /// Four squares: the full workspace.
    LayoutGrid,
    /// One square: the chart-only workspace.
    Square,
    /// An arrow leaving a box: switch to another connection.
    LogOut,
    /// A small triangle pointing up: the price rose.
    TickUp,
    /// A small triangle pointing down: the price fell.
    TickDown,
    /// A magnifying glass: search.
    Search,
    /// Two chevrons, one up and one down: a control that opens a list.
    ChevronsUpDown,
    /// An arrow up: a key.
    ArrowUp,
    /// An arrow down: a key.
    ArrowDown,
    /// The return key.
    Enter,
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
            Self::Sliders => "wyck/sliders.svg",
            Self::LayoutGrid => "wyck/layout-grid.svg",
            Self::Square => "wyck/square.svg",
            Self::LogOut => "wyck/log-out.svg",
            Self::TickUp => "wyck/tick-up.svg",
            Self::TickDown => "wyck/tick-down.svg",
            Self::Search => "wyck/lucide-search.svg",
            Self::ChevronsUpDown => "wyck/lucide-chevrons-up-down.svg",
            Self::ArrowUp => "wyck/lucide-arrow-up.svg",
            Self::ArrowDown => "wyck/lucide-arrow-down.svg",
            Self::Enter => "wyck/lucide-corner-down-left.svg",
        }
    }
}

/// An icon of `size` pixels in `color`.
pub fn glyph(glyph: Glyph, size: f32, color: Hsla) -> Svg {
    svg()
        .path(glyph.path())
        .size(sz(size))
        .flex_none()
        .text_color(color)
}

/// The panel of a screen: a rounded, bordered, softly shadowed card of `width` pixels whose
/// content is centered.
///
/// Its children do not appear all at once: each rises 8 px into place and fades in, one after the
/// other (see [`motion`]). The first child starts 90 ms after the card is mounted, and every next
/// one 32 ms later.
#[derive(IntoElement)]
pub struct Card {
    width: f32,
    children: Vec<AnyElement>,
}

/// A [`Card`] of `width` pixels.
pub fn card(width: f32) -> Card {
    Card {
        width,
        children: Vec::new(),
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// One child of a card, faded in and lifted into place after its turn.
pub fn rise(index: usize, child: AnyElement, window: &mut Window, cx: &mut App) -> Div {
    let transition = Transition::new(motion::RISE_TIME)
        .delay(motion::RISE_START + motion::STAGGER * u32::try_from(index).unwrap_or(0))
        .easing(motion::enter());
    let progress = Presence::new(TransitionId::from((index, "rise")), true)
        .transition(transition)
        .sample(window, cx)
        .progress;
    div()
        .relative()
        .top(sz((1.0 - progress) * motion::RISE))
        .opacity(progress)
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .child(child)
}

impl RenderOnce for Card {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let shadow = |alpha: f32, y: f32, blur: f32, spread: f32| BoxShadow {
            color: hsla(0., 0., 0., alpha),
            offset: point(sz(0.), sz(y)),
            blur_radius: sz(blur),
            spread_radius: sz(spread),
            inset: false,
        };
        let children: Vec<Div> = self
            .children
            .into_iter()
            .enumerate()
            .map(|(index, child)| rise(index, child, window, cx))
            .collect();
        div()
            .w(sz(self.width))
            .flex_none()
            .my_auto()
            .flex()
            .flex_col()
            .items_center()
            .px(sz(34.))
            .py(sz(36.))
            .bg(theme::card())
            .border_1()
            .border_color(theme::border())
            .rounded(sz(12.))
            .shadow(vec![shadow(0.4, 1., 2., 0.), shadow(0.6, 12., 32., -16.)])
            .children(children)
    }
}

/// The heading of a card.
pub fn title(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(sz(19.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::fg())
        .text_center()
        .child(text.into())
}

/// The paragraph under a heading, at most `max_width` pixels wide.
pub fn lead(text: impl Into<SharedString>, max_width: f32) -> Div {
    div()
        .mt(sz(8.))
        .mb(sz(24.))
        .max_w(sz(max_width))
        .text_size(sz(13.))
        .line_height(relative(1.6))
        .text_color(theme::dim())
        .text_center()
        .child(text.into())
}

/// The full-width main button. It brightens under the pointer.
pub fn primary_button(
    id: &'static str,
    label: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let hover = Hover::track(id, window, cx);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w_full()
        .h(sz(44.))
        .rounded(sz(8.))
        .bg(hover.mix(theme::accent(), theme::accent_hover()))
        .text_color(theme::card())
        .text_size(sz(13.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .active(|style| style.bg(theme::accent()).opacity(0.88))
        .on_hover(hover.handler())
        .child(label.into())
}

/// A full-width outlined button, for actions that are not the main one. It fills in under the
/// pointer.
pub fn secondary_button(
    id: &'static str,
    label: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let hover = Hover::track(id, window, cx);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w_full()
        .h(sz(44.))
        .rounded(sz(8.))
        .bg(hover.mix(theme::bg(), theme::muted()))
        .border_1()
        .border_color(theme::alpha(theme::fg(), 0.16))
        .text_color(theme::fg())
        .text_size(sz(13.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .active(|style| style.opacity(0.88))
        .on_hover(hover.handler())
        .child(label.into())
}

/// A quiet text link, with an optional icon after the text. The text and the icon brighten under
/// the pointer, and a chevron slides 2 px toward where the link leads.
pub fn text_link(
    id: &'static str,
    label: impl Into<SharedString>,
    trailing: Option<Glyph>,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let hover = Hover::track(id, window, cx);
    let color = hover.mix(theme::dim(), theme::fg());
    let nudge = if trailing == Some(Glyph::ChevronRight) {
        2.0 * hover.amount
    } else {
        0.0
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(sz(4.))
        .text_size(sz(12.))
        .text_color(color)
        .cursor_pointer()
        .on_hover(hover.handler())
        .child(label.into())
        .children(trailing.map(|g| glyph(g, 12., color).relative().left(sz(nudge))))
}

/// The "Back" link in the top left corner of a screen. Its chevron slides 2 px to the left under
/// the pointer.
pub fn back_button(id: &'static str, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    let hover = Hover::track(id, window, cx);
    let color = hover.mix(theme::dim(), theme::fg());
    div()
        .id(id)
        .absolute()
        .top(sz(24.))
        .left(sz(28.))
        .flex()
        .items_center()
        .gap(sz(6.))
        .text_size(sz(12.))
        .text_color(color)
        .cursor_pointer()
        .on_hover(hover.handler())
        .child(
            glyph(Glyph::ChevronLeft, 12., color)
                .relative()
                .left(sz(-2.0 * hover.amount)),
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
        .rounded(sz(10.))
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
        .gap(sz(10.))
        .px(sz(14.))
        .py(sz(11.))
        .when(divider, |row| {
            row.border_b_1().border_color(theme::border())
        })
        .children(icon.map(|g| glyph(g, 14., theme::dim())))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(sz(12.))
                .line_height(relative(1.5))
                .text_color(theme::dim())
                .child(label.into()),
        )
        .child(value)
}

/// The value side of a row: tokens, addresses and figures, in tabular figures.
pub fn value(text: impl Into<SharedString>) -> Div {
    div()
        .font_features(theme::tabular())
        .text_size(sz(12.))
        .text_color(theme::fg())
        .child(text.into())
}

/// A small uppercase label with its own colors.
pub fn pill(text: impl Into<SharedString>, color: Hsla, background: Hsla) -> Div {
    let text: SharedString = text.into();
    div()
        .px(sz(6.))
        .py(sz(2.))
        .rounded(sz(4.))
        .bg(background)
        .text_color(color)
        .text_size(sz(9.5))
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

/// A round 52 pixel badge with an icon: the mark at the top of a card. It pops in: it grows from
/// 70 percent, goes a little past its size, and settles.
pub fn status_disc(
    icon: Glyph,
    color: Hsla,
    background: Hsla,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let pop = Presence::new(TransitionId::from("disc-pop"), true)
        .transition(
            Transition::new(Duration::from_millis(460))
                .delay(motion::RISE_START)
                .ease(motion::overshoot),
        )
        .sample(window, cx)
        .progress;
    let scale = 0.7 + 0.3 * pop;
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(sz(52.))
        .mb(sz(16.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(sz(52. * scale))
                .rounded_full()
                .bg(background)
                .child(glyph(icon, 22. * scale, color)),
        )
}

/// A tinted box with a warning icon and a line of monospace text: an error as the server said it.
pub fn error_box(text: impl Into<SharedString>) -> Div {
    div()
        .w_full()
        .mb(sz(18.))
        .flex()
        .flex_row()
        .items_start()
        .gap(sz(10.))
        .px(sz(13.))
        .py(sz(11.))
        .rounded(sz(8.))
        .bg(theme::alpha(theme::red(), 0.06))
        .border_1()
        .border_color(theme::alpha(theme::red(), 0.25))
        .child(
            div()
                .mt(sz(2.))
                .child(glyph(Glyph::TriangleAlert, 14., theme::red())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_features(theme::tabular())
                .text_size(sz(11.5))
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
                    el.top(sz(offset))
                        .left(sz(offset))
                        .size(sz(side))
                        .opacity(0.5 * (1.0 - eased))
                },
            )
    };
    div()
        .relative()
        .size(sz(BOX))
        .mb(sz(18.))
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
                .top(sz(4.))
                .left(sz(4.))
                .size(sz(48.))
                .text_color(theme::muted()),
        )
        .child(
            svg()
                .path("wyck/arc.svg")
                .absolute()
                .top(sz(4.))
                .left(sz(4.))
                .size(sz(48.))
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
                .size(sz(disc))
                .rounded_full()
                .bg(theme::muted())
                .child(glyph(icon, disc * 0.47, theme::fg())),
        )
}

/// A dot that breathes: something is happening.
pub fn pulse_dot(color: Hsla) -> impl IntoElement {
    div().size(sz(6.)).rounded_full().bg(color).with_animation(
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
        .h(sz(2.))
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
    let line = || div().flex_1().h(sz(1.)).bg(theme::border());
    div()
        .w_full()
        .mt(sz(20.))
        .mb(sz(4.))
        .flex()
        .flex_row()
        .items_center()
        .gap(sz(12.))
        .child(line())
        .child(
            div()
                .text_size(sz(11.))
                .text_color(theme::dim())
                .child("or"),
        )
        .child(line())
}

/// A small square button with an icon, whose ground fades in under the pointer.
pub fn icon_button(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    icon_button_sized(id, 30., 6., window, cx)
}

/// An [`icon_button`] of `side` design pixels with corners of `radius`.
pub fn icon_button_sized(
    id: impl Into<ElementId>,
    side: f32,
    radius: f32,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let hover = Hover::track(id.clone(), window, cx);
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(sz(side))
        .rounded(sz(radius))
        .bg(hover.mix(theme::alpha(theme::muted(), 0.0), theme::muted()))
        .cursor_pointer()
        .on_hover(hover.handler())
}

/// The builder of a tooltip that says `text`, to pass to `.tooltip(...)` of an element.
pub fn tip(text: &'static str) -> impl Fn(&mut Window, &mut App) -> gpui_kit::AnyView + 'static {
    move |window, cx| gpui_kit::component::tooltip::Tooltip::new(text).build(window, cx)
}

/// The svg of an asset class icon.
fn class_icon_path(class: AssetClass) -> &'static str {
    match class {
        AssetClass::Metals => "wyck/lucide-gem.svg",
        AssetClass::Energies => "wyck/lucide-fuel.svg",
        AssetClass::Indices => "wyck/lucide-trending-up.svg",
        AssetClass::Crypto => "wyck/lucide-bitcoin.svg",
        AssetClass::Shares => "wyck/lucide-building-2.svg",
        AssetClass::Commodities => "wyck/lucide-wheat.svg",
        AssetClass::Bonds => "wyck/lucide-landmark.svg",
        AssetClass::Forex | AssetClass::Other => "wyck/lucide-layers.svg",
    }
}

/// One mark (a flag, an asset icon or letters) in a circle of `diameter` design pixels.
fn mark_circle(mark: &Mark, diameter: f32) -> AnyElement {
    match mark {
        Mark::Flag(code) => gpui_kit::img(SharedString::from(format!("wyck/flags/{code}.svg")))
            .size(sz(diameter))
            .flex_none()
            .rounded_full()
            .into_any_element(),
        Mark::Class(class) => div()
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .size(sz(diameter))
            .rounded_full()
            .bg(theme::muted())
            .child(
                svg()
                    .path(class_icon_path(*class))
                    .size(sz(diameter * 0.52))
                    .flex_none()
                    .text_color(theme::fg()),
            )
            .into_any_element(),
        Mark::Letters(text) => div()
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .size(sz(diameter))
            .rounded_full()
            .bg(theme::muted())
            .text_size(sz(diameter * 0.34))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::fg())
            .child(text.clone())
            .into_any_element(),
    }
}

/// The icon of a symbol in a square of `size` design pixels: one round mark, or two overlapped for
/// a pair (the base at the top left, the quote at the bottom right, ringed in `ring` so it reads as
/// laid over the first).
pub fn symbol_icon(icon: &SymbolIcon, size: f32, ring: Hsla) -> AnyElement {
    let Some(secondary) = &icon.secondary else {
        return div()
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .size(sz(size))
            .child(mark_circle(&icon.primary, size * 0.92))
            .into_any_element();
    };
    let diameter = size * 0.68;
    div()
        .relative()
        .flex_none()
        .size(sz(size))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .child(mark_circle(&icon.primary, diameter)),
        )
        .child(
            div()
                .absolute()
                .bottom_0()
                .right_0()
                .rounded_full()
                .border_2()
                .border_color(ring)
                .child(mark_circle(secondary, diameter - 3.0)),
        )
        .into_any_element()
}

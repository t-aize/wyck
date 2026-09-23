//! Motion helpers built on GPUI's own `with_animation`.
//!
//! GPUI has no CSS-style transitions: an animation is an [`Animation`] (a duration, an optional
//! repeat and an easing) plus a closure that receives a progress value from 0 to 1 and restyles
//! the element for that frame. An animation starts when its element first appears under a given
//! element id and stays finished afterwards, so an id that should replay (a screen that is
//! entered again) has to change, which is what the `epoch` values threaded through the UI are
//! for.

use std::f32::consts::PI;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, Div, ElementId, IntoElement, Styled, Svg, Transformation, percentage,
    px,
};

/// Length of a screen or card entrance.
const ENTER: Duration = Duration::from_millis(380);
/// Extra delay per item in a staggered list.
const STAGGER: Duration = Duration::from_millis(70);

pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Overshoots the target a little before settling: good for things that "pop" in.
pub fn ease_out_back(t: f32) -> f32 {
    const C1: f32 = 1.70158;
    const C3: f32 = C1 + 1.0;
    1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
}

pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

/// Progress of an item that starts `delay` into an animation of `total` length and then takes
/// `own` to finish, clamped to 0..=1.
fn staggered(delta: f32, total: Duration, delay: Duration, own: Duration) -> f32 {
    ((delta * total.as_secs_f32() - delay.as_secs_f32()) / own.as_secs_f32()).clamp(0.0, 1.0)
}

/// Fades an element in while it slides up into place. `index` staggers siblings: item 0 starts
/// right away, item 1 a beat later, and so on.
pub fn enter<E: Styled + IntoElement + 'static>(
    el: E,
    id: impl Into<ElementId>,
    index: usize,
) -> impl IntoElement {
    let delay = STAGGER * index as u32;
    let total = ENTER + delay;
    el.relative()
        .with_animation(id, Animation::new(total), move |el, delta| {
            let t = ease_out_cubic(staggered(delta, total, delay, ENTER));
            el.opacity(t).top(px((1.0 - t) * 16.0))
        })
}

/// Slides an element in from above while fading it in, for banners that appear over content.
pub fn drop_in(el: Div, id: impl Into<ElementId>) -> impl IntoElement {
    el.relative().with_animation(
        id,
        Animation::new(Duration::from_millis(320)),
        |el, delta| {
            let t = ease_out_back(delta).min(1.2);
            el.opacity(delta.min(1.0)).top(px((t - 1.0) * 14.0))
        },
    )
}

/// A quick horizontal shake that fades out, for an element that just reported a mistake.
pub fn shake(el: Div, id: impl Into<ElementId>) -> impl IntoElement {
    el.relative().with_animation(
        id,
        Animation::new(Duration::from_millis(420)),
        |el, delta| {
            let amplitude = (1.0 - delta) * 7.0;
            el.left(px((delta * PI * 6.0).sin() * amplitude))
        },
    )
}

/// Grows an element from nothing to its full size with a small overshoot.
pub fn pop(el: Div, id: impl Into<ElementId>, size: f32) -> impl IntoElement {
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(520)),
        move |el, delta| {
            let t = ease_out_back(delta);
            el.size(px(size * t.max(0.0))).opacity(delta.min(1.0))
        },
    )
}

/// An expanding, fading ring behind an element: a radar-style "ping" that repeats forever.
pub fn ping(el: Div, id: impl Into<ElementId>, size: f32) -> impl IntoElement {
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(1800)).repeat(),
        move |el, delta| {
            let t = ease_out_cubic(delta);
            el.size(px(size * (1.0 + 0.6 * t))).opacity((1.0 - t) * 0.5)
        },
    )
}

/// Slow breathing opacity, for a status dot or a "waiting" hint.
pub fn breathe(el: Div, id: impl Into<ElementId>) -> impl IntoElement {
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(1600))
            .repeat()
            .with_easing(gpui::pulsating_between(0.35, 1.0)),
        |el, delta| el.opacity(delta),
    )
}

/// A gentle up-and-down float, for the hero icon on the welcome screen.
pub fn float(el: Div, id: impl Into<ElementId>) -> impl IntoElement {
    el.relative().with_animation(
        id,
        Animation::new(Duration::from_millis(3600)).repeat(),
        |el, delta| el.top(px((delta * 2.0 * PI).sin() * 4.0)),
    )
}

/// Spins an icon forever, for a "working on it" indicator.
pub fn spin(icon: Svg, id: impl Into<ElementId>) -> impl IntoElement {
    icon.with_animation(
        id,
        Animation::new(Duration::from_millis(1000)).repeat(),
        |icon, delta| icon.with_transformation(Transformation::rotate(percentage(delta))),
    )
}

pub fn lerp_color(from: gpui::Rgba, to: gpui::Rgba, t: f32) -> gpui::Rgba {
    gpui::Rgba {
        r: lerp(from.r, to.r, t),
        g: lerp(from.g, to.g, t),
        b: lerp(from.b, to.b, t),
        a: lerp(from.a, to.a, t),
    }
}

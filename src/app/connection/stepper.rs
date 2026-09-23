//! The progress indicator along the top of the sign-in steps: four numbered stops joined by
//! connectors. The connector that was just completed fills in with an animation, and the active
//! stop has a ring pinging around it.

use std::time::Duration;

use gpui::prelude::*;
use gpui::{Animation, AnimationExt, div, px, relative};
use gpui_kit::assets::IconName;

use super::theme;
use super::ui;
use crate::app::anim;

const STEPS: [&str; 4] = ["Credentials", "Sign in", "Account", "Connect"];

/// `current` is the index of the active stop, or `STEPS.len()` when every stop is done.
pub fn stepper(current: usize, epoch: u64) -> impl IntoElement {
    let mut row = div().flex().flex_row().items_start();
    for (index, label) in STEPS.iter().enumerate() {
        if index > 0 {
            row = row.child(connector(index, current, epoch));
        }
        row = row.child(stop(index, label, current));
    }
    div()
        .absolute()
        .top_6()
        .w_full()
        .flex()
        .justify_center()
        .child(anim::enter(row, ("stepper-enter", epoch), 0))
}

fn stop(index: usize, label: &'static str, current: usize) -> impl IntoElement {
    let done = index < current;
    let active = index == current;

    let circle = div()
        .size(px(26.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.))
        .border_1()
        .when(done, |el| {
            el.bg(theme::accent())
                .border_color(theme::accent())
                .text_color(theme::accent_fg())
                .child(ui::icon_colored(IconName::Check, 13., theme::accent_fg()))
        })
        .when(active, |el| {
            el.bg(theme::accent_selected())
                .border_color(theme::accent())
                .text_color(theme::fg())
                .child((index + 1).to_string())
        })
        .when(!done && !active, |el| {
            el.border_color(theme::border_subtle())
                .text_color(theme::muted_fg())
                .child((index + 1).to_string())
        });

    let marker = div()
        .relative()
        .size(px(26.))
        .flex()
        .items_center()
        .justify_center()
        .child(circle);

    div()
        .w(px(76.))
        .flex()
        .flex_col()
        .items_center()
        .gap_1p5()
        .child(marker)
        .child(
            div()
                .text_size(px(11.))
                .text_color(if done || active {
                    theme::fg()
                } else {
                    theme::muted_fg()
                })
                .child(label),
        )
}

/// The line between stop `index - 1` and stop `index`.
fn connector(index: usize, current: usize, epoch: u64) -> impl IntoElement {
    let filled = index <= current;
    // Only the line that was completed by the latest step animates; older ones are just full.
    let just_completed = index == current;

    let fill = div().h_full().rounded_full().bg(theme::accent());
    let fill = if just_completed {
        fill.with_animation(
            ("stepper-fill", epoch),
            Animation::new(Duration::from_millis(520)),
            |el, delta| el.w(relative(anim::ease_out_cubic(delta))),
        )
        .into_any_element()
    } else {
        fill.w(relative(if filled { 1.0 } else { 0.0 }))
            .into_any_element()
    };

    // Vertically centered on the 26px circles.
    div()
        .mt(px(12.))
        .w(px(28.))
        .h(px(2.))
        .flex()
        .rounded_full()
        .bg(theme::border_subtle())
        .child(fill)
}

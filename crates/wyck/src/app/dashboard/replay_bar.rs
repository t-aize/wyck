//! The Replay control strip: a persistent dock between the header and the chart body
//! while a replay is active. Forward-only for now (play/pause, step forward, speed,
//! exit): scrubbing back would need a way to retract bars already revealed on the chart,
//! which does not exist yet, so it is left out rather than half-built (exiting and
//! toggling replay back on starts a fresh one from the chart's current live edge).

use gpui::prelude::*;
use gpui::{Context, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::Sizable;

use wyck_market_data::replay::prefs::SPEED_PRESETS;

use super::Dashboard;
use crate::app::theme;

/// The preset after `current` in [`SPEED_PRESETS`], wrapping around, or the middle of the
/// range if `current` is not one of them.
pub(super) fn next_speed(current: f64) -> f64 {
    let at = SPEED_PRESETS
        .iter()
        .position(|speed| (*speed - current).abs() < 0.001)
        .unwrap_or(2);
    SPEED_PRESETS[(at + 1) % SPEED_PRESETS.len()]
}

/// Formats a Replay cursor for the control bar's time readout (UTC, minute precision:
/// finer than that is not meaningful for bar-by-bar replay).
fn format_cursor(time_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(time_ms)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

impl Dashboard {
    /// The control strip, or `None` when no replay is active.
    pub(super) fn render_replay_bar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (speed, playing, cursor_ms) = self.replay_state()?;

        let icon_button = |id: &'static str, icon: IconName, tooltip: &'static str| {
            Button::new(id)
                .ghost()
                .small()
                .icon(icon)
                .tooltip(tooltip)
                .cursor_pointer()
        };

        Some(
            div()
                .flex_none()
                .w_full()
                .h(px(36.))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_3()
                .border_b_1()
                .border_color(theme::border_hairline())
                .bg(theme::bg())
                .child(
                    icon_button("replay-exit", IconName::X, "Exit replay").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.toggle_replay(cx);
                        },
                    )),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::muted_fg())
                        .child("Replay"),
                )
                .child(div().flex_1().min_w_0())
                .child(
                    icon_button(
                        "replay-play-pause",
                        if playing { IconName::Pause } else { IconName::Play },
                        if playing { "Pause" } else { "Play" },
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.replay_play_pause(cx))),
                )
                .child(
                    icon_button("replay-step-forward", IconName::StepForward, "Step forward")
                        .on_click(cx.listener(|this, _, _, cx| this.replay_step_forward(cx))),
                )
                .child(
                    Button::new("replay-speed")
                        .ghost()
                        .small()
                        .label(format!("{speed:.2}x"))
                        .tooltip("Playback speed")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| this.replay_cycle_speed(cx))),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::muted_fg())
                        .child(format_cursor(cursor_ms)),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_speed_cycles_through_the_presets_and_wraps() {
        assert_eq!(next_speed(0.25), 0.5);
        assert_eq!(next_speed(10.0), 0.25);
    }

    #[test]
    fn an_unrecognized_speed_lands_on_the_middle_preset() {
        assert_eq!(next_speed(3.7), SPEED_PRESETS[3]);
    }
}

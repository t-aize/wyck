//! The Replay control strip: a compact floating-looking bar docked at the bottom of the
//! chart while a replay is active, styled and ordered after TradingView's own Bar Replay
//! toolbar (play/pause, step forward, speed, jump to start / go to date, exit).
//!
//! Rewinding by scrubbing is deliberately not offered: neither TradingView nor
//! MetaTrader's Visual Mode support it either (see the module docs on
//! `chart::replay` for why). "Go to date" covers the same need by reloading the chart at
//! the picked point instead.
//!
//! Every icon-only button carries an [`gpui_kit::component::button::Button::accessibility_label`]
//! that matches its tooltip text (a verb phrase, e.g. "Step forward", not a description of
//! the icon), following the WAI-ARIA toolbar pattern for transport controls; a control
//! that is momentarily meaningless (step forward once the loaded range is exhausted, jump
//! to start already at the start) stays visible but disabled rather than disappearing, so
//! it stays discoverable and keyboard-reachable.

use gpui::prelude::*;
use gpui::{Context, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::{Disableable, Selectable, Sizable};

use super::Dashboard;
use crate::app::chart::drawing_props;
use crate::app::menu::{Entry, Item, Menu, Placement};
use crate::app::theme;

/// The prefill for the "go to date" field: the replay's current position.
pub(super) fn format_goto_default(cursor_ms: i64) -> String {
    drawing_props::format_time(wyck_chart::Zone::default(), cursor_ms)
}

/// Parses the "go to date" field the same way a drawing's time field is read:
/// `YYYY-MM-DD HH:MM`, with seconds, or a bare date.
pub(super) fn parse_goto(text: &str) -> Option<i64> {
    drawing_props::parse_time(wyck_chart::Zone::default(), text)
}

impl Dashboard {
    /// The control strip, or `None` when no replay is running on the layout.
    pub(super) fn render_replay_bar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let view = self.replay_view(cx)?;
        let zone = self.multi.read(cx).active_chart().read(cx).settings().zone;
        let dashboard = cx.entity();
        let speed_menu = Menu::new("replay-speed-menu", window, cx);
        let speed_items = if speed_menu.is_open(cx) {
            wyck_market_data::replay::prefs::SPEED_PRESETS
                .iter()
                .map(|&preset| {
                    let menu = speed_menu.clone();
                    let dashboard = dashboard.clone();
                    Item::Entry(
                        Entry::new(format!("{preset}x"))
                            .checked((preset - view.speed).abs() < 0.001)
                            .on_click(move |_window, cx| {
                                menu.close(cx);
                                dashboard.update(cx, |this, cx| this.replay_set_speed(preset, cx));
                            }),
                    )
                })
                .collect()
        } else {
            Vec::new()
        };

        let icon_button = |id: &'static str, icon: IconName, label: &'static str| {
            Button::new(id)
                .ghost()
                .small()
                .icon(icon)
                .tooltip(label)
                .accessibility_label(label)
                .cursor_pointer()
        };

        let can_jump_to_start = view.cursor_ms != view.start_ms;

        let bar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .rounded_xl()
            .border_1()
            .border_color(theme::border_subtle())
            .bg(theme::bg())
            .shadow_md()
            .px_2()
            .py_1p5()
            .child(
                icon_button(
                    "replay-play-pause",
                    if view.playing { IconName::Pause } else { IconName::Play },
                    if view.playing { "Pause" } else { "Play" },
                )
                .selected(view.playing)
                .on_click(cx.listener(|this, _, _, cx| this.replay_play_pause(cx))),
            )
            .child(
                icon_button("replay-step-forward", IconName::StepForward, "Step forward")
                    .disabled(view.exhausted)
                    .on_click(cx.listener(|this, _, _, cx| this.replay_step_forward(cx))),
            )
            .child(div().relative().child(
                Button::new("replay-speed")
                    .ghost()
                    .small()
                    .label(format!("{}x", view.speed))
                    .tooltip("Playback speed")
                    .accessibility_label("Playback speed")
                    .cursor_pointer()
                    .on_click({
                        let menu = speed_menu.clone();
                        move |_, _, cx| menu.toggle(cx)
                    }),
                ).children(speed_menu.popup(speed_items, Placement::Below(4.), window, cx)),
            )
            .child(
                icon_button("replay-jump-to-start", IconName::SkipBack, "Jump to start")
                    .disabled(!can_jump_to_start)
                    .on_click(cx.listener(|this, _, _, cx| this.replay_jump_to_start(cx))),
            )
            .child(self.render_replay_goto(cx))
            .child(
                div()
                    .px_1()
                    .text_size(px(12.))
                    .text_color(theme::muted_fg())
                    .child(drawing_props::format_time(zone, view.cursor_ms)),
            )
            .child(
                icon_button("replay-exit", IconName::X, "Exit replay")
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_replay(cx))),
            );

        Some(
            div()
                .flex_none()
                .w_full()
                .flex()
                .flex_row()
                .justify_center()
                .py_2()
                .child(bar),
        )
    }

    /// The "go to date" button and its popover (a single text field, `YYYY-MM-DD HH:MM`,
    /// submitted with Enter): the minimal keyboard-accessible version of jumping to an
    /// arbitrary point, since none of TradingView, MetaTrader or NinjaTrader offer a full
    /// calendar picker for this either.
    fn render_replay_goto(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let button = Button::new("replay-goto")
            .ghost()
            .small()
            .icon(IconName::Calendar)
            .tooltip("Go to date")
            .accessibility_label("Go to date")
            .cursor_pointer()
            .selected(self.replay_goto_open)
            .on_click(cx.listener(|this, _, window, cx| this.toggle_replay_goto(window, cx)));

        let popover = (self.replay_goto_open)
            .then(|| self.replay_goto.clone())
            .flatten()
            .map(|state| {
                crate::app::menu::below(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_2()
                        .w(px(220.))
                        .rounded_lg()
                        .border_1()
                        .border_color(theme::border_subtle())
                        .bg(theme::bg())
                        .shadow_md()
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::muted_fg())
                                .child("Go to date (YYYY-MM-DD HH:MM)"),
                        )
                        .child(Input::new(&state).small()),
                    4.,
                    // Above the click-outside-to-close backdrop (priority 0, see
                    // Dashboard::render), and matching Menu::popup's own priority so it
                    // stacks consistently with the speed dropdown next to it.
                    100,
                )
            });

        div().relative().child(button).children(popover)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_datetime_round_trips_through_format_and_parse() {
        let ms = 1_710_500_400_000; // 2024-03-15 11:00 UTC
        let text = format_goto_default(ms);
        assert_eq!(parse_goto(&text), Some(ms));
    }

    #[test]
    fn a_bare_date_parses_to_midnight() {
        assert!(parse_goto("2024-03-15").is_some());
    }

    #[test]
    fn garbage_does_not_parse() {
        assert_eq!(parse_goto("not a date"), None);
    }
}

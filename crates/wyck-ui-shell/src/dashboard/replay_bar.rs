//! The control bar over the charts while a replay runs: play, step, speed, step size, a custom
//! step, jumping to a date, and back to live.

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use wyck_app::replay::{Phase, Replay, TickMode};
use wyck_chart::{REPLAY_RATES, ReplayStep, TIME_PATTERN};
use wyck_ui_kit::input::{InputEvent, InputState};
use wyck_ui_kit::menu::{Entry, Menu, Placement};
use wyck_ui_kit::prelude::Selectable;
use wyck_ui_kit::{anim, button, controls, icon, theme, tokens};

use super::Dashboard;
use icon::IconName;

impl Dashboard {
    /// The charts, with the replay bar over them while a replay runs.
    pub(super) fn charts_with_replay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let multi = self.multi.clone().into_any_element();
        // Without a replay the charts are laid out exactly as they always were.
        if !self.replay.read(cx).is_on() {
            return multi;
        }
        let bar = self.replay_bar(window, cx);
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .child(bar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(multi),
            )
            .into_any_element()
    }

    fn replay_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.replay_goto.is_none() {
            let state =
                cx.new(|cx| InputState::new(window, cx).placeholder("Go to 2025-03-14 09:30"));
            cx.subscribe_in(
                &state,
                window,
                |this, _state, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.replay_go_to_date(cx);
                    }
                },
            )
            .detach();
            self.replay_goto = Some(state);
        }
        if self.replay_step_field.is_none() {
            let state = cx.new(|cx| InputState::new(window, cx).placeholder("Custom: 7 min"));
            cx.subscribe_in(
                &state,
                window,
                |this, state, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.replay_set_custom_step(state, window, cx);
                    }
                },
            )
            .detach();
            self.replay_step_field = Some(state);
        }
        let replay = self.replay.clone();
        let (
            active,
            playing,
            buffering,
            rate,
            step,
            step_ms,
            cursor,
            skip_gaps,
            future,
            ticks,
            mode,
        ) = {
            let held = replay.read(cx);
            (
                held.phase() == Phase::Active,
                held.is_playing(),
                held.is_buffering(),
                held.rate_hz(),
                held.step(),
                held.step_ms(),
                held.cursor_ms(),
                held.skips_gaps(),
                held.shows_future_drawings(),
                held.ticks_available(),
                held.tick_mode(),
            )
        };
        let zone = self.multi.read(cx).active_chart().read(cx).settings().zone;
        let time = cursor.map_or_else(String::new, |ms| zone.format(ms, TIME_PATTERN));

        let rate_menu = Menu::new("replay-rate-menu", window, cx);
        let step_menu = Menu::new("replay-step-menu", window, cx);
        let rate_items = if rate_menu.is_open(cx) {
            REPLAY_RATES
                .iter()
                .map(|hz| {
                    let hz = *hz;
                    let replay = replay.clone();
                    Entry::new(rate_label(hz))
                        .checked((hz - rate).abs() < 1e-9)
                        .on_click(move |_, cx| replay.update(cx, |r, cx| r.set_rate(hz, cx)))
                        .into()
                })
                .collect()
        } else {
            Vec::new()
        };
        let step_items = if step_menu.is_open(cx) {
            let tick_steps = [
                ReplayStep::Ticks(1),
                ReplayStep::Ticks(10),
                ReplayStep::Ticks(100),
            ];
            ReplayStep::PRESETS
                .iter()
                .chain(tick_steps.iter().filter(|_| ticks))
                .map(|choice| {
                    let choice = *choice;
                    let replay = replay.clone();
                    Entry::new(choice.label())
                        .checked(choice == step)
                        .on_click(move |_, cx| replay.update(cx, |r, cx| r.set_step(choice, cx)))
                        .into()
                })
                .collect()
        } else {
            Vec::new()
        };
        let toggle_rate = rate_menu.clone();
        let toggle_step = step_menu.clone();

        let control = |id: &'static str, glyph: IconName, tip: &'static str| {
            button::quiet(id).icon(glyph).tooltip(tip)
        };
        let to_start = {
            let replay = replay.clone();
            control("replay-restart", IconName::SkipBack, "Back to the start")
                .on_click(move |_, _, cx| replay.update(cx, Replay::restart))
        };
        let back = {
            let replay = replay.clone();
            control("replay-back", IconName::StepBack, "One step back")
                .on_click(move |_, _, cx| replay.update(cx, Replay::step_back))
        };
        let play = {
            let replay = replay.clone();
            control(
                "replay-play",
                if playing {
                    IconName::Pause
                } else {
                    IconName::Play
                },
                if playing { "Pause" } else { "Play" },
            )
            .selected(playing)
            .on_click(move |_, _, cx| replay.update(cx, Replay::toggle))
        };
        let forward = {
            let replay = replay.clone();
            control("replay-forward", IconName::StepForward, "One step forward")
                .on_click(move |_, _, cx| replay.update(cx, Replay::step_forward))
        };
        let exit = {
            let replay = replay.clone();
            button::quiet("replay-exit")
                .icon(IconName::X)
                .label("Back to live")
                .tooltip("Stop the replay and go back to the live chart")
                .on_click(move |_, _, cx| replay.update(cx, Replay::exit))
        };
        let switch = |id: &'static str,
                      label: &'static str,
                      on: bool,
                      change: fn(&mut Replay, bool, &mut Context<Replay>)| {
            let replay = replay.clone();
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .text_size(px(tokens::text::small()))
                .text_color(theme::muted_fg())
                .child(controls::toggle(id, on, move |on, _, cx| {
                    replay.update(cx, |r, cx| change(r, on, cx));
                }))
                .child(label)
        };
        let status: AnyElement = if buffering || !active {
            anim::spin(
                icon::tinted(IconName::LoaderCircle, 14., theme::muted_fg()),
                "replay-buffering",
            )
            .into_any_element()
        } else {
            div().w(px(14.0)).into_any_element()
        };

        div()
            .flex_none()
            .w_full()
            .h(px(tokens::bar::header()))
            .px_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(theme::border_hairline())
            .bg(theme::bg())
            .child(
                div()
                    .text_size(px(tokens::text::small()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::accent())
                    .child("REPLAY"),
            )
            .child(to_start)
            .child(back)
            .child(play)
            .child(forward)
            .child(status)
            .child(
                div()
                    .relative()
                    .child(
                        button::quiet("replay-rate")
                            .icon(IconName::Gauge)
                            .label(SharedString::from(rate_label(rate)))
                            .tooltip("How many updates a second")
                            .on_click(move |_, _, cx| toggle_rate.toggle(cx)),
                    )
                    .children(rate_menu.popup(
                        rate_items,
                        Placement::Below(tokens::height::compact()),
                        window,
                        cx,
                    )),
            )
            .child(
                div()
                    .relative()
                    .child(
                        button::quiet("replay-step")
                            .icon(IconName::Timer)
                            .label(SharedString::from(step_label(step, step_ms)))
                            .tooltip("How much time one update covers")
                            .on_click(move |_, _, cx| toggle_step.toggle(cx)),
                    )
                    .children(step_menu.popup(
                        step_items,
                        Placement::Below(tokens::height::compact()),
                        window,
                        cx,
                    )),
            )
            .children(
                self.replay_step_field
                    .as_ref()
                    .map(|field| div().w(px(120.0)).child(wyck_ui_kit::input::dense(field))),
            )
            .child(
                div()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::fg())
                    .child(time),
            )
            .child(div().flex_1().min_w_0())
            .child(switch(
                "replay-skip",
                "Skip closed hours",
                skip_gaps,
                Replay::set_skip_gaps,
            ))
            .child(switch(
                "replay-ticks",
                "Real ticks",
                mode == TickMode::Auto,
                |replay, on, cx| {
                    replay.set_tick_mode(
                        if on {
                            TickMode::Auto
                        } else {
                            TickMode::Minutes
                        },
                        cx,
                    );
                },
            ))
            .child(switch(
                "replay-future",
                "Drawings from the future",
                future,
                Replay::set_future_drawings,
            ))
            .children(
                self.replay_goto
                    .as_ref()
                    .map(|field| div().w(px(170.0)).child(wyck_ui_kit::input::dense(field))),
            )
            .child(
                button::quiet("replay-goto")
                    .icon(IconName::CalendarClock)
                    .tooltip("Go to the date and time typed")
                    .on_click(cx.listener(|this, _, _, cx| this.replay_go_to_date(cx))),
            )
            .child(exit)
            .into_any_element()
    }
}

fn rate_label(hz: f64) -> String {
    format!("{hz} updates/s")
}

fn step_label(step: ReplayStep, step_ms: i64) -> String {
    if step == ReplayStep::Auto {
        let unit = if step_ms % 60_000 == 0 {
            format!("{} min", step_ms / 60_000)
        } else {
            format!("{} s", step_ms / 1_000)
        };
        format!("Step: auto ({unit})")
    } else {
        format!("Step: {}", step.label())
    }
}

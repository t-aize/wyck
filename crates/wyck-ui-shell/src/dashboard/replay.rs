//! The replay as the dashboard shows it: the button in the header and its menu, the control bar
//! over the charts, and what a running replay forbids (orders and alerts use the live prices).
//!
//! Nothing here decides how a replay behaves: that is `wyck_app::replay::Replay`, which this only
//! drives and reads.

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use wyck_app::replay::{Phase, Replay, ReplayEvent};
use wyck_chart::{REPLAY_RATES, ReplayStep, TIME_PATTERN};
use wyck_ui_kit::input::{InputEvent, InputState};
use wyck_ui_kit::menu::{self as popup, Entry, Menu, Placement};
use wyck_ui_kit::prelude::Selectable;
use wyck_ui_kit::{anim, button, controls, icon, theme, toast, tokens};

use super::{Dashboard, MenuKind};
use icon::IconName;

/// The width of the menu of the header button.
const MENU_WIDTH: f32 = 300.0;

impl Dashboard {
    /// Whether a replay runs. If so, says why the order, alert or line the user asked for is not
    /// done: a replay shows the past, and orders and alerts use the live prices.
    pub(super) fn replay_blocks(&self, cx: &mut Context<Self>) -> bool {
        if !self.replay.read(cx).is_on() {
            return false;
        }
        toast::Toast::info(
            "Replay is on",
            "Orders and alerts use the live prices, not the replayed ones.",
        )
        .hint("Stop the replay to trade or add an alert.")
        .show(cx);
        true
    }

    // ---- Starting and stopping.

    /// Makes the replay hold what the charts show, then runs `start`.
    fn replay_start(
        &mut self,
        cx: &mut Context<Self>,
        start: impl FnOnce(&mut Replay, &mut Context<Replay>),
    ) {
        self.close_menus();
        // Nothing new is sent while a replay runs; an order on its way is let through first.
        if !self
            .trading
            .update(cx, |account, cx| account.set_replay(true, cx))
        {
            toast::Toast::warning(
                "An order is being sent",
                "Wait for its answer, then start the replay.",
            )
            .show(cx);
            return;
        }
        self.multi
            .update(cx, |multi, cx| multi.publish_replay_demand(cx));
        self.replay.update(cx, start);
        cx.notify();
    }

    /// The replay ended or the choice of its start was given up: orders can be sent again.
    pub(super) fn on_replay_event(&mut self, event: &ReplayEvent, cx: &mut Context<Self>) {
        if matches!(event, ReplayEvent::Exited | ReplayEvent::PickCancelled) {
            self.trading.update(cx, |account, cx| {
                account.set_replay(false, cx);
            });
        }
        cx.notify();
    }

    fn replay_start_date(&mut self, cx: &mut Context<Self>) {
        let Some(ms) = self.parse_replay_date(self.replay_date.clone(), cx) else {
            return;
        };
        self.replay_start(cx, |replay, cx| replay.start_at(ms, cx));
    }

    fn replay_go_to_date(&mut self, cx: &mut Context<Self>) {
        let Some(ms) = self.parse_replay_date(self.replay_goto.clone(), cx) else {
            return;
        };
        self.replay.update(cx, |replay, cx| replay.go_to(ms, cx));
    }

    /// The date typed in `field`, read in the time zone of the active chart.
    fn parse_replay_date(
        &self,
        field: Option<gpui::Entity<InputState>>,
        cx: &mut Context<Self>,
    ) -> Option<i64> {
        let text = field?.read(cx).value().trim().to_owned();
        let zone = self.multi.read(cx).active_chart().read(cx).settings().zone;
        let parsed = zone.parse(&text);
        if parsed.is_none() {
            toast::Toast::warning(
                "Not a date",
                format!("\"{text}\" is not a date and time. Write it like 2025-03-14 09:30."),
            )
            .show(cx);
        }
        parsed
    }

    /// The header button: opens the menu to start, or stops the replay that runs.
    fn replay_button_clicked(&mut self, cx: &mut Context<Self>) {
        match self.replay.read(cx).phase() {
            Phase::Off => self.toggle_menu(MenuKind::Replay, cx),
            Phase::Picking => self.replay.update(cx, Replay::cancel_pick),
            Phase::Preparing | Phase::Active => {
                self.close_menus();
                self.replay.update(cx, Replay::exit);
            }
        }
        cx.notify();
    }

    // ---- The header.

    /// The button that starts and stops a replay, and its menu.
    pub(super) fn replay_button(
        &mut self,
        labels: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let phase = self.replay.read(cx).phase();
        let open = self.menu_is(MenuKind::Replay);
        if open && self.replay_date.is_none() {
            let state = cx.new(|cx| InputState::new(window, cx).placeholder("2025-03-14 09:30"));
            cx.subscribe_in(
                &state,
                window,
                |this, _state, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.replay_start_date(cx);
                    }
                },
            )
            .detach();
            self.replay_date = Some(state);
        }
        let tooltip = match phase {
            Phase::Off => "Replay: watch the past unfold",
            Phase::Picking => "Cancel the choice of a start",
            Phase::Preparing | Phase::Active => "Stop the replay",
        };
        let mut replay_button = button::quiet("header-replay")
            .icon(IconName::Rewind)
            .selected(phase != Phase::Off || open)
            .tooltip(tooltip)
            .on_click(cx.listener(|this, _, _, cx| this.replay_button_clicked(cx)));
        if labels {
            replay_button = replay_button.label("Replay");
        }
        div()
            .relative()
            .flex_none()
            .child(replay_button)
            .children(open.then(|| self.replay_menu(cx)))
    }

    fn replay_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let row = |id: &'static str, glyph: IconName, title: &'static str, hint: &'static str| {
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .px_2()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .hover(|style| style.bg(theme::surface_hover()))
                .child(icon::tinted(glyph, 16., theme::muted_fg()))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(tokens::text::emphasis()))
                                .text_color(theme::fg())
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(tokens::text::small()))
                                .text_color(theme::muted_fg())
                                .child(hint),
                        ),
                )
        };
        let date = self.replay_date.as_ref().map(|field| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .child(div().flex_1().child(wyck_ui_kit::input::dense(field)))
                .child(
                    button::accent("replay-start-date")
                        .label("Start")
                        .on_click(cx.listener(|this, _, _, cx| this.replay_start_date(cx))),
                )
        });
        let card = popup::parts::panel(MENU_WIDTH)
            .id("replay-menu")
            .gap_1()
            .child(popup::parts::section_title("Replay from").px_0())
            .child(
                row(
                    "replay-pick",
                    IconName::Scissors,
                    "A bar on the chart",
                    "Click the candle to start from",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.replay_start(cx, Replay::begin_pick);
                })),
            )
            .child(
                row(
                    "replay-random",
                    IconName::Shuffle,
                    "A random bar",
                    "Somewhere in the available history",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.replay_start(cx, Replay::start_random);
                })),
            )
            .child(
                row(
                    "replay-first",
                    IconName::SkipBack,
                    "The first available bar",
                    "The oldest data every chart has",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.replay_start(cx, Replay::start_first);
                })),
            )
            .child(popup::parts::section_title("Or from a date and time").px_0())
            .children(date)
            .child(
                div()
                    .px_2()
                    .text_size(px(tokens::text::small()))
                    .text_color(theme::muted_fg())
                    .child(
                        "In the time zone of the chart. Nothing is traded: it only replays prices.",
                    ),
            );
        popup::parts::below(card, tokens::height::control(), 1)
    }

    // ---- The bar under the charts.

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
        let replay = self.replay.clone();
        let (active, playing, buffering, rate, step, step_ms, cursor, skip_gaps, future) = {
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
            ReplayStep::ALL
                .iter()
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
        format!("Step: auto ({} min)", step_ms / 60_000)
    } else {
        format!("Step: {}", step.label())
    }
}

impl Dashboard {
    /// The keys of the replay. Kept out of `render`, whose frame grows with every handler.
    #[inline(never)]
    pub(super) fn replay_actions(root: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        use crate::actions as a;
        root.on_action(cx.listener(|this, _: &a::ReplayToggle, _window, cx| {
            this.replay_toggle(cx);
        }))
        .on_action(cx.listener(|this, _: &a::ReplayPlayPause, _window, cx| {
            this.replay.update(cx, Replay::toggle);
        }))
        .on_action(cx.listener(|this, _: &a::ReplayStepBack, _window, cx| {
            this.replay.update(cx, Replay::step_back);
        }))
        .on_action(cx.listener(|this, _: &a::ReplayStepForward, _window, cx| {
            this.replay.update(cx, Replay::step_forward);
        }))
        .on_action(cx.listener(|this, _: &a::ReplayFaster, _window, cx| {
            this.replay_rate_step(true, cx);
        }))
        .on_action(cx.listener(|this, _: &a::ReplaySlower, _window, cx| {
            this.replay_rate_step(false, cx);
        }))
    }

    /// The key that starts a replay (by choosing the bar on a chart) or stops the one that runs.
    fn replay_toggle(&mut self, cx: &mut Context<Self>) {
        match self.replay.read(cx).phase() {
            Phase::Off => self.replay_start(cx, Replay::begin_pick),
            Phase::Picking => self.replay.update(cx, Replay::cancel_pick),
            Phase::Preparing | Phase::Active => self.replay.update(cx, Replay::exit),
        }
    }

    /// The next faster or slower speed on the menu.
    fn replay_rate_step(&mut self, faster: bool, cx: &mut Context<Self>) {
        let rate = self.replay.read(cx).rate_hz();
        let next = if faster {
            REPLAY_RATES.iter().copied().find(|hz| *hz > rate + 1e-9)
        } else {
            REPLAY_RATES
                .iter()
                .rev()
                .copied()
                .find(|hz| *hz < rate - 1e-9)
        };
        if let Some(next) = next {
            self.replay
                .update(cx, |replay, cx| replay.set_rate(next, cx));
        }
    }
}

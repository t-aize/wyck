//! The replay as the dashboard shows it: the button in the header and its menu, the control bar
//! over the charts, and what a running replay forbids (orders and alerts use the live prices).
//!
//! Nothing here decides how a replay behaves: that is `wyck_app::replay::Replay`, which this only
//! drives and reads.

use crate::kit::input::{InputEvent, InputState};
use crate::kit::menu as popup;
use crate::kit::prelude::Selectable;
use crate::kit::{button, icon, theme, toast, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Window, div, px};
use wyck_app::replay::{Phase, Replay, ReplayEvent};
use wyck_domain::chart::{REPLAY_RATES, REPLAY_STEP_HELP, parse_step};

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

    pub(super) fn replay_go_to_date(&mut self, cx: &mut Context<Self>) {
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
                .child(div().flex_1().child(crate::kit::input::dense(field)))
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
}

impl Dashboard {
    /// The keys of the replay. Kept out of `render`, whose frame grows with every handler.
    #[inline(never)]
    pub(super) fn replay_actions(root: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        use crate::shell::actions as a;
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

impl Dashboard {
    /// Reads the step typed in the custom field and takes it, or says what is allowed.
    pub(super) fn replay_set_custom_step(
        &mut self,
        field: &gpui::Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = field.read(cx).value().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let Some(step) = parse_step(&text) else {
            toast::Toast::warning("Not a step", format!("\"{text}\" is not a step."))
                .hint(REPLAY_STEP_HELP)
                .show(cx);
            return;
        };
        if step.ticks().is_some() && !self.replay.read(cx).ticks_available() {
            toast::Toast::warning(
                "No tick data",
                "A step in ticks needs the real ticks, which this replay does not hold.",
            )
            .show(cx);
            return;
        }
        self.replay
            .update(cx, |replay, cx| replay.set_step(step, cx));
        field.update(cx, |state, cx| state.set_value("", window, cx));
    }
}
